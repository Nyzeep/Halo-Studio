//! task.* 方法（IPC 文档 3.4 节）。

use serde::{Deserialize, Serialize};

use super::{AgentKind, Attribution, VerificationStatus};
use crate::envelope::Event;

/// task.create params —— 任务只携带用户显式提供的内容，绝不自动附带完整工作区或历史。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskSpec {
    pub agent: AgentKind,
    pub config_id: String,
    pub title: String,
    /// 必填任务目标
    pub instructions: String,
    /// 用户主动选取，可空
    #[serde(default)]
    pub files: Vec<String>,
    /// 用户提供的已有 Diff，可空
    #[serde(default)]
    pub base_diff: Option<String>,
    /// 补充说明
    #[serde(default)]
    pub notes: Option<String>,
    /// 从交接包接续时携带
    #[serde(default)]
    pub handoff_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Created,
    Running,
    WaitingDeveloper,
    AwaitingAction,
    Finishing,
    ReviewReady,
    Accepted,
    Rejected,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelMode {
    Native,
    Forced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskBaseline {
    pub head: Option<String>,
    pub captured_at: String,
}

/// TaskStatus（task.create/task.status 的 result 载体，亦是 task.state 事件 payload 的一部分）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskStatus {
    pub task_id: String,
    pub agent: AgentKind,
    pub title: String,
    pub state: TaskState,
    pub attribution: Attribution,
    pub baseline: TaskBaseline,
    pub created_at: String,
    pub ended_at: Option<String>,
    pub cancel_mode: Option<CancelMode>,
    pub latest_evidence_version: u32,
}

/// task.create result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTaskResult {
    pub task: TaskStatus,
}

/// task.cancel params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CancelTaskParams {
    pub task_id: String,
}

/// task.cancel result（结果经事件）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CancelTaskResult {
    pub accepted: bool,
}

/// task.mark_manual_edit params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarkManualEditParams {
    pub task_id: String,
    pub note: String,
}

/// task.mark_manual_edit result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarkManualEditResult {
    pub attribution: Attribution,
}

/// task.mark_verification params（用户显式标记未执行）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarkVerificationParams {
    pub task_id: String,
    pub status: VerificationStatus,
    pub note: String,
}

/// task.mark_verification result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarkVerificationResult {
    pub ok: bool,
}

/// task.status params：{"task_id": …} 或 {}（当前任务）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskStatusParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// task.status result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskStatusResult {
    pub task: Option<TaskStatus>,
}

/// 活动受管任务会话中可展示的一条文本消息。
/// 该记录只在 Sidecar 内存中保存；不会进入任务历史、证据或审查包。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSessionMessageRole {
    User,
    Agent,
}

/// 经过脱敏和长度限制的活动会话消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskSessionMessage {
    pub role: TaskSessionMessageRole,
    pub text: String,
    pub truncated: bool,
}

/// 当前活动任务中等待开发者一次性决定的操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskActionKind {
    Permission,
    Clarification,
}

/// 操作请求的开发者决定。权限没有永久授权；澄清只能回答或拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDecision {
    AllowOnce,
    Reject,
    Answer,
}

/// 仅当前任务进程内保留的可展示操作请求。
/// 远程 OpenCode session、端口和认证信息不属于该 DTO。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskActionRequest {
    pub request_id: String,
    pub kind: TaskActionKind,
    pub prompt: String,
    /// 决议已经提交给原生 Agent，但尚未收到真实反馈时保持卡片可见且禁用重复操作。
    pub decision_sent: bool,
}

/// task.resolve_action params。request_id 必须属于当前 task_id 的活动请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolveActionParams {
    pub task_id: String,
    pub request_id: String,
    pub decision: ActionDecision,
    /// 仅 clarification + answer 使用；其他决定必须为 null。
    pub answer: Option<String>,
}

/// task.resolve_action result。accepted 只表示 Sidecar 已把一次性决定提交给 Agent；
/// 任务状态仍由后续的真实 Agent 事件推进。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolveActionResult {
    pub accepted: bool,
}

/// task.snapshot params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskSnapshotParams {
    pub after_seq: u64,
}

/// task.snapshot result；缓冲不足覆盖 after_seq 时返回错误 EVENT_GAP
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskSnapshotResult {
    pub task: Option<TaskStatus>,
    pub last_seq: u64,
    pub events: Vec<Event>,
    /// 当前活动任务的内存会话记录；历史任务始终返回空数组。
    pub session_messages: Vec<TaskSessionMessage>,
    /// 当前活动任务中等待决议的操作请求；不持久化到历史或证据。
    pub action_requests: Vec<TaskActionRequest>,
}
