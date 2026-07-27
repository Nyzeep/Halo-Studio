//! Sidecar 可执行入口：默认 serve（stdio JSONL 服务）；
//! `cred set <ref>` 从 stdin 读密钥写入凭据存储（stdout 只回执成功/失败，不回显内容）；
//! `cred check <ref>` 输出引用存在性。契约见 docs/module-contracts.md 第 6 节。

mod dispatch;
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
    // 当前非终态任务 → interrupted（不自动恢复或重放）
    let record = {
        let mut guard = state::lock(app);
        match guard.task.as_mut() {
            Some(task) if !task.state.is_terminal() => {
                match task.state.apply(&halo_core::TaskEvent::MarkInterrupted) {
                    Ok(next) => {
                        task.state = next;
                        task.ended_at = Some(mapping::now_ts());
                        Some(task.to_record())
                    }
                    Err(_) => None,
                }
            }
            _ => None,
        }
    };
    if let Some(rec) = record {
        if let Err(e) = store.put_task(&rec) {
            eprintln!("[halo-sidecar] 退出时任务标记失败：{e}");
        }
    }
    for agent in [halo_config::AgentKind::Pi, halo_config::AgentKind::OpenCode] {
        state::stop_slot(app, bus, agent, timeouts.shutdown_grace);
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
}
