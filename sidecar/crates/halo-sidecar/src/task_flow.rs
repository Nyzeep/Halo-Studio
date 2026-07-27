//! 任务编排：前置校验之后的完整生命周期——记基线 → 交给运行时 → RuntimeEvent
//! 规范化为契约事件 → 终态 → 关联变更取证（sanitize + cap）→ 证据落库 → 收尾事件。
//!
//! 状态迁移一律经 halo_core::TaskState::apply 驱动，编排层不得私设状态。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender};
use serde_json::{json, Value};

use halo_config::AgentKind;
use halo_core::{cap, limits, sanitize, TaskEvent, TaskState, Verification};
use halo_runtime::{RunTaskSpec, RuntimeEvent, RuntimeState, Timeouts};
use halo_store::Store;

use crate::dispatch::SidecarError;
use crate::git::GitClient;
use crate::mapping::{
    attribution_core_to_str, attribution_reasons, now_ts, verification_source_core_to_str,
    verification_status_core_to_str,
};
use crate::server::EventBus;
use crate::state::{lock, ActiveTask, AgentHandle, AppState};

/// 编排上下文：全部为共享句柄，可克隆进任务线程。
#[derive(Clone)]
pub struct FlowCtx {
    pub bus: Arc<EventBus>,
    pub store: Arc<Store>,
    pub app: Arc<Mutex<AppState>>,
    pub timeouts: Timeouts,
}

/// 任务启动入参（前置校验已由 dispatch 完成）。
pub struct StartArgs {
    pub agent: AgentKind,
    pub title: String,
    pub instructions: String,
    pub files: Vec<String>,
    pub base_diff: Option<String>,
    pub notes: Option<String>,
}

/// 记基线、建任务、交给运行时并启动编排线程；返回 running 状态的 TaskStatus DTO。
pub fn start_task(
    ctx: &FlowCtx,
    git_root: &str,
    handle: Arc<dyn AgentHandle>,
    args: StartArgs,
) -> Result<halo_protocol::methods::task::TaskStatus, SidecarError> {
    let git = GitClient::new(git_root);

    // 1. 任务基线：HEAD + 临时索引树 + 脏文件清单（基线前修改永不归因 Agent）
    let head = git.head()?;
    let tree = git.capture_tree()?;
    let dirty_files = git.status_dirty_files()?;
    let baseline = halo_core::Baseline {
        head,
        tree,
        dirty_files,
        captured_at: now_ts(),
    };

    let task_id = format!("task-{}", uuid::Uuid::new_v4());
    let (task_tx, task_rx) = unbounded::<RuntimeEvent>();
    let (cancel_tx, cancel_rx) = bounded::<()>(4);

    let task = ActiveTask {
        task_id: task_id.clone(),
        agent: args.agent,
        title: args.title.clone(),
        instructions: args.instructions.clone(),
        state: TaskState::Created,
        attribution: halo_core::Attribution::AgentOnly,
        baseline,
        created_at: now_ts(),
        ended_at: None,
        cancel_mode: None,
        latest_evidence_version: 0,
        verification_agent: None,
        verification_user: None,
        cancel_tx: Some(cancel_tx),
    };

    let created_status = task.to_status();
    {
        let mut app = lock(&ctx.app);
        app.slot_mut(args.agent).task_tx = Some(task_tx);
        app.task = Some(task);
    }
    persist_current(ctx, &task_id);
    emit_task_state(ctx, &task_id, &created_status);

    // 2. 组 RunTaskSpec 交给运行时
    let spec = RunTaskSpec {
        instructions: args.instructions,
        files: args.files,
        base_diff: args.base_diff,
        notes: args.notes,
    };
    if let Err(e) = handle.run_task(&spec) {
        apply_event(
            ctx,
            &task_id,
            &TaskEvent::Fail("无法把任务提交给受管运行时".to_string()),
        );
        clear_route(ctx, args.agent);
        return Err(SidecarError::from(e));
    }

    // 3. Created → Running
    apply_event(ctx, &task_id, &TaskEvent::Started);

    // 4. 编排线程接管事件流
    let thread_ctx = ctx.clone();
    let thread_git = GitClient::new(PathBuf::from(git_root));
    let thread_task_id = task_id.clone();
    std::thread::spawn(move || {
        run_task_loop(
            &thread_ctx,
            &thread_git,
            &thread_task_id,
            args.agent,
            handle,
            task_rx,
            cancel_rx,
        );
    });

    let status = current_status(ctx, &task_id)
        .ok_or_else(|| SidecarError::internal("任务创建后状态丢失"))?;
    Ok(status)
}

