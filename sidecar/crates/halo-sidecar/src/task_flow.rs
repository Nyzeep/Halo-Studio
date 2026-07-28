//! 任务编排：前置校验之后的完整生命周期——记基线 → 交给运行时 → RuntimeEvent
//! 规范化为契约事件 → 终态 → 关联变更取证（sanitize + cap）→ 证据落库 → 收尾事件。
//!
//! 状态迁移一律经 halo_core::TaskState::apply 驱动，编排层不得私设状态。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use halo_config::AgentKind;
use halo_core::{cap, limits, sanitize, TaskEvent, TaskState, Verification};
use halo_runtime::{
    ActionDecision as RuntimeActionDecision, RunTaskSpec, RuntimeError, RuntimeEvent, RuntimeState,
    Timeouts,
};
use halo_store::Store;

use crate::dispatch::SidecarError;
use crate::git::GitClient;
use crate::mapping::{
    attribution_core_to_str, attribution_reasons, now_ts, verification_source_core_to_str,
    verification_status_core_to_str,
};
use crate::server::EventBus;
use crate::state::{append_active_session_message, lock, ActiveTask, AgentHandle, AppState};

/// 与 fs.read/fs.write 的哈希口径一致；更大的文件只保留文件级证据，不做行级断言。
const END_HASH_MAX_BYTES: usize = 8 * 1024 * 1024;

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
        manual_edit_paths: Default::default(),
        baseline,
        created_at: now_ts(),
        ended_at: None,
        cancel_mode: None,
        latest_evidence_version: 0,
        verification_agent: None,
        verification_user: None,
        session_messages: vec![],
        action_requests: Default::default(),
        cancellation_requested: false,
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
    append_active_session_message(
        &ctx.app,
        &ctx.bus,
        &task_id,
        halo_protocol::methods::task::TaskSessionMessageRole::User,
        &args.instructions,
    );

    // 2. 先进入 Running 再交给运行时。run_task 可能立即从异步事件流送回首轮回复；
    // 若仍停在 Created，RoundCompleted 会被状态机拒绝而丢失等待开发者的转换。
    apply_event(ctx, &task_id, &TaskEvent::Started);

    // 3. 组 RunTaskSpec 交给运行时
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

