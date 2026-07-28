//! AppState：活动工作区、受管运行时句柄、当前任务的进程内状态。
//! 持久化真相在 halo-store；这里只保存运行期上下文与路由。

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use serde_json::{json, Value};

use halo_config::AgentKind;
use halo_runtime::{
    OpenCodeHandle, PiHandle, RunTaskSpec, RuntimeError, RuntimeEvent, RuntimeState, StopOutcome,
};

use crate::mapping::runtime_state_payload;
use crate::server::EventBus;

/// Mutex 中毒时继续使用内部值：状态为普通数据，恢复使用不破坏不变量。
pub fn lock<'a, T>(m: &'a Mutex<T>) -> MutexGuard<'a, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 受管运行时句柄的统一抽象；生产实现为 PiHandle / OpenCodeHandle，
/// 测试替身只允许出现在 #[cfg(test)]。
pub trait AgentHandle: Send + Sync {
    fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError>;
    fn cancel_native(&self);
    fn stop(&self, grace: Duration) -> StopOutcome;
    fn state(&self) -> RuntimeState;
}

impl AgentHandle for PiHandle {
    fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError> {
        PiHandle::run_task(self, spec)
    }
    fn cancel_native(&self) {
        PiHandle::cancel_native(self)
    }
    fn stop(&self, grace: Duration) -> StopOutcome {
        PiHandle::stop(self, grace)
    }
    fn state(&self) -> RuntimeState {
        PiHandle::state(self)
    }
}

impl AgentHandle for OpenCodeHandle {
    fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError> {
        OpenCodeHandle::run_task(self, spec)
    }
    fn cancel_native(&self) {
        OpenCodeHandle::cancel_native(self)
    }
    fn stop(&self, grace: Duration) -> StopOutcome {
        OpenCodeHandle::stop(self, grace)
    }
    fn state(&self) -> RuntimeState {
        OpenCodeHandle::state(self)
    }
}

/// 单个受管应用的运行期槽位；两个槽位独立，绝不合并为"全局在线"。
pub struct RuntimeSlot {
    pub last_state: RuntimeState,
    pub version: Option<String>,
    pub handle: Option<Arc<dyn AgentHandle>>,
    /// 每次启动或停止都推进。迟到的旧实例事件不得覆盖当前实例状态。
    pub generation: u64,
    /// 任务事件路由：当前任务的事件消费端；无任务时为 None。
    pub task_tx: Option<Sender<RuntimeEvent>>,
}

impl RuntimeSlot {
    fn new() -> Self {
        RuntimeSlot {
            last_state: RuntimeState::NotProbed,
            version: None,
            handle: None,
            generation: 0,
            task_tx: None,
        }
    }

    pub fn advance_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    /// 当前对外可见状态：有句柄时以句柄实时状态为准。
    pub fn effective_state(&self) -> RuntimeState {
        match &self.handle {
            Some(h) => h.state(),
            None => self.last_state.clone(),
        }
    }
}

/// 活动工作区（唯一项目上下文）。
#[derive(Debug, Clone)]
pub struct ActiveWorkspace {
    pub workspace_id: String,
    pub real_path: String,
    pub git_root: String,
    pub root_commit: Option<String>,
    pub trust: halo_core::TrustState,
    pub identity_changed: bool,
}

impl ActiveWorkspace {
    pub fn is_trusted(&self) -> bool {
        matches!(self.trust, halo_core::TrustState::Trusted)
    }
}

/// 当前任务的进程内状态；持久化副本经 store 落库。
pub struct ActiveTask {
    pub task_id: String,
    pub agent: AgentKind,
    pub title: String,
    pub instructions: String,
    pub state: halo_core::TaskState,
    pub attribution: halo_core::Attribution,
    /// 活跃任务期经工作台发生人工写入的路径；BTreeSet 保证持久化和展示稳定有序。
    pub manual_edit_paths: BTreeSet<String>,
    pub baseline: halo_core::Baseline,
    pub created_at: String,
    pub ended_at: Option<String>,
    pub cancel_mode: Option<String>,
    pub latest_evidence_version: u32,
    /// Agent 原生运行时最近一次报告的验证结论。
    pub verification_agent: Option<halo_core::Verification>,
    /// 用户显式标记的"未执行"结论；证据落库时优先级高于 Agent 缺省。
    pub verification_user: Option<halo_core::Verification>,
    /// 仅存于当前 Sidecar 进程的活动会话记录。它不会进入任务记录、证据或历史。
    pub session_messages: Vec<halo_protocol::methods::task::TaskSessionMessage>,
    /// 取消请求通道（发往任务编排线程）。
    pub cancel_tx: Option<Sender<()>>,
}