/// 任务事件主循环：规范化运行时事件、驱动状态机、处理取消，直至终局。
pub fn run_task_loop(
    ctx: &FlowCtx,
    git: &GitClient,
    task_id: &str,
    agent: AgentKind,
    handle: Arc<dyn AgentHandle>,
    events_rx: Receiver<RuntimeEvent>,
    cancel_rx: Receiver<()>,
) {
    enum Ending {
        Done { outcome: String, summary: String },
        Cancelled { mode: &'static str },
        RuntimeFailed { reason: String },
    }

    let ending = loop {
        crossbeam_channel::select! {
            recv(events_rx) -> msg => match msg {
                Err(_) => break Ending::RuntimeFailed { reason: "运行时事件通道意外中断".to_string() },
                Ok(RuntimeEvent::State(RuntimeState::Failed { reason, .. })) =>
                    break Ending::RuntimeFailed { reason },
                Ok(RuntimeEvent::State(RuntimeState::Stopped)) =>
                    break Ending::RuntimeFailed { reason: "受管运行时在任务结束前停止".to_string() },
                Ok(RuntimeEvent::State(_)) => {}
                Ok(RuntimeEvent::Trace(item)) => on_trace(ctx, task_id, &item),
                Ok(RuntimeEvent::ActionRequest { request_id, kind, prompt }) =>
                    on_action_request(ctx, task_id, &request_id, &kind, &prompt),
                Ok(RuntimeEvent::Verification { status, detail }) =>
                    on_verification(ctx, task_id, &status, &detail),
                Ok(RuntimeEvent::TaskDone { outcome, summary }) =>
                    break Ending::Done { outcome, summary },
            },
            recv(cancel_rx) -> msg => {
                if msg.is_err() {
                    // 取消发送端消失属于异常内部状态；任务照常继续
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                handle.cancel_native();
                let deadline = Instant::now() + ctx.timeouts.cancel_grace;
                let mode = loop {
                    match events_rx.recv_deadline(deadline) {
                        Ok(RuntimeEvent::TaskDone { .. }) => break "native",
                        Ok(RuntimeEvent::State(RuntimeState::Stopped))
                        | Ok(RuntimeEvent::State(RuntimeState::Failed { .. })) => break "native",
                        Ok(_) => continue,
                        Err(RecvTimeoutError::Timeout) => {
                            // 宽限期内未原生退出：强杀
                            handle.stop(Duration::ZERO);
                            break "forced";
                        }
                        Err(RecvTimeoutError::Disconnected) => break "native",
                    }
                };
                break Ending::Cancelled { mode };
            }
        }
    };

    match ending {
        Ending::Done { outcome, summary } => {
            if outcome == "finished" {
                resolve_pending_action(ctx, task_id);
                apply_event(ctx, task_id, &TaskEvent::Finishing);
                match append_evidence(ctx, git, task_id, "finished", &summary) {
                    Ok(version) => {
                        apply_event(ctx, task_id, &TaskEvent::EvidenceReady);
                        ctx.bus.emit(
                            Some(task_id),
                            "task.finished",
                            json!({"outcome": "finished", "evidence_version": version}),
                        );
                    }
                    Err(()) => mark_evidence_persistence_failure(ctx, task_id),
                }
            } else {
                apply_event(ctx, task_id, &TaskEvent::Fail(summary.clone()));
                match append_evidence(ctx, git, task_id, "failed", &summary) {
                    Ok(version) => {
                        ctx.bus.emit(
                            Some(task_id),
                            "task.finished",
                            json!({"outcome": "failed", "evidence_version": version}),
                        );
                    }
                    Err(()) => mark_evidence_persistence_failure(ctx, task_id),
                }
            }
        }
        Ending::RuntimeFailed { reason } => {
            apply_event(ctx, task_id, &TaskEvent::Fail(reason.clone()));
            match append_evidence(ctx, git, task_id, "failed", &reason) {
                Ok(version) => {
                    ctx.bus.emit(
                        Some(task_id),
                        "task.finished",
                        json!({"outcome": "failed", "evidence_version": version}),
                    );
                }
                Err(()) => mark_evidence_persistence_failure(ctx, task_id),
            }
        }
        Ending::Cancelled { mode } => {
            let ev = if mode == "forced" {
                TaskEvent::CancelledForced
            } else {
                TaskEvent::CancelledNative
            };
            apply_event(ctx, task_id, &ev);
            let _ = append_evidence(ctx, git, task_id, "cancelled", "任务被本地开发者取消");
            ctx.bus
                .emit(Some(task_id), "task.cancelled", json!({"mode": mode}));
        }
    }

    clear_route(ctx, agent);
}

/// 递归脱敏 JSON 树：对每个字符串值过 sanitize 并按 max_bytes 截断，
/// 数字/布尔/null 原样保留。detail 来自 Agent 原生输出的任意 JSON（不可信），
/// 任何进入事件总线或持久化的 JSON 都必须先经此处理（防御纵深）。
pub(crate) fn sanitize_json_strings(value: &Value, max_bytes: usize) -> Value {
    match value {
        Value::String(s) => {
            let (text, _) = cap(&sanitize(s), max_bytes);
            Value::String(text)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| sanitize_json_strings(v, max_bytes))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), sanitize_json_strings(v, max_bytes)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// 结构化轨迹条目：脱敏限长后推送 trace.item；phase 另推 task.phase。
fn on_trace(ctx: &FlowCtx, task_id: &str, item: &halo_runtime::RuntimeTraceItem) {
    resolve_pending_action(ctx, task_id);
    let (text, _) = cap(&sanitize(&item.text), limits::TRACE_TEXT_MAX);
    // detail 是 Agent 原生输出的任意 JSON：整树递归脱敏后才允许进入事件 payload
    let detail = sanitize_json_strings(&item.detail, limits::TRACE_TEXT_MAX);
    ctx.bus.emit(
        Some(task_id),
        "trace.item",
        json!({"kind": item.kind, "text": text, "detail": detail}),
    );
    if item.kind == "phase" {
        let phase = detail
            .get("phase")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| text.clone());
        ctx.bus.emit(
            Some(task_id),
            "task.phase",
            json!({"phase": phase, "detail": text}),
        );
    }
}

/// Agent 操作请求：Running → AwaitingAction，等待用户经其原生通道决定。
fn on_action_request(ctx: &FlowCtx, task_id: &str, request_id: &str, kind: &str, prompt: &str) {
    apply_event(ctx, task_id, &TaskEvent::ActionRequested);
    let (prompt, _) = cap(&sanitize(prompt), limits::TRACE_TEXT_MAX);
    ctx.bus.emit(
        Some(task_id),
        "task.action_request",
        json!({"request_id": request_id, "kind": kind, "prompt": prompt, "channel": "native"}),
    );
    ctx.bus.emit(
        Some(task_id),
        "trace.item",
        json!({"kind": "action_request", "text": prompt, "detail": {"request_id": request_id, "kind": kind}}),
    );
}

/// Agent 原生验证结论：记录 + 推送 task.verification（source=agent）。
fn on_verification(ctx: &FlowCtx, task_id: &str, status: &str, detail: &str) {
    resolve_pending_action(ctx, task_id);
    let status = match status {
        "passed" => halo_core::VerificationStatus::Passed,
        "failed" => halo_core::VerificationStatus::Failed,
        _ => halo_core::VerificationStatus::NotRun,
    };
    let (detail, _) = cap(&sanitize(detail), limits::TRACE_TEXT_MAX);
    let verification = Verification::from_agent(status, detail.clone());
    {
        let mut app = lock(&ctx.app);
        if let Some(task) = app.task.as_mut().filter(|t| t.task_id == task_id) {
            task.verification_agent = Some(verification.clone());
        }
    }
    ctx.bus.emit(
        Some(task_id),
        "task.verification",
        json!({
            "status": verification_status_core_to_str(status),
            "detail": detail,
            "source": "agent"
        }),
    );
    ctx.bus.emit(
        Some(task_id),
        "trace.item",
        json!({"kind": "verification", "text": detail, "detail": {"status": verification_status_core_to_str(status)}}),
    );
}

/// Agent 恢复输出即视为操作请求已在原生通道解决：AwaitingAction → Running。
fn resolve_pending_action(ctx: &FlowCtx, task_id: &str) {
    let awaiting = {
        let app = lock(&ctx.app);
        app.task
            .as_ref()
            .map(|t| t.task_id == task_id && t.state == TaskState::AwaitingAction)
            .unwrap_or(false)
    };
    if awaiting {
        apply_event(ctx, task_id, &TaskEvent::ActionResolved);
    }
}

/// 经 core 状态机驱动一次迁移；成功则持久化并推送 task.state。
/// 非法迁移（终态竞态等）如实忽略并记 stderr，不得伪造状态。
pub fn apply_event(ctx: &FlowCtx, task_id: &str, ev: &TaskEvent) -> Option<TaskState> {
    let (record, status, next) = {
        let mut app = lock(&ctx.app);
        let task = app.task.as_mut().filter(|t| t.task_id == task_id)?;
        match task.state.apply(ev) {
            Ok(next) => {
                task.state = next;
                if next.is_terminal() && task.ended_at.is_none() {
                    task.ended_at = Some(now_ts());
                }
                match ev {
                    TaskEvent::CancelledNative => task.cancel_mode = Some("native".to_string()),
                    TaskEvent::CancelledForced => task.cancel_mode = Some("forced".to_string()),
                    _ => {}
                }
                (task.to_record(), task.to_status(), next)
            }
            Err(e) => {
                eprintln!("[halo-sidecar] 忽略非法任务迁移：{e}");
                return None;
            }
        }
    };
    if let Err(e) = ctx.store.put_task(&record) {
        eprintln!("[halo-sidecar] 任务记录写入失败：{e}");
    }
    let state_value = serde_json::to_value(status.state).unwrap_or(Value::Null);
    let task_value = serde_json::to_value(&status).unwrap_or(Value::Null);
    ctx.bus.emit(
        Some(task_id),
        "task.state",
        json!({"state": state_value, "task": task_value}),
    );
    Some(next)
}

/// 任务结束取证：再取树 → diff 基线树 → sanitize + cap → append_evidence 落库。
///
/// cap 策略：脱敏后先按"单版本总量上限"做安全截断，再交由 store 按逐项上限
/// 截断并记录 truncated 标记——若在此处直接截到最终上限，store 将无法感知
/// 截断发生，ReviewBundle 的 truncated 字段会失真。
fn append_evidence(
    ctx: &FlowCtx,
    git: &GitClient,
    task_id: &str,
    outcome: &str,
    summary_raw: &str,
) -> Result<u32, ()> {
    let baseline = {
        let app = lock(&ctx.app);
        match app.task.as_ref().filter(|t| t.task_id == task_id) {
            Some(t) => t.baseline.clone(),
            None => return Err(()),
        }
    };

    let mut summary = sanitize(summary_raw);
    let mut files: Vec<halo_store::FileChangeDraft> = Vec::new();
    match git
        .capture_tree()
        .and_then(|end| git.diff_trees(&baseline.tree, &end))
    {
        Ok(diffs) => {
            for d in diffs {
                let (diff, _) = cap(&sanitize(&d.diff), limits::VERSION_TOTAL_MAX);
                files.push(halo_store::FileChangeDraft {
                    path: d.path,
                    change: d.change,
                    diff,
                });
            }
        }
        Err(e) => {
            // 取证失败必须如实呈现，不得假装没有变更。
            // GitError 文本携带 git stderr（不可信外部输出，可能回显含密钥样式的
            // 参数或环境内容）：追加后对整体 summary 再过一次脱敏，之后才 cap。
            summary = sanitize(&format!("{summary}\n【取证失败】无法读取任务关联变更：{e}"));
        }
    }
    let (summary, _) = cap(&summary, limits::VERSION_TOTAL_MAX);

    let (attribution, reasons, verification) = {
        let app = lock(&ctx.app);
        match app.task.as_ref().filter(|t| t.task_id == task_id) {
            Some(t) => {
                let verification = t
                    .verification_agent
                    .clone()
                    .or_else(|| t.verification_user.clone())
                    .unwrap_or_else(|| {
                        Verification::from_agent(
                            halo_core::VerificationStatus::NotRun,
                            "Agent 未报告验证结果",
                        )
                    });
                (
                    attribution_core_to_str(&t.attribution).to_string(),
                    attribution_reasons(&t.attribution),
                    verification,
                )
            }
            None => return Err(()),
        }
    };

    let draft = halo_store::EvidenceDraft {
        outcome: outcome.to_string(),
        attribution,
        attribution_reasons: reasons.iter().map(|r| sanitize(r)).collect(),
        summary,
        files,
        verification_status: verification_status_core_to_str(verification.status).to_string(),
        verification_detail: sanitize(&verification.detail),
        verification_source: verification_source_core_to_str(verification.source).to_string(),
        baseline_dirty_files: baseline.dirty_files.clone(),
        created_at: now_ts(),
    };

    match ctx.store.append_evidence(task_id, &draft) {
        Ok(version) => {
            let mut app = lock(&ctx.app);
            if let Some(task) = app.task.as_mut().filter(|t| t.task_id == task_id) {
                task.latest_evidence_version = version;
            }
            Ok(version)
        }
        Err(_) => {
            // 存储错误可能包含底层实现细节；事件与日志只记录稳定、无敏感信息的事实。
            eprintln!("[halo-sidecar] 交付证据未能写入本地历史");
            Err(())
        }
    }
}

fn mark_evidence_persistence_failure(ctx: &FlowCtx, task_id: &str) {
    apply_event(
        ctx,
        task_id,
        &TaskEvent::Fail("交付证据未能写入本地历史，任务不能进入可审查状态".to_string()),
    );
    ctx.bus.emit(
        Some(task_id),
        "task.finished",
        json!({
            "outcome": "failed",
            "evidence_version": Value::Null,
            "reason": "evidence_persistence_failed",
        }),
    );
}

fn clear_route(ctx: &FlowCtx, agent: AgentKind) {
    let mut app = lock(&ctx.app);
    app.slot_mut(agent).task_tx = None;
}

fn persist_current(ctx: &FlowCtx, task_id: &str) {
    let record = {
        let app = lock(&ctx.app);
        app.task
            .as_ref()
            .filter(|t| t.task_id == task_id)
            .map(|t| t.to_record())
    };
    if let Some(rec) = record {
        if let Err(e) = ctx.store.put_task(&rec) {
            eprintln!("[halo-sidecar] 任务记录写入失败：{e}");
        }
    }
}

fn current_status(
    ctx: &FlowCtx,
    task_id: &str,
) -> Option<halo_protocol::methods::task::TaskStatus> {
    let app = lock(&ctx.app);
    app.task
        .as_ref()
        .filter(|t| t.task_id == task_id)
        .map(|t| t.to_status())
}

fn emit_task_state(
    ctx: &FlowCtx,
    task_id: &str,
    status: &halo_protocol::methods::task::TaskStatus,
) {
    let state_value = serde_json::to_value(status.state).unwrap_or(Value::Null);
    let task_value = serde_json::to_value(status).unwrap_or(Value::Null);
    ctx.bus.emit(
        Some(task_id),
        "task.state",
        json!({"state": state_value, "task": task_value}),
    );
}

/// 发送取消请求给编排线程（由 dispatch task.cancel 调用）。
pub fn request_cancel(cancel_tx: &Sender<()>) -> bool {
    cancel_tx.try_send(()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::Outbound;
    use crossbeam_channel::unbounded;
    use halo_runtime::{RuntimeError, RuntimeTraceItem, StopOutcome};
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // ---- 进程内假运行时（仅测试；生产路径零 mock）----

    struct FakeHandle {
        cancels: AtomicUsize,
        force_stops: AtomicUsize,
        /// cancel_native 时向事件通道回发 TaskDone（模拟原生停止成功）
        done_on_cancel: Option<Sender<RuntimeEvent>>,
    }

    impl FakeHandle {
        fn inert() -> Self {
            FakeHandle {
                cancels: AtomicUsize::new(0),
                force_stops: AtomicUsize::new(0),
                done_on_cancel: None,
            }
        }
    }

    impl AgentHandle for FakeHandle {
        fn run_task(&self, _spec: &RunTaskSpec) -> Result<(), RuntimeError> {
            Ok(())
        }
        fn cancel_native(&self) {
            self.cancels.fetch_add(1, Ordering::SeqCst);
            if let Some(tx) = &self.done_on_cancel {
                let _ = tx.send(RuntimeEvent::TaskDone {
                    outcome: "cancelled".to_string(),
                    summary: "已原生停止".to_string(),
                });
            }
        }
        fn stop(&self, _grace: Duration) -> StopOutcome {
            self.force_stops.fetch_add(1, Ordering::SeqCst);
            StopOutcome::Forced
        }
        fn state(&self) -> RuntimeState {
            RuntimeState::Ready
        }
    }

    // ---- 测试装配 ----

    struct Fixture {
        ctx: FlowCtx,
        repo: PathBuf,
        db_path: PathBuf,
        events: Receiver<Outbound>,
        _store_dir: tempfile::TempDir,
        _repo_dir: tempfile::TempDir,
    }

    fn git(repo: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git 不可用");
        assert!(out.status.success(), "git {args:?} 失败");
    }

    fn fixture() -> Fixture {
        let store_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let repo = repo_dir.path().join("任务 仓库");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        fs::write(repo.join("base.txt"), "基线内容\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-m",
                "init",
                "--no-gpg-sign",
            ],
        );

        let db_path = store_dir.path().join("halo.db");
        let store = Store::open(&db_path, halo_store::StoreLimits::default()).unwrap();
        let (tx, rx) = unbounded();
        let ctx = FlowCtx {
            bus: Arc::new(EventBus::new(tx)),
            store: Arc::new(store),
            app: Arc::new(Mutex::new(AppState::new())),
            timeouts: Timeouts {
                ready: Duration::from_secs(1),
                cancel_grace: Duration::from_millis(200),
                shutdown_grace: Duration::from_millis(200),
            },
        };
        Fixture {
            ctx,
            repo,
            db_path,
            events: rx,
            _store_dir: store_dir,
            _repo_dir: repo_dir,
        }
    }

    /// 在 app 中放置一个 Running 任务（基线取自当前仓库状态）。
    fn install_running_task(f: &Fixture, task_id: &str) {
        let gitc = GitClient::new(&f.repo);
        let baseline = halo_core::Baseline {
            head: gitc.head().unwrap(),
            tree: gitc.capture_tree().unwrap(),
            dirty_files: gitc.status_dirty_files().unwrap(),
            captured_at: now_ts(),
        };
        let task = ActiveTask {
            task_id: task_id.to_string(),
            agent: AgentKind::Pi,
            title: "编排测试任务".to_string(),
            instructions: "修改仓库文件".to_string(),
            state: TaskState::Running,
            attribution: halo_core::Attribution::AgentOnly,
            baseline,
            created_at: now_ts(),
            ended_at: None,
            cancel_mode: None,
            latest_evidence_version: 0,
            verification_agent: None,
            verification_user: None,
            cancel_tx: None,
        };
        f.ctx.store.put_task(&task.to_record()).unwrap();
        lock(&f.ctx.app).task = Some(task);
    }

    fn drain_events(rx: &Receiver<Outbound>) -> Vec<halo_protocol::Event> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Outbound::Event(e) = msg {
                out.push(e);
            }
        }
        out
    }

    fn task_state(f: &Fixture) -> TaskState {
        lock(&f.ctx.app)
            .task
            .as_ref()
            .map(|t| t.state)
            .expect("任务应存在")
    }

    #[test]
    fn happy_flow_finished_produces_review_ready_and_evidence() {
        let f = fixture();
        install_running_task(&f, "task-happy");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        let handle: Arc<dyn AgentHandle> = Arc::new(FakeHandle::inert());

        // 脚本：phase → agent 写真实文件 → 验证通过 → TaskDone(finished)
        etx.send(RuntimeEvent::Trace(RuntimeTraceItem {
            kind: "phase".to_string(),
            text: "编辑中".to_string(),
            detail: json!({"phase": "editing"}),
        }))
        .unwrap();
        fs::write(f.repo.join("agent_out 中文.txt"), "agent 写入的内容\n").unwrap();
        etx.send(RuntimeEvent::Verification {
            status: "passed".to_string(),
            detail: "自检通过".to_string(),
        })
        .unwrap();
        etx.send(RuntimeEvent::TaskDone {
            outcome: "finished".to_string(),
            summary: "完成，密钥 sk-abcdefgh12345678 不应入库".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-happy",
            AgentKind::Pi,
            handle,
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::ReviewReady);
        let evidence = f
            .ctx
            .store
            .latest_evidence("task-happy")
            .unwrap()
            .expect("应有证据");
        assert_eq!(evidence.version, 1);
        assert_eq!(evidence.outcome, "finished");
        assert_eq!(evidence.verification_status, "passed");
        assert_eq!(evidence.verification_source, "agent");
        assert!(
            evidence
                .files
                .iter()
                .any(|fi| fi.path == "agent_out 中文.txt" && fi.change == "added"),
            "{:?}",
            evidence.files
        );
        // 脱敏红线：证据摘要不得携带密钥明文
        assert!(!evidence.summary.contains("sk-abcdefgh12345678"));
        assert!(evidence.summary.contains("[REDACTED]"));

        let events = drain_events(&f.events);
        let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
        assert!(names.contains(&"trace.item"), "{names:?}");
        assert!(names.contains(&"task.phase"), "{names:?}");
        assert!(names.contains(&"task.verification"), "{names:?}");
        assert!(names.contains(&"task.finished"), "{names:?}");
        // task.state 应出现 finishing 与 review_ready 两次迁移
        let states: Vec<&str> = events
            .iter()
            .filter(|e| e.event == "task.state")
            .filter_map(|e| e.payload.get("state").and_then(Value::as_str))
            .collect();
        assert_eq!(states, vec!["finishing", "review_ready"], "{states:?}");
        let finished = events.iter().find(|e| e.event == "task.finished").unwrap();
        assert_eq!(finished.payload["evidence_version"], 1);
        assert_eq!(finished.payload["outcome"], "finished");
        // seq 全局单调递增
        let mut prev = 0;
        for e in &events {
            assert!(e.seq > prev);
            prev = e.seq;
        }
    }

    #[test]
    fn failed_outcome_marks_failed_with_evidence() {
        let f = fixture();
        install_running_task(&f, "task-fail");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        etx.send(RuntimeEvent::TaskDone {
            outcome: "failed".to_string(),
            summary: "编译失败".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-fail",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::Failed);
        let evidence = f.ctx.store.latest_evidence("task-fail").unwrap().unwrap();
        assert_eq!(evidence.outcome, "failed");
        // 未报告验证结论时如实标记 not_run
        assert_eq!(evidence.verification_status, "not_run");
        let events = drain_events(&f.events);
        let finished = events.iter().find(|e| e.event == "task.finished").unwrap();
        assert_eq!(finished.payload["outcome"], "failed");
        let rec = f.ctx.store.get_task("task-fail").unwrap().unwrap();
        assert_eq!(rec.state, "failed");
        assert!(rec.ended_at.is_some());
    }

    #[test]
    fn evidence_persistence_failure_never_marks_task_review_ready() {
        let f = fixture();
        install_running_task(&f, "task-evidence-write-failure");

        // 使用真实 SQLite 约束失败而非生产路径 mock：任务记录仍可写入，证据 INSERT 在存储接缝被拒绝。
        let conn = Connection::open(&f.db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TRIGGER reject_evidence_for_test BEFORE INSERT ON evidence
            BEGIN
                SELECT RAISE(ABORT, 'injected evidence write failure');
            END;
            "#,
        )
        .unwrap();

        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        etx.send(RuntimeEvent::TaskDone {
            outcome: "finished".to_string(),
            summary: "Agent 已完成实现".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-evidence-write-failure",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::Failed);
        assert!(f
            .ctx
            .store
            .latest_evidence("task-evidence-write-failure")
            .unwrap()
            .is_none());

        let events = drain_events(&f.events);
        let states: Vec<&str> = events
            .iter()
            .filter(|e| e.event == "task.state")
            .filter_map(|e| e.payload.get("state").and_then(Value::as_str))
            .collect();
        assert_eq!(states, vec!["finishing", "failed"], "{states:?}");
        let finished = events
            .iter()
            .find(|e| e.event == "task.finished")
            .expect("应发出终局失败事件");
        assert_eq!(finished.payload["outcome"], "failed");
        assert!(finished.payload["evidence_version"].is_null());
        assert_eq!(finished.payload["reason"], "evidence_persistence_failed");
    }

    #[test]
    fn cancel_timeout_forces_kill_and_marks_forced() {
        let f = fixture();
        install_running_task(&f, "task-cancel-f");
        let (_etx, erx) = unbounded::<RuntimeEvent>();
        let (ctx_tx, crx) = bounded::<()>(1);
        let handle = Arc::new(FakeHandle::inert());
        let handle_dyn: Arc<dyn AgentHandle> = handle.clone();

        ctx_tx.send(()).unwrap();
        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-cancel-f",
            AgentKind::Pi,
            handle_dyn,
            erx,
            crx,
        );

        assert_eq!(
            handle.cancels.load(Ordering::SeqCst),
            1,
            "必须先请求原生停止"
        );
        assert_eq!(
            handle.force_stops.load(Ordering::SeqCst),
            1,
            "宽限超时后必须强杀"
        );
        assert_eq!(task_state(&f), TaskState::Cancelled);
        let rec = f.ctx.store.get_task("task-cancel-f").unwrap().unwrap();
        assert_eq!(rec.state, "cancelled");
        assert_eq!(rec.cancel_mode.as_deref(), Some("forced"));
        let events = drain_events(&f.events);
        let cancelled = events.iter().find(|e| e.event == "task.cancelled").unwrap();
        assert_eq!(cancelled.payload["mode"], "forced");
        let evidence = f
            .ctx
            .store
            .latest_evidence("task-cancel-f")
            .unwrap()
            .unwrap();
        assert_eq!(evidence.outcome, "cancelled");
    }

    #[test]
    fn cancel_native_within_grace_marks_native() {
        let f = fixture();
        install_running_task(&f, "task-cancel-n");
        let (etx, erx) = unbounded::<RuntimeEvent>();
        let (ctx_tx, crx) = bounded::<()>(1);
        let handle = Arc::new(FakeHandle {
            cancels: AtomicUsize::new(0),
            force_stops: AtomicUsize::new(0),
            done_on_cancel: Some(etx.clone()),
        });
        let handle_dyn: Arc<dyn AgentHandle> = handle.clone();

        ctx_tx.send(()).unwrap();
        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-cancel-n",
            AgentKind::Pi,
            handle_dyn,
            erx,
            crx,
        );

        assert_eq!(handle.cancels.load(Ordering::SeqCst), 1);
        assert_eq!(
            handle.force_stops.load(Ordering::SeqCst),
            0,
            "原生停止成功不得强杀"
        );
        let rec = f.ctx.store.get_task("task-cancel-n").unwrap().unwrap();
        assert_eq!(rec.cancel_mode.as_deref(), Some("native"));
        let events = drain_events(&f.events);
        let cancelled = events.iter().find(|e| e.event == "task.cancelled").unwrap();
        assert_eq!(cancelled.payload["mode"], "native");
    }

    #[test]
    fn action_request_pauses_then_resumes_by_next_event() {
        let f = fixture();
        install_running_task(&f, "task-action");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);

        etx.send(RuntimeEvent::ActionRequest {
            request_id: "ar-1".to_string(),
            kind: "permission".to_string(),
            prompt: "允许写入 src/a.rs 吗？".to_string(),
        })
        .unwrap();
        etx.send(RuntimeEvent::Trace(RuntimeTraceItem {
            kind: "agent_note".to_string(),
            text: "已获授权，继续".to_string(),
            detail: json!({}),
        }))
        .unwrap();
        etx.send(RuntimeEvent::TaskDone {
            outcome: "finished".to_string(),
            summary: "完成".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-action",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::ReviewReady);
        let events = drain_events(&f.events);
        let action = events
            .iter()
            .find(|e| e.event == "task.action_request")
            .unwrap();
        assert_eq!(action.payload["request_id"], "ar-1");
        assert_eq!(action.payload["channel"], "native");
        let states: Vec<&str> = events
            .iter()
            .filter(|e| e.event == "task.state")
            .filter_map(|e| e.payload.get("state").and_then(Value::as_str))
            .collect();
        assert_eq!(
            states,
            vec!["awaiting_action", "running", "finishing", "review_ready"],
            "{states:?}"
        );
    }

    #[test]
    fn runtime_failure_mid_task_marks_failed() {
        let f = fixture();
        install_running_task(&f, "task-rtfail");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        etx.send(RuntimeEvent::State(RuntimeState::Failed {
            reason: "Pi 进程输出流意外结束（EOF）".to_string(),
            recovery_hint: "请重启".to_string(),
        }))
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-rtfail",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::Failed);
        let evidence = f.ctx.store.latest_evidence("task-rtfail").unwrap().unwrap();
        assert_eq!(evidence.outcome, "failed");
        assert!(evidence.summary.contains("EOF"));
    }

    #[test]
    fn manual_edit_attribution_lands_in_evidence_as_mixed() {
        let f = fixture();
        install_running_task(&f, "task-mixed");
        {
            let mut app = lock(&f.ctx.app);
            let task = app.task.as_mut().unwrap();
            task.attribution = task
                .attribution
                .clone()
                .with_manual_edit("用户于 08:12 标记人工编辑");
        }
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        etx.send(RuntimeEvent::TaskDone {
            outcome: "finished".to_string(),
            summary: "完成".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-mixed",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        let evidence = f.ctx.store.latest_evidence("task-mixed").unwrap().unwrap();
        assert_eq!(evidence.attribution, "mixed");
        assert_eq!(
            evidence.attribution_reasons,
            vec!["用户于 08:12 标记人工编辑"]
        );
    }

    #[test]
    fn trace_detail_strings_are_recursively_sanitized_and_capped() {
        let f = fixture();
        install_running_task(&f, "task-detail");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        let long = "A".repeat(limits::TRACE_TEXT_MAX + 100);
        // detail 模拟 Agent 原生输出：嵌套对象/数组内藏多种密钥样式与超长字符串
        etx.send(RuntimeEvent::Trace(RuntimeTraceItem {
            kind: "agent_note".to_string(),
            text: "普通说明".to_string(),
            detail: json!({
                "cmd": "curl -H 'Authorization: Bearer tok12345678'",
                "nested": {"key": "sk-abcdefgh12345678"},
                "list": ["password=hunter2secret", {"aws": "AKIAIOSFODNN7EXAMPLE"}],
                "long": long,
                "count": 42
            }),
        }))
        .unwrap();
        etx.send(RuntimeEvent::TaskDone {
            outcome: "finished".to_string(),
            summary: "完成".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-detail",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        let events = drain_events(&f.events);
        let trace = events
            .iter()
            .find(|e| e.event == "trace.item" && e.payload["kind"] == "agent_note")
            .expect("应有 agent_note trace.item");
        let detail = &trace.payload["detail"];
        let rendered = detail.to_string();
        for plain in [
            "tok12345678",
            "sk-abcdefgh12345678",
            "hunter2secret",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            assert!(!rendered.contains(plain), "detail 泄漏 {plain}：{rendered}");
        }
        assert!(rendered.contains("[REDACTED]"), "{rendered}");
        // 非字符串值原样保留；超长字符串按 TRACE_TEXT_MAX 截断
        assert_eq!(detail["count"], 42);
        assert!(
            detail["long"].as_str().expect("long 应仍为字符串").len() <= limits::TRACE_TEXT_MAX
        );
    }

    #[test]
    fn evidence_summary_sanitizes_git_error_stderr() {
        let f = fixture();
        install_running_task(&f, "task-git-err");
        // 把基线树篡改为密钥样式的非法引用：git diff-tree 失败时会把该参数
        // 原样回显进 stderr，构成"GitError 文本注入密钥"的真实路径
        {
            let mut app = lock(&f.ctx.app);
            app.task.as_mut().unwrap().baseline.tree = "sk-fakegitsecret1234567890".to_string();
        }

        let version = append_evidence(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-git-err",
            "finished",
            "任务完成",
        )
        .expect("取证失败时仍应写入包含失败摘要的证据");
        assert_eq!(version, 1);

        let evidence = f
            .ctx
            .store
            .latest_evidence("task-git-err")
            .unwrap()
            .unwrap();
        assert!(
            evidence.summary.contains("取证失败"),
            "{}",
            evidence.summary
        );
        assert!(
            !evidence.summary.contains("sk-fakegitsecret1234567890"),
            "证据摘要泄漏 git stderr 中的密钥样式文本：{}",
            evidence.summary
        );
        assert!(
            evidence.summary.contains("[REDACTED]"),
            "{}",
            evidence.summary
        );
    }

    #[test]
    fn baseline_dirty_files_recorded_and_kept_separate() {
        let f = fixture();
        // 基线前已有修改：dirty.txt
        fs::write(f.repo.join("dirty.txt"), "基线前的未提交修改\n").unwrap();
        install_running_task(&f, "task-dirty");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);
        etx.send(RuntimeEvent::TaskDone {
            outcome: "finished".to_string(),
            summary: "完成".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-dirty",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        let evidence = f.ctx.store.latest_evidence("task-dirty").unwrap().unwrap();
        assert!(
            evidence
                .baseline_dirty_files
                .contains(&"dirty.txt".to_string()),
            "{:?}",
            evidence.baseline_dirty_files
        );
        // dirty.txt 任务期间未再变化：不应出现在关联变更中
        assert!(
            !evidence.files.iter().any(|x| x.path == "dirty.txt"),
            "{:?}",
            evidence.files
        );
    }
}