/// 将当前任务中精确匹配的操作请求以一次性方式提交给原生 Agent。
/// HTTP 请求成功只表示决议已送达；AwaitingAction 仍须等待 RuntimeEvent::ActionResolved。
pub fn resolve_action(
    ctx: &FlowCtx,
    params: &halo_protocol::methods::task::ResolveActionParams,
    handle: Arc<dyn AgentHandle>,
) -> Result<(), SidecarError> {
    use halo_protocol::methods::task::{ActionDecision, TaskActionKind};

    let decision = {
        let mut app = lock(&ctx.app);
        let task = app
            .task
            .as_mut()
            .filter(|task| task.task_id == params.task_id)
            .ok_or_else(|| {
                SidecarError::new(
                    halo_protocol::ErrorCode::ActionRequestNotFound,
                    "当前任务没有匹配的操作请求",
                )
            })?;
        if task.cancellation_requested || task.state != TaskState::AwaitingAction {
            return Err(SidecarError::new(
                halo_protocol::ErrorCode::ActionRequestNotPending,
                "当前任务没有等待决议的操作请求",
            ));
        }
        let request = task
            .action_requests
            .get_mut(&params.request_id)
            .ok_or_else(|| {
                SidecarError::new(
                    halo_protocol::ErrorCode::ActionRequestNotFound,
                    "当前任务没有匹配的操作请求",
                )
            })?;
        if request.decision_sent {
            return Err(SidecarError::new(
                halo_protocol::ErrorCode::ActionRequestAlreadyResolved,
                "该操作请求已经提交过一次决定",
            ));
        }

        let decision = match (request.kind, params.decision) {
            (TaskActionKind::Permission, ActionDecision::AllowOnce) => {
                RuntimeActionDecision::AllowOnce
            }
            (
                TaskActionKind::Permission | TaskActionKind::Clarification,
                ActionDecision::Reject,
            ) => RuntimeActionDecision::Reject,
            (TaskActionKind::Clarification, ActionDecision::Answer) => {
                let answer = params.answer.as_deref().unwrap_or_default().trim();
                if answer.is_empty() {
                    return Err(SidecarError::new(
                        halo_protocol::ErrorCode::InvalidParams,
                        "澄清回答不能为空",
                    ));
                }
                let (answer, _) = cap(&sanitize(answer), limits::TRACE_TEXT_MAX);
                RuntimeActionDecision::Answer(answer)
            }
            _ => {
                return Err(SidecarError::new(
                    halo_protocol::ErrorCode::InvalidParams,
                    "该操作请求不接受此决定",
                ))
            }
        };
        if matches!(
            params.decision,
            ActionDecision::AllowOnce | ActionDecision::Reject
        ) && params.answer.is_some()
        {
            return Err(SidecarError::new(
                halo_protocol::ErrorCode::InvalidParams,
                "本次允许或拒绝不接受回答内容",
            ));
        }
        request.decision_sent = true;
        decision
    };

    if let Err(error) = handle.resolve_action(&params.request_id, decision) {
        // 送达不确定时运行时已失败关闭；重开卡片会允许同一原生操作被重复决议。
        if !matches!(error, RuntimeError::ActionRequestDeliveryUncertain) {
            let mut app = lock(&ctx.app);
            if let Some(request) = app
                .task
                .as_mut()
                .filter(|task| task.task_id == params.task_id)
                .and_then(|task| task.action_requests.get_mut(&params.request_id))
            {
                request.decision_sent = false;
            }
        }
        return Err(match error {
            RuntimeError::ActionRequestNotFound => SidecarError::new(
                halo_protocol::ErrorCode::ActionRequestNotFound,
                "当前任务没有匹配的操作请求",
            ),
            RuntimeError::ActionRequestAlreadyResolved => SidecarError::new(
                halo_protocol::ErrorCode::ActionRequestAlreadyResolved,
                "该操作请求已经提交过一次决定",
            ),
            other => SidecarError::from(other),
        });
    }
    Ok(())
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
                Ok(RuntimeEvent::ActionRequest { request_id, kind, prompt }) => {
                    if agent != AgentKind::OpenCode {
                        break Ending::RuntimeFailed {
                            reason: "当前受管执行器不支持一次性操作请求".to_string(),
                        };
                    }
                    on_action_request(ctx, task_id, &request_id, &kind, &prompt);
                }
                Ok(RuntimeEvent::ActionResolved { request_id }) =>
                    on_action_resolved(ctx, task_id, &request_id),
                Ok(RuntimeEvent::Verification { status, detail }) =>
                    on_verification(ctx, task_id, &status, &detail),
                Ok(RuntimeEvent::SessionReply { text }) => on_session_reply(ctx, task_id, &text),
                Ok(RuntimeEvent::TaskDone { outcome, summary }) => {
                    let waiting_for_developer = {
                        let app = lock(&ctx.app);
                        app.task
                            .as_ref()
                            .is_some_and(|task| task.task_id == task_id && task.state == TaskState::WaitingDeveloper)
                    };
                    // OpenCode 的首轮回合以 SessionReply 结束。即使服务端或旧适配层随后
                    // 迟到一个 TaskDone，也不能把开发者尚未结束的会话自动送进交付审查。
                    if waiting_for_developer {
                        continue;
                    }
                    let awaiting_action = {
                        let app = lock(&ctx.app);
                        app.task.as_ref().is_some_and(|task| {
                            task.task_id == task_id
                                && task.state == TaskState::AwaitingAction
                                && !task.action_requests.is_empty()
                        })
                    };
                    if awaiting_action {
                        break Ending::RuntimeFailed {
                            reason: "受管 Agent 在操作请求尚未得到确认前结束".to_string(),
                        };
                    }
                    break Ending::Done { outcome, summary };
                }
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

/// 首轮 OpenCode 回复只追加活动会话记录，并把任务留在等待开发者。
/// 它绝不触发取证、审查或 task.finished；这些属于后续显式结束会话动作。
fn on_session_reply(ctx: &FlowCtx, task_id: &str, text: &str) {
    let is_running = {
        let app = lock(&ctx.app);
        app.task
            .as_ref()
            .is_some_and(|task| task.task_id == task_id && task.state == TaskState::Running)
    };
    if !is_running {
        return;
    }
    if append_active_session_message(
        &ctx.app,
        &ctx.bus,
        task_id,
        halo_protocol::methods::task::TaskSessionMessageRole::Agent,
        text,
    )
    .is_some()
    {
        apply_event(ctx, task_id, &TaskEvent::RoundCompleted);
    }
}

/// Agent 操作请求：Running → AwaitingAction，等待用户经其原生通道决定。
fn on_action_request(ctx: &FlowCtx, task_id: &str, request_id: &str, kind: &str, prompt: &str) {
    use halo_protocol::methods::task::{TaskActionKind, TaskActionRequest};

    let kind = match kind {
        "permission" => TaskActionKind::Permission,
        "clarification" => TaskActionKind::Clarification,
        _ => {
            apply_event(
                ctx,
                task_id,
                &TaskEvent::Fail("OpenCode 返回了不受支持的操作请求类型".to_string()),
            );
            return;
        }
    };
    if request_id.is_empty()
        || request_id.len() > 160
        || !request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        apply_event(
            ctx,
            task_id,
            &TaskEvent::Fail("OpenCode 返回了无法安全决议的操作请求".to_string()),
        );
        return;
    }
    let (prompt, _) = cap(&sanitize(prompt), limits::TRACE_TEXT_MAX);
    let (emit, enter_awaiting) = {
        let mut app = lock(&ctx.app);
        let Some(task) = app.task.as_mut().filter(|task| task.task_id == task_id) else {
            return;
        };
        if task.cancellation_requested
            || !matches!(task.state, TaskState::Running | TaskState::AwaitingAction)
            || task.action_requests.contains_key(request_id)
        {
            return;
        }
        let enter_awaiting = task.state == TaskState::Running;
        task.action_requests.insert(
            request_id.to_string(),
            TaskActionRequest {
                request_id: request_id.to_string(),
                kind,
                prompt: prompt.clone(),
                decision_sent: false,
            },
        );
        (true, enter_awaiting)
    };
    if !emit {
        return;
    }
    if enter_awaiting {
        apply_event(ctx, task_id, &TaskEvent::ActionRequested);
    }
    let kind_name = match kind {
        TaskActionKind::Permission => "permission",
        TaskActionKind::Clarification => "clarification",
    };
    ctx.bus.emit(
        Some(task_id),
        "task.action_request",
        json!({"request_id": request_id, "kind": kind_name, "prompt": prompt, "decision_sent": false}),
    );
    ctx.bus.emit(
        Some(task_id),
        "trace.item",
        json!({"kind": "action_request", "text": prompt, "detail": {"request_id": request_id, "kind": kind_name}}),
    );
}

