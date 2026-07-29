//! Sidecar 可执行入口：默认 serve（stdio JSONL 服务）；
//! `cred set <ref>` 从 stdin 读密钥写入凭据存储（stdout 只回执成功/失败，不回显内容）；
//! `cred check <ref>` 输出引用存在性。契约见 docs/module-contracts.md 第 6 节。

mod dispatch;
mod fs;
mod git;
mod mapping;
mod server;
mod state;
mod task_flow;

use std::io::BufRead;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::unbounded;
use halo_config::{CredentialStore, Secret, WindowsCredentialStore};
use halo_runtime::Timeouts;
use halo_store::{Store, StoreLimits};
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("serve") => serve(),
        Some("cred") => cred_cli(&args[1..]),
        Some(other) => {
            eprintln!("未知子命令：{other}");
            eprintln!("用法：halo-sidecar [serve] | cred set <ref> | cred check <ref>");
            2
        }
    };
    std::process::exit(code);
}

/// 数据目录：HALO_DATA_DIR 覆盖，默认 %LOCALAPPDATA%\HaloStudio。
fn data_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("HALO_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    match std::env::var("LOCALAPPDATA") {
        Ok(base) if !base.trim().is_empty() => Ok(PathBuf::from(base).join("HaloStudio")),
        _ => Err("无法确定数据目录：LOCALAPPDATA 不可用且未设置 HALO_DATA_DIR".to_string()),
    }
}

/// 超时配置：环境变量注入毫秒值（供测试注入真实超时，不是模拟开关）。
fn timeouts_from_env() -> Timeouts {
    let ms = |name: &str, default_ms: u64| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(default_ms))
    };
    Timeouts {
        ready: ms("HALO_READY_TIMEOUT_MS", 10_000),
        cancel_grace: ms("HALO_CANCEL_GRACE_MS", 10_000),
        shutdown_grace: ms("HALO_SHUTDOWN_GRACE_MS", 5_000),
    }
}

fn serve() -> i32 {
    let dir = match data_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[halo-sidecar] {e}");
            return 1;
        }
    };
    let store = match Store::open(&dir.join("halo.db"), StoreLimits::default()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[halo-sidecar] 本地存储打开失败：{e}");
            return 1;
        }
    };
    // 启动恢复：非终态任务如实标记 interrupted，不自动恢复或重放
    match store.mark_non_terminal_interrupted() {
        Ok(ids) if !ids.is_empty() => {
            eprintln!("[halo-sidecar] 启动恢复：{} 个非终态任务已标记 interrupted", ids.len());
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("[halo-sidecar] 启动恢复失败：{e}");
            return 1;
        }
    }

    let timeouts = timeouts_from_env();
    let (out_tx, out_rx) = unbounded();
    let writer = server::spawn_writer(std::io::stdout(), out_rx);
    let bus = Arc::new(server::EventBus::new(out_tx));

    // 启动后首条事件
    bus.emit(
        None,
        "sidecar.state",
        json!({"state": "ready", "protocol_version": halo_protocol::PROTOCOL_VERSION}),
    );

    let app = Arc::new(Mutex::new(state::AppState::new()));
    let ctx = dispatch::Ctx {
        store: Arc::new(store),
        cred: Arc::new(WindowsCredentialStore::new()),
        bus: Arc::clone(&bus),
        app: Arc::clone(&app),
        timeouts,
    };
    let store_ref = Arc::clone(&ctx.store);
    let mut dispatcher = dispatch::Dispatcher::new(ctx);

    let stdin = std::io::stdin();
    server::read_loop(stdin.lock(), &mut dispatcher);

    // UI 关闭（stdin EOF）：如实标记未完成任务并停止受管运行时
    shutdown(&app, &bus, &store_ref, timeouts);
    drop(dispatcher);
    drop(bus);
    let _ = writer.join();
    0
}

fn shutdown(
    app: &Arc<Mutex<state::AppState>>,
    bus: &Arc<server::EventBus>,
    store: &Arc<Store>,
    timeouts: Timeouts,
) {
    // 先设置关闭闸门并移除内存活动任务，防止停止运行时产生的迟到事件再次
    // 触碰会话、证据或公开事件；运行时路由随后由 stop_slot 统一切断。
    let record = {
        let mut guard = state::lock(app);
        guard.shutting_down = true;
        guard.task.take().and_then(|mut task| {
            task.session_messages.clear();
            task.action_requests.clear();
            task.cancel_tx = None;
            task.finish_tx = None;
            if task.state.is_terminal() {
                return None;
            }
            match task.state.apply(&halo_core::TaskEvent::MarkInterrupted) {
                Ok(next) => {
                    task.state = next;
                    task.ended_at = Some(mapping::now_ts());
                    Some(task.to_record())
                }
                Err(_) => None,
            }
        })
    };
    for agent in [halo_config::AgentKind::Pi, halo_config::AgentKind::OpenCode] {
        state::stop_slot(app, bus, agent, timeouts.shutdown_grace);
    }
    if let Some(rec) = record {
        if store.put_task(&rec).is_err() {
            eprintln!("[halo-sidecar] 退出时任务标记失败");
        }
    }
    // 兜底处理已经持久化但不再挂在当前内存路由上的非终态任务。
    if store.mark_non_terminal_interrupted().is_err() {
        eprintln!("[halo-sidecar] 退出时中断收口失败");
    }
}

