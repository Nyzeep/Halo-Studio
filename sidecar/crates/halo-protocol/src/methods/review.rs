//! review.* / delivery.* 方法（IPC 文档 3.5 节）。审查载体只读，无任何写入能力。

use serde::{Deserialize, Serialize};

use super::{Attribution, VerificationSource, VerificationStatus};

/// review.get params（version 省略 = 最新）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReviewGetParams {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    Finished,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Modified,
    Added,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReviewFile {
    pub path: String,
    pub change: FileChange,
    pub diff: String,
    pub truncated: bool,
}

/// 验证结论（含来源）；task.verification 事件 payload 同构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Verification {
    pub status: VerificationStatus,
    pub detail: String,
    pub source: VerificationSource,
}

/// review.get result —— 只读，无任何写入/编辑/保存能力。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReviewBundle {
    pub task_id: String,
    pub evidence_version: u32,
    pub is_latest: bool,
    pub outcome: ReviewOutcome,
    pub attribution: Attribution,
    pub attribution_reasons: Vec<String>,
    /// 脱敏、大小受限
    pub summary: String,
    pub files: Vec<ReviewFile>,
    pub verification: Verification,
    /// 任务前已有修改，明确与关联变更区分
    pub baseline_dirty_files: Vec<String>,
}

/// delivery.accept params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeliveryAcceptParams {
    pub task_id: String,
    pub evidence_version: u32,
}

/// delivery.reject params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeliveryRejectParams {
    pub task_id: String,
    pub evidence_version: u32,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Accepted,
    Rejected,
}

/// Decision —— 只记录任务结论，不触发任何 Git 操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Decision {
    pub kind: DecisionKind,
    pub task_id: String,
    pub evidence_version: u32,
    pub decided_at: String,
    pub reason: Option<String>,
}

/// delivery.accept / delivery.reject result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DecisionResult {
    pub decision: Decision,
}