/// 只有 OpenCode 对同一请求的真实 replied/rejected 事件才能结束 AwaitingAction。
fn on_action_resolved(ctx: &FlowCtx, task_id: &str, request_id: &str) {
    let return_to_running = {
        let mut app = lock(&ctx.app);
        let Some(task) = app.task.as_mut().filter(|task| task.task_id == task_id) else {
            return;
        };
        let Some(request) = task.action_requests.get(request_id) else {
            return;
        };
        if !request.decision_sent {
            return;
        }
        task.action_requests.remove(request_id);
        task.state == TaskState::AwaitingAction
            && task.action_requests.is_empty()
            && !task.cancellation_requested
    };
    ctx.bus.emit(
        Some(task_id),
        "task.action_resolved",
        json!({"request_id": request_id}),
    );
    if return_to_running {
        apply_event(ctx, task_id, &TaskEvent::ActionResolved);
    }
}

/// Agent 原生验证结论：记录 + 推送 task.verification（source=agent）。
fn on_verification(ctx: &FlowCtx, task_id: &str, status: &str, detail: &str) {
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
                if next.is_terminal() {
                    task.session_messages.clear();
                    task.action_requests.clear();
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
    match git.capture_tree().and_then(|end| {
        let diffs = git.diff_trees(&baseline.tree, &end)?;
        Ok((end, diffs))
    }) {
        Ok((end_tree, diffs)) => {
            for d in diffs {
                let (diff, _) = cap(&sanitize(&d.diff), limits::VERSION_TOTAL_MAX);
                let end_hash = end_tree_file_hash(git, &end_tree, &d.path, &d.change);
                files.push(halo_store::FileChangeDraft {
                    path: d.path,
                    change: d.change,
                    diff,
                    end_hash,
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

fn end_tree_file_hash(git: &GitClient, end_tree: &str, path: &str, change: &str) -> Option<String> {
    if change == "deleted" {
        return None;
    }
    let size = git.tree_file_size(end_tree, path).ok()?;
    if size > END_HASH_MAX_BYTES as u64 {
        return None;
    }
    let bytes = git.show_tree_file(end_tree, path).ok()?;
    let digest = Sha256::digest(bytes);
    Some(format!("sha256:{digest:x}"))
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
    use halo_runtime::{
        ActionDecision as RuntimeActionDecision, RuntimeError, RuntimeTraceItem, StopOutcome,
    };
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

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

    struct ActionHandle {
        decisions: Mutex<Vec<(String, RuntimeActionDecision)>>,
    }

    impl ActionHandle {
        fn new() -> Self {
            Self {
                decisions: Mutex::new(vec![]),
            }
        }
    }

    impl AgentHandle for ActionHandle {
        fn run_task(&self, _spec: &RunTaskSpec) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn resolve_action(
            &self,
            request_id: &str,
            decision: RuntimeActionDecision,
        ) -> Result<(), RuntimeError> {
            self.decisions
                .lock()
                .expect("测试决议记录锁不应中毒")
                .push((request_id.to_string(), decision));
            Ok(())
        }

        fn cancel_native(&self) {}

        fn stop(&self, _grace: Duration) -> StopOutcome {
            StopOutcome::Graceful
        }

        fn state(&self) -> RuntimeState {
            RuntimeState::Ready
        }
    }

    struct UncertainActionHandle;

    impl AgentHandle for UncertainActionHandle {
        fn run_task(&self, _spec: &RunTaskSpec) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn resolve_action(
            &self,
            _request_id: &str,
            _decision: RuntimeActionDecision,
        ) -> Result<(), RuntimeError> {
            Err(RuntimeError::ActionRequestDeliveryUncertain)
        }

        fn cancel_native(&self) {}

        fn stop(&self, _grace: Duration) -> StopOutcome {
            StopOutcome::Graceful
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
            manual_edit_paths: Default::default(),
            baseline,
            created_at: now_ts(),
            ended_at: None,
            cancel_mode: None,
            latest_evidence_version: 0,
            verification_agent: None,
            verification_user: None,
            session_messages: vec![],
            action_requests: Default::default(),
            cancellation_requested: false,
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
    fn first_session_reply_waits_for_developer_without_delivery_evidence() {
        let f = fixture();
        install_running_task(&f, "task-first-reply");
        crate::state::append_active_session_message(
            &f.ctx.app,
            &f.ctx.bus,
            "task-first-reply",
            halo_protocol::methods::task::TaskSessionMessageRole::User,
            "请完成这项受管任务",
        )
        .expect("首条用户消息应进入活动会话记录");

        let (event_tx, event_rx) = unbounded();
        let (cancel_tx, cancel_rx) = bounded::<()>(1);
        let handle: Arc<dyn AgentHandle> = Arc::new(FakeHandle {
            cancels: AtomicUsize::new(0),
            force_stops: AtomicUsize::new(0),
            done_on_cancel: Some(event_tx.clone()),
        });
        let ctx = f.ctx.clone();
        let repo = f.repo.clone();
        let join = std::thread::spawn(move || {
            run_task_loop(
                &ctx,
                &GitClient::new(repo),
                "task-first-reply",
                AgentKind::OpenCode,
                handle,
                event_rx,
                cancel_rx,
            );
        });

        event_tx
            .send(RuntimeEvent::SessionReply {
                text: format!(
                    "已完成首轮回复，password=do-not-leak {}",
                    "x".repeat(limits::TRACE_TEXT_MAX + 16)
                ),
            })
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if task_state(&f) == TaskState::WaitingDeveloper {
                break;
            }
            assert!(Instant::now() < deadline, "首轮回复后应进入等待开发者");
            std::thread::sleep(Duration::from_millis(5));
        }

        assert!(
            f.ctx
                .store
                .latest_evidence("task-first-reply")
                .unwrap()
                .is_none(),
            "等待开发者不得自动生成交付证据"
        );
        let active = lock(&f.ctx.app).task.as_ref().unwrap().session_messages.clone();
        assert_eq!(active.len(), 2);
        assert_eq!(
            active[1].role,
            halo_protocol::methods::task::TaskSessionMessageRole::Agent
        );
        assert!(active[1].truncated);
        assert!(!active[1].text.contains("do-not-leak"));
        assert!(active[1].text.contains("[REDACTED]"));

        // 迟到或重复的同轮回复不能在 waiting_developer 中形成第二条消息，
        // 也不能把状态机从等待开发者错误地推向其他状态。
        event_tx
            .send(RuntimeEvent::SessionReply {
                text: "重复的迟到回复".to_string(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(task_state(&f), TaskState::WaitingDeveloper);
        assert_eq!(
            lock(&f.ctx.app)
                .task
                .as_ref()
                .unwrap()
                .session_messages
                .len(),
            2,
            "waiting_developer 不得追加重复会话回复"
        );

        event_tx
            .send(RuntimeEvent::TaskDone {
                outcome: "finished".to_string(),
                summary: "迟到的旧式完成事件".to_string(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(task_state(&f), TaskState::WaitingDeveloper);
        assert!(
            f.ctx
                .store
                .latest_evidence("task-first-reply")
                .unwrap()
                .is_none(),
            "迟到 TaskDone 不得让等待开发者自动产生交付证据"
        );

        let events = drain_events(&f.events);
        assert!(events.iter().any(|event| {
            event.event == "task.session_message"
                && event.payload["role"] == "user"
                && event.payload["text"] == "请完成这项受管任务"
        }));
        assert!(events.iter().any(|event| {
            event.event == "task.session_message"
                && event.payload["role"] == "agent"
                && event.payload["truncated"] == true
        }));
        assert!(events.iter().any(|event| {
            event.event == "task.state" && event.payload["state"] == "waiting_developer"
        }));
        assert!(!events.iter().any(|event| event.event == "task.finished"));

        cancel_tx.send(()).unwrap();
        join.join().unwrap();
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
    fn action_request_does_not_resume_from_trace_or_task_done_without_exact_resolution() {
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
            AgentKind::OpenCode,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::Failed);
        let events = drain_events(&f.events);
        let action = events
            .iter()
            .find(|e| e.event == "task.action_request")
            .unwrap();
        assert_eq!(
            action.payload,
            json!({
                "request_id": "ar-1",
                "kind": "permission",
                "prompt": "允许写入 src/a.rs 吗？",
                "decision_sent": false
            })
        );
        let states: Vec<&str> = events
            .iter()
            .filter(|e| e.event == "task.state")
            .filter_map(|e| e.payload.get("state").and_then(Value::as_str))
            .collect();
        assert_eq!(states, vec!["awaiting_action", "failed"], "{states:?}");
    }

    #[test]
    fn non_opencode_action_request_fails_closed_without_publishing_a_card() {
        let f = fixture();
        install_running_task(&f, "task-pi-action");
        let (etx, erx) = unbounded();
        let (_ctx_tx, crx) = bounded::<()>(1);

        etx.send(RuntimeEvent::ActionRequest {
            request_id: "pi-request-1".to_string(),
            kind: "permission".to_string(),
            prompt: "允许本次写入吗？".to_string(),
        })
        .unwrap();

        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-pi-action",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            erx,
            crx,
        );

        assert_eq!(task_state(&f), TaskState::Failed);
        let events = drain_events(&f.events);
        assert!(!events
            .iter()
            .any(|event| event.event == "task.action_request"));
        let states: Vec<&str> = events
            .iter()
            .filter(|event| event.event == "task.state")
            .filter_map(|event| event.payload.get("state").and_then(Value::as_str))
            .collect();
        assert_eq!(states, vec!["failed"], "{states:?}");
    }

    #[test]
    fn action_resolution_requires_exact_pending_request_and_real_agent_ack() {
        use halo_protocol::methods::task::{ActionDecision, ResolveActionParams};

        let f = fixture();
        install_running_task(&f, "task-action-resolution");
        on_action_request(
            &f.ctx,
            "task-action-resolution",
            "per-1",
            "permission",
            "允许 password=do-not-leak 写入 src/auth.rs 吗？",
        );

        assert_eq!(task_state(&f), TaskState::AwaitingAction);
        let prompt = lock(&f.ctx.app)
            .task
            .as_ref()
            .and_then(|task| task.action_requests.get("per-1"))
            .expect("权限请求应保留在活动任务中")
            .prompt
            .clone();
        assert!(!prompt.contains("do-not-leak"));
        assert!(prompt.contains("[REDACTED]"));

        let handle = Arc::new(ActionHandle::new());
        let mismatch = ResolveActionParams {
            task_id: "task-action-resolution".to_string(),
            request_id: "per-other".to_string(),
            decision: ActionDecision::AllowOnce,
            answer: None,
        };
        let error = resolve_action(&f.ctx, &mismatch, handle.clone()).unwrap_err();
        assert_eq!(error.code, halo_protocol::ErrorCode::ActionRequestNotFound);
        assert!(handle
            .decisions
            .lock()
            .expect("测试决议记录锁不应中毒")
            .is_empty());

        let allow_once = ResolveActionParams {
            task_id: "task-action-resolution".to_string(),
            request_id: "per-1".to_string(),
            decision: ActionDecision::AllowOnce,
            answer: None,
        };
        resolve_action(&f.ctx, &allow_once, handle.clone()).expect("本次允许应提交给原生 Agent");
        assert_eq!(task_state(&f), TaskState::AwaitingAction);
        assert_eq!(
            *handle.decisions.lock().expect("测试决议记录锁不应中毒"),
            vec![("per-1".to_string(), RuntimeActionDecision::AllowOnce)]
        );

        let duplicate = resolve_action(&f.ctx, &allow_once, handle.clone()).unwrap_err();
        assert_eq!(
            duplicate.code,
            halo_protocol::ErrorCode::ActionRequestAlreadyResolved
        );

        on_trace(
            &f.ctx,
            "task-action-resolution",
            &RuntimeTraceItem {
                kind: "agent_note".to_string(),
                text: "继续执行".to_string(),
                detail: json!({}),
            },
        );
        on_action_resolved(&f.ctx, "task-action-resolution", "per-other");
        assert_eq!(
            task_state(&f),
            TaskState::AwaitingAction,
            "普通轨迹和不匹配的确认不得结束等待"
        );

        on_action_resolved(&f.ctx, "task-action-resolution", "per-1");
        assert_eq!(task_state(&f), TaskState::Running);
        let resolved = drain_events(&f.events)
            .into_iter()
            .find(|event| event.event == "task.action_resolved")
            .expect("精确的原生回执应通知活动会话");
        assert_eq!(resolved.task_id.as_deref(), Some("task-action-resolution"));
        assert_eq!(resolved.payload, json!({"request_id": "per-1"}));
        assert!(lock(&f.ctx.app)
            .task
            .as_ref()
            .expect("任务仍活动")
            .action_requests
            .is_empty());

        on_action_request(
            &f.ctx,
            "task-action-resolution",
            "que-2",
            "clarification",
            "请选择要继续的本地配置。",
        );
        let invalid_permission_decision = ResolveActionParams {
            task_id: "task-action-resolution".to_string(),
            request_id: "que-2".to_string(),
            decision: ActionDecision::AllowOnce,
            answer: None,
        };
        let error =
            resolve_action(&f.ctx, &invalid_permission_decision, handle.clone()).unwrap_err();
        assert_eq!(error.code, halo_protocol::ErrorCode::InvalidParams);

        let answer = ResolveActionParams {
            task_id: "task-action-resolution".to_string(),
            request_id: "que-2".to_string(),
            decision: ActionDecision::Answer,
            answer: Some("password=do-not-forward".to_string()),
        };
        resolve_action(&f.ctx, &answer, handle.clone()).expect("澄清回答应提交给原生 Agent");
        let decisions = handle.decisions.lock().expect("测试决议记录锁不应中毒");
        assert!(matches!(
            decisions.last(),
            Some((request_id, RuntimeActionDecision::Answer(text)))
                if request_id == "que-2"
                    && !text.contains("do-not-forward")
                    && text.contains("[REDACTED]")
        ));
        drop(decisions);
        on_action_resolved(&f.ctx, "task-action-resolution", "que-2");
        assert_eq!(task_state(&f), TaskState::Running);
    }

    #[test]
    fn uncertain_action_delivery_keeps_the_exact_request_locked_until_failure() {
        use halo_protocol::methods::task::{ActionDecision, ResolveActionParams};

        let f = fixture();
        install_running_task(&f, "task-action-uncertain");
        on_action_request(
            &f.ctx,
            "task-action-uncertain",
            "per-uncertain",
            "permission",
            "允许本次写入 src/auth.rs 吗？",
        );
        let params = ResolveActionParams {
            task_id: "task-action-uncertain".to_string(),
            request_id: "per-uncertain".to_string(),
            decision: ActionDecision::AllowOnce,
            answer: None,
        };

        let error = resolve_action(&f.ctx, &params, Arc::new(UncertainActionHandle)).unwrap_err();
        assert_eq!(
            error.code,
            halo_protocol::ErrorCode::ActionRequestNotPending
        );
        let request = lock(&f.ctx.app)
            .task
            .as_ref()
            .and_then(|task| task.action_requests.get("per-uncertain"))
            .expect("送达不确定前的精确请求仍须保留到失败事件处理");
        assert!(request.decision_sent);

        let duplicate =
            resolve_action(&f.ctx, &params, Arc::new(UncertainActionHandle)).unwrap_err();
        assert_eq!(
            duplicate.code,
            halo_protocol::ErrorCode::ActionRequestAlreadyResolved,
            "送达不确定时不得重新开放同一原生操作"
        );
    }

    #[test]
    fn resolving_one_of_multiple_requests_only_removes_the_exact_card() {
        use halo_protocol::methods::task::{ActionDecision, ResolveActionParams};

        let f = fixture();
        install_running_task(&f, "task-multiple-actions");
        on_action_request(
            &f.ctx,
            "task-multiple-actions",
            "per-1",
            "permission",
            "允许本次写入 src/auth.rs 吗？",
        );
        on_action_request(
            &f.ctx,
            "task-multiple-actions",
            "que-2",
            "clarification",
            "请选择继续任务的配置。",
        );

        let handle = Arc::new(ActionHandle::new());
        resolve_action(
            &f.ctx,
            &ResolveActionParams {
                task_id: "task-multiple-actions".to_string(),
                request_id: "per-1".to_string(),
                decision: ActionDecision::AllowOnce,
                answer: None,
            },
            handle,
        )
        .expect("本次允许应提交给原生 Agent");
        on_action_resolved(&f.ctx, "task-multiple-actions", "per-1");

        assert_eq!(task_state(&f), TaskState::AwaitingAction);
        let (permission_removed, clarification_pending) = {
            let app = lock(&f.ctx.app);
            let task = app.task.as_ref().expect("任务仍活动");
            (
                !task.action_requests.contains_key("per-1"),
                task.action_requests.contains_key("que-2"),
            )
        };
        assert!(permission_removed);
        assert!(clarification_pending);
        let events = drain_events(&f.events);
        assert!(events
            .iter()
            .any(|event| event.event == "task.action_resolved"
                && event.payload == json!({"request_id": "per-1"})));
        assert!(!events
            .iter()
            .any(|event| { event.event == "task.state" && event.payload["state"] == "running" }));
    }

    #[test]
    fn rejected_action_can_only_fail_after_the_agent_acknowledges_it() {
        use halo_protocol::methods::task::{ActionDecision, ResolveActionParams};

        let f = fixture();
        install_running_task(&f, "task-action-reject");
        on_action_request(
            &f.ctx,
            "task-action-reject",
            "que-1",
            "clarification",
            "请确认是否继续执行？",
        );
        let handle = Arc::new(ActionHandle::new());
        let reject = ResolveActionParams {
            task_id: "task-action-reject".to_string(),
            request_id: "que-1".to_string(),
            decision: ActionDecision::Reject,
            answer: None,
        };
        resolve_action(&f.ctx, &reject, handle.clone()).expect("拒绝应提交给原生 Agent");
        assert_eq!(task_state(&f), TaskState::AwaitingAction);
        assert_eq!(
            *handle.decisions.lock().expect("测试决议记录锁不应中毒"),
            vec![("que-1".to_string(), RuntimeActionDecision::Reject)]
        );

        on_action_resolved(&f.ctx, "task-action-reject", "que-1");
        assert_eq!(task_state(&f), TaskState::Running);

        let (event_tx, event_rx) = unbounded();
        let (_cancel_tx, cancel_rx) = bounded::<()>(1);
        event_tx
            .send(RuntimeEvent::TaskDone {
                outcome: "failed".to_string(),
                summary: "Agent 因开发者拒绝而停止".to_string(),
            })
            .expect("测试事件通道应可用");
        run_task_loop(
            &f.ctx,
            &GitClient::new(&f.repo),
            "task-action-reject",
            AgentKind::Pi,
            Arc::new(FakeHandle::inert()),
            event_rx,
            cancel_rx,
        );
        assert_eq!(task_state(&f), TaskState::Failed);
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