/// cred 子命令：录入与检查凭据引用。任何输出都不回显密钥内容。
fn cred_cli(rest: &[String]) -> i32 {
    let usage = || {
        eprintln!("用法：halo-sidecar cred set <ref>   （密钥经 stdin 传入）");
        eprintln!("      halo-sidecar cred check <ref>");
        2
    };
    let (action, ref_name) = match (rest.first(), rest.get(1)) {
        (Some(a), Some(r)) if !r.trim().is_empty() => (a.as_str(), r.as_str()),
        _ => return usage(),
    };
    let store = WindowsCredentialStore::new();
    match action {
        "set" => {
            if !store.available() {
                eprintln!("失败：操作系统凭据存储不可用（失败关闭，不回退明文文件）");
                return 1;
            }
            let mut line = String::new();
            if std::io::stdin().lock().read_line(&mut line).is_err() {
                eprintln!("失败：无法从标准输入读取密钥");
                return 1;
            }
            let secret = {
                let value = line.trim_end_matches(['\r', '\n']);
                if value.is_empty() {
                    eprintln!("失败：标准输入未提供密钥内容");
                    return 1;
                }
                Secret::new(value)
            };
            // 明文已复制进 Secret：立即清零行缓冲，缩短明文驻留窗口
            // （尽力而为；0x00 是合法 UTF-8，Secret 自身 Drop 时同样清零）
            unsafe { line.as_mut_vec() }.fill(0);
            match store.set(ref_name, &secret) {
                Ok(()) => {
                    println!("成功：凭据已保存（引用名：{ref_name}）");
                    0
                }
                Err(e) => {
                    eprintln!("失败：{e}");
                    1
                }
            }
        }
        "check" => match store.exists(ref_name) {
            Ok(true) => {
                println!("存在：{ref_name}");
                0
            }
            Ok(false) => {
                println!("不存在：{ref_name}");
                0
            }
            Err(e) => {
                eprintln!("失败：{e}");
                1
            }
        },
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_env_injection_overrides_defaults() {
        // 环境变量注入是真实超时配置，不是模拟开关
        std::env::set_var("HALO_READY_TIMEOUT_MS", "1234");
        std::env::set_var("HALO_CANCEL_GRACE_MS", "10");
        std::env::remove_var("HALO_SHUTDOWN_GRACE_MS");
        let t = timeouts_from_env();
        assert_eq!(t.ready, Duration::from_millis(1234));
        assert_eq!(t.cancel_grace, Duration::from_millis(10));
        assert_eq!(t.shutdown_grace, Duration::from_millis(5000));
        std::env::remove_var("HALO_READY_TIMEOUT_MS");
        std::env::remove_var("HALO_CANCEL_GRACE_MS");
    }

    #[test]
    fn data_dir_prefers_halo_data_dir() {
        std::env::set_var("HALO_DATA_DIR", "D:\\自定义 数据目录");
        assert_eq!(data_dir().unwrap(), PathBuf::from("D:\\自定义 数据目录"));
        std::env::remove_var("HALO_DATA_DIR");
    }

    #[test]
    fn shutdown_persists_interruption_and_clears_active_session_boundary() {
        let data_dir = tempfile::tempdir().expect("创建测试数据目录失败");
        let store = Arc::new(
            Store::open(
                &data_dir.path().join("halo.db"),
                StoreLimits::default(),
            )
            .expect("打开测试存储失败"),
        );
        let (out_tx, _out_rx) = unbounded();
        let bus = Arc::new(server::EventBus::new(out_tx));
        let app = Arc::new(Mutex::new(state::AppState::new()));
        let (route_tx, _route_rx) = unbounded();
        let mut task = state::ActiveTask {
            task_id: "task-shutdown".to_string(),
            agent: halo_config::AgentKind::OpenCode,
            title: "退出清理测试".to_string(),
            instructions: "活动任务说明".to_string(),
            state: halo_core::TaskState::Running,
            attribution: halo_core::Attribution::AgentOnly,
            manual_edit_paths: Default::default(),
            baseline: halo_core::Baseline {
                head: None,
                tree: "tree".to_string(),
                dirty_files: vec![],
                captured_at: "2026-07-28T00:00:00Z".to_string(),
            },
            created_at: "2026-07-28T00:00:00Z".to_string(),
            ended_at: None,
            cancel_mode: None,
            latest_evidence_version: 0,
            verification_agent: None,
            verification_user: None,
            session_messages: vec![],
            action_requests: Default::default(),
            cancellation_requested: false,
            finish_requested: false,
            cancel_tx: None,
            finish_tx: None,
        };
        task.append_session_message(
            halo_protocol::methods::task::TaskSessionMessageRole::Agent,
            "仅存于活动会话的回复",
        );
        task.action_requests.insert(
            "action-shutdown".to_string(),
            halo_protocol::methods::task::TaskActionRequest {
                request_id: "action-shutdown".to_string(),
                kind: halo_protocol::methods::task::TaskActionKind::Permission,
                prompt: "仅存于活动会话的请求".to_string(),
                decision_sent: false,
            },
        );
        store.put_task(&task.to_record()).expect("写入测试任务失败");
        {
            let mut guard = state::lock(&app);
            guard.task = Some(task);
            guard.slot_mut(halo_config::AgentKind::OpenCode).task_tx = Some(route_tx);
        }

        shutdown(&app, &bus, &store, Timeouts {
            ready: Duration::ZERO,
            cancel_grace: Duration::ZERO,
            shutdown_grace: Duration::ZERO,
        });

        let stored = store
            .get_task("task-shutdown")
            .expect("读取测试任务失败")
            .expect("中断任务应保留");
        assert_eq!(stored.state, "interrupted");
        let guard = state::lock(&app);
        assert!(guard.task.is_none(), "退出后不得保留活动任务路由");
        assert!(guard
            .slot(halo_config::AgentKind::OpenCode)
            .task_tx
            .is_none());
    }
}
