//! history.* 方法（IPC 文档 3.7 节）。
//! 本地历史只保存脱敏、大小受限的摘要与 Diff 证据。

use serde::{Deserialize, Serialize};

use super::review::{Decision, FileChange, ReviewOutcome, Verification};
use super::task::TaskStatus;
use super::Attribution;

/// history.list params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HistoryListParams {
    pub limit: u32,
}

/// history.list result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HistoryListResult {
    pub tasks: Vec<TaskStatus>,
    pub decisions: Vec<Decision>,
}

/// history.evidence params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HistoryEvidenceParams {
    pub task_id: String,
}

/// 证据摘要中的文件条目：不含逐文件 diff 正文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceFileSummary {
    pub path: String,
    pub change: FileChange,
    pub truncated: bool,
}

/// ReviewBundle 的摘要形式（IPC 文档 3.7 节：不含逐文件 diff 正文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceSummary {
    pub task_id: String,
    pub evidence_version: u32,
    pub is_latest: bool,
    pub outcome: ReviewOutcome,
    pub attribution: Attribution,
    pub attribution_reasons: Vec<String>,
    pub summary: String,
    pub files: Vec<EvidenceFileSummary>,
    pub verification: Verification,
    pub baseline_dirty_files: Vec<String>,
}

/// history.evidence result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HistoryEvidenceResult {
    pub versions: Vec<EvidenceSummary>,
}