impl ActiveTask {
    pub fn session_message(
        role: halo_protocol::methods::task::TaskSessionMessageRole,
        text: &str,
    ) -> halo_protocol::methods::task::TaskSessionMessage {
        let (text, truncated) = halo_core::cap(
            &halo_core::sanitize(text),
            halo_core::limits::TRACE_TEXT_MAX,
        );
        halo_protocol::methods::task::TaskSessionMessage {
            role,
            text,
            truncated,
        }
    }

    pub fn append_session_message(
        &mut self,
        role: halo_protocol::methods::task::TaskSessionMessageRole,
        text: &str,
    ) -> halo_protocol::methods::task::TaskSessionMessage {
        let message = Self::session_message(role, text);
        self.session_messages.push(message.clone());
        message
    }

    pub fn to_record(&self) -> halo_store::TaskRecord {
        let (goal, _) = halo_core::cap(
            &halo_core::sanitize(&self.instructions),
            halo_core::limits::SUMMARY_MAX,
        );
        halo_store::TaskRecord {
            task_id: self.task_id.clone(),
            agent: self.agent.as_str().to_string(),
            title: self.title.clone(),
            goal,
            state: self.state.as_str().to_string(),
            attribution: crate::mapping::attribution_core_to_str(&self.attribution).to_string(),
            manual_edit_paths: self.manual_edit_paths.iter().cloned().collect(),
            baseline_head: self.baseline.head.clone(),
            baseline_captured_at: self.baseline.captured_at.clone(),
            created_at: self.created_at.clone(),
            ended_at: self.ended_at.clone(),
            cancel_mode: self.cancel_mode.clone(),
        }
    }

    pub fn to_status(&self) -> halo_protocol::methods::task::TaskStatus {
        halo_protocol::methods::task::TaskStatus {
            task_id: self.task_id.clone(),
            agent: crate::mapping::agent_domain_to_dto(self.agent),
            title: self.title.clone(),
            state: crate::mapping::task_state_core_to_dto(self.state),
            attribution: crate::mapping::attribution_core_to_dto(&self.attribution),
            baseline: halo_protocol::methods::task::TaskBaseline {
                head: self.baseline.head.clone(),
                captured_at: self.baseline.captured_at.clone(),
            },
            created_at: self.created_at.clone(),
            ended_at: self.ended_at.clone(),
            cancel_mode: self
                .cancel_mode
                .as_deref()
                .map(crate::mapping::cancel_mode_from_str),
            latest_evidence_version: self.latest_evidence_version,
        }
    }
}

/// 将一条已规范化的会话消息加入当前任务的进程内记录，并追加 IPC 事件。
/// 调用者不持有任务锁时使用；未知或已终态任务不会生成可观察消息。
pub fn append_active_session_message(
    app: &Arc<Mutex<AppState>>,
    bus: &Arc<EventBus>,
    task_id: &str,
    role: halo_protocol::methods::task::TaskSessionMessageRole,
    text: &str,
) -> Option<halo_protocol::methods::task::TaskSessionMessage> {
    let message = {
        let mut app = lock(app);
        let task = app
            .task
            .as_mut()
            .filter(|task| task.task_id == task_id && !task.state.is_terminal())?;
        let message = task.append_session_message(role, text);
        bus.emit(
            Some(task_id),
            "task.session_message",
            json!({
                "role": message.role,
                "text": &message.text,
                "truncated": message.truncated,
            }),
        );
        message
    };
    Some(message)
}

