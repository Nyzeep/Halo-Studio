//! AppState：活动工作区、受管运行时句柄、当前任务的进程内状态。
//! 持久化真相在 halo-store；这里只保存运行期上下文与路由。

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use serde_json::Value;

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
    /// 任务事件路由：当前任务的事件消费端；无任务时为 None。
    pub task_tx: Option<Sender<RuntimeEvent>>,
}

impl RuntimeSlot {
    fn new() -> Self {
        RuntimeSlot {
            last_state: RuntimeState::NotProbed,
            version: None,
            handle: None,
            task_tx: None,
        }
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
    pub baseline: halo_core::Baseline,
    pub created_at: String,
    pub ended_at: Option<String>,
    pub cancel_mode: Option<String>,
    pub latest_evidence_version: u32,
    /// Agent 原生运行时最近一次报告的验证结论。
    pub verification_agent: Option<halo_core::Verification>,
    /// 用户显式标记的"未执行"结论；证据落库时优先级高于 Agent 缺省。
    pub verification_user: Option<halo_core::Verification>,
    /// 取消请求通道（发往任务编排线程）。
    pub cancel_tx: Option<Sender<()>>,
}

impl ActiveTask {
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
    rx: Receiver<RuntimeEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for ev in rx {
            if let RuntimeEvent::State(s) = &ev {
                let (payload, changed): (Value, bool) = {
                    let mut guard = lock(&app);
                    let slot = guard.slot_mut(agent);
                    let changed = slot.last_state != *s;
                    slot.last_state = s.clone();
                    let version = slot.version.clone();
                    (runtime_state_payload(agent, s, version), changed)
                };
                if changed {
                    bus.emit(None, "runtime.state", payload);
                }
            }
            let task_tx = { lock(&app).slot(agent).task_tx.clone() };
            if let Some(tx) = task_tx {
                let _ = tx.send(ev);
            }
        }
    })
}

/// 停止并清理一个槽位的受管运行时（工作区切换/撤销信任/关闭时使用）。
/// runtime.state 事件由转发线程按句柄的真实事件播报，这里只兜底记录最终状态。
pub fn stop_slot(app: &Arc<Mutex<AppState>>, _bus: &Arc<EventBus>, agent: AgentKind, grace: Duration) {
    let handle = {
        let mut guard = lock(app);
        let slot = guard.slot_mut(agent);
        slot.task_tx = None;
        slot.handle.take()
    };
    if let Some(h) = handle {
        h.stop(grace);
        lock(app).slot_mut(agent).last_state = RuntimeState::Stopped;
    }
}
