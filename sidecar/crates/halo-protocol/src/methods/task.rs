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
}