pub struct AppState {
    pub workspace: Option<ActiveWorkspace>,
    pub pi: RuntimeSlot,
    pub opencode: RuntimeSlot,
    pub task: Option<ActiveTask>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            workspace: None,
            pi: RuntimeSlot::new(),
            opencode: RuntimeSlot::new(),
            task: None,
        }
    }

    pub fn slot(&self, agent: AgentKind) -> &RuntimeSlot {
        match agent {
            AgentKind::Pi => &self.pi,
            AgentKind::OpenCode => &self.opencode,
        }
    }

    pub fn slot_mut(&mut self, agent: AgentKind) -> &mut RuntimeSlot {
        match agent {
            AgentKind::Pi => &mut self.pi,
            AgentKind::OpenCode => &mut self.opencode,
        }
    }

    /// 是否存在非终态任务（一个活动工作区同一时刻只允许一个）。
    pub fn has_running_task(&self) -> bool {
        self.task
            .as_ref()
            .map(|t| !t.state.is_terminal())
            .unwrap_or(false)
    }
}

/// 运行时事件转发线程：State 事件更新槽位并推送 runtime.state；
/// 全部事件（含 State）转发给当前任务路由，让任务编排能感知运行时失败。
pub fn spawn_runtime_forwarder(
    app: Arc<Mutex<AppState>>,
    bus: Arc<EventBus>,
    agent: AgentKind,
    generation: u64,
    rx: Receiver<RuntimeEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for ev in rx {
            if let RuntimeEvent::State(s) = &ev {
                let update = {
                    let mut guard = lock(&app);
                    let slot = guard.slot_mut(agent);
                    if slot.generation != generation {
                        None
                    } else {
                        let changed = slot.last_state != *s;
                        slot.last_state = s.clone();
                        let version = slot.version.clone();
                        Some((runtime_state_payload(agent, s, version), changed))
                    }
                };
                if let Some((payload, true)) = update {
                    bus.emit(None, "runtime.state", payload);
                }
            }
            let task_tx = {
                let guard = lock(&app);
                let slot = guard.slot(agent);
                (slot.generation == generation)
                    .then(|| slot.task_tx.clone())
                    .flatten()
            };
            if let Some(tx) = task_tx {
                let _ = tx.send(ev);
            }
        }
    })
}

/// 在失效旧运行时事件流之后，显式发布当前槽位的最终停止状态。
/// 这样旧 forwarder 不会覆盖新实例，事件订阅者也不会错过 stopped。
pub fn mark_slot_stopped(app: &Arc<Mutex<AppState>>, bus: &Arc<EventBus>, agent: AgentKind) {
    let (payload, changed) = {
        let mut guard = lock(app);
        let slot = guard.slot_mut(agent);
        let state = RuntimeState::Stopped;
        let changed = slot.last_state != state;
        slot.last_state = state.clone();
        let payload = runtime_state_payload(agent, &state, slot.version.clone());
        (payload, changed)
    };
    if changed {
        bus.emit(None, "runtime.state", payload);
    }
}

/// 停止并清理一个槽位的受管运行时（工作区切换/撤销信任/关闭时使用）。
/// 旧句柄事件在 generation 推进后被忽略，因此最终 stopped 由当前槽位显式广播。
pub fn stop_slot(
    app: &Arc<Mutex<AppState>>,
    bus: &Arc<EventBus>,
    agent: AgentKind,
    grace: Duration,
) {
    let handle = {
        let mut guard = lock(app);
        let slot = guard.slot_mut(agent);
        slot.task_tx = None;
        slot.advance_generation();
        slot.handle.take()
    };
    if let Some(h) = handle {
        h.stop(grace);
        mark_slot_stopped(app, bus, agent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use halo_protocol::methods::task::TaskSessionMessageRole;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct StoppableHandle {
        stopped: AtomicBool,
    }

    impl AgentHandle for StoppableHandle {
        fn run_task(&self, _spec: &RunTaskSpec) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn cancel_native(&self) {}

        fn stop(&self, _grace: Duration) -> StopOutcome {
            self.stopped.store(true, Ordering::SeqCst);
            StopOutcome::Graceful
        }

        fn state(&self) -> RuntimeState {
            RuntimeState::Ready
        }
    }

    #[test]
    fn stale_runtime_events_cannot_override_a_newer_instance() {
        let app = Arc::new(Mutex::new(AppState::new()));
        let (outbound_tx, outbound_rx) = unbounded();
        let bus = Arc::new(EventBus::new(outbound_tx));

        let first_generation = {
            let mut guard = lock(&app);
            guard.slot_mut(AgentKind::OpenCode).advance_generation()
        };
        let (old_tx, old_rx) = unbounded();
        let old_forwarder = spawn_runtime_forwarder(
            Arc::clone(&app),
            Arc::clone(&bus),
            AgentKind::OpenCode,
            first_generation,
            old_rx,
        );

        let second_generation = {
            let mut guard = lock(&app);
            guard.slot_mut(AgentKind::OpenCode).advance_generation()
        };
        let (new_tx, new_rx) = unbounded();
        let new_forwarder = spawn_runtime_forwarder(
            Arc::clone(&app),
            Arc::clone(&bus),
            AgentKind::OpenCode,
            second_generation,
            new_rx,
        );

        old_tx
            .send(RuntimeEvent::State(RuntimeState::Failed {
                reason: "过期实例失败".to_string(),
                recovery_hint: "不应覆盖新实例".to_string(),
            }))
            .unwrap();
        new_tx
            .send(RuntimeEvent::State(RuntimeState::Ready))
            .unwrap();
        drop(old_tx);
        drop(new_tx);
        old_forwarder.join().unwrap();
        new_forwarder.join().unwrap();

        assert_eq!(
            lock(&app).slot(AgentKind::OpenCode).last_state,
            RuntimeState::Ready
        );
        let emitted: Vec<_> = outbound_rx.try_iter().collect();
        assert_eq!(emitted.len(), 1, "过期实例不得推送 runtime.state");
    }

    #[test]
    fn stop_slot_publishes_stopped_after_invalidating_old_forwarder() {
        let app = Arc::new(Mutex::new(AppState::new()));
        let (outbound_tx, outbound_rx) = unbounded();
        let bus = Arc::new(EventBus::new(outbound_tx));
        let handle = Arc::new(StoppableHandle {
            stopped: AtomicBool::new(false),
        });

        {
            let mut guard = lock(&app);
            let slot = guard.slot_mut(AgentKind::OpenCode);
            slot.last_state = RuntimeState::Ready;
            slot.version = Some("1.18.5".to_string());
            slot.handle = Some(handle.clone());
        }

        stop_slot(&app, &bus, AgentKind::OpenCode, Duration::ZERO);

        assert!(handle.stopped.load(Ordering::SeqCst));
        assert_eq!(
            lock(&app).slot(AgentKind::OpenCode).last_state,
            RuntimeState::Stopped
        );
        let outbound = outbound_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let crate::server::Outbound::Event(event) = outbound else {
            panic!("停止必须广播 runtime.state 事件");
        };
        assert_eq!(event.event, "runtime.state");
        assert_eq!(event.payload["agent"], "opencode");
        assert_eq!(event.payload["state"], "stopped");
        assert_eq!(event.payload["version"], "1.18.5");
    }

    #[test]
    fn active_session_message_is_redacted_and_capped_before_it_is_stored_or_emitted() {
        let app = Arc::new(Mutex::new(AppState::new()));
        let (outbound_tx, outbound_rx) = unbounded();
        let bus = Arc::new(EventBus::new(outbound_tx));
        let long = "x".repeat(halo_core::limits::TRACE_TEXT_MAX + 64);
        let raw = format!("password=do-not-leak {long}");

        let task = ActiveTask {
            task_id: "task-session".to_string(),
            agent: AgentKind::OpenCode,
            title: "活动会话".to_string(),
            instructions: "首条任务说明".to_string(),
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
            cancel_tx: None,
        };
        lock(&app).task = Some(task);

        let message = append_active_session_message(
            &app,
            &bus,
            "task-session",
            TaskSessionMessageRole::Agent,
            &raw,
        )
        .expect("活动任务应接收会话消息");

        assert_eq!(message.role, TaskSessionMessageRole::Agent);
        assert!(message.truncated);
        assert!(message.text.len() <= halo_core::limits::TRACE_TEXT_MAX);
        assert!(!message.text.contains("do-not-leak"));
        assert!(message.text.contains("[REDACTED]"));
        assert_eq!(
            lock(&app).task.as_ref().unwrap().session_messages,
            vec![message.clone()]
        );

        let crate::server::Outbound::Event(event) = outbound_rx.recv().unwrap() else {
            panic!("会话消息必须生成 IPC 事件");
        };
        assert_eq!(event.task_id.as_deref(), Some("task-session"));
        assert_eq!(event.event, "task.session_message");
        assert_eq!(event.payload["role"], "agent");
        assert_eq!(event.payload["truncated"], true);
        assert!(!event.payload.to_string().contains("do-not-leak"));
    }
}
