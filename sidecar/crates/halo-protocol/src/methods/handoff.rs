//! handoff.* 方法（IPC 文档 3.6 节）。
//! HandoffPackage 构造上就不可能包含完整对话、原始工具日志、凭据或配置文件。

use serde::{Deserialize, Serialize};

use super::{AgentKind, VerificationStatus};

/// handoff.preview params（selected_files 为 null = 默认全部关联文件）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HandoffPreviewParams {
    pub task_id: String,
    pub selected_files: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SelectedChange {
    pub path: String,
    pub diff: String,
}

/// 交接包内的验证结论（无 source 字段，与 ReviewBundle.verification 形状不同）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HandoffVerification {
    pub status: VerificationStatus,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HandoffPackage {
    /// preview 时为 null
    pub handoff_id: Option<String>,
    pub task_id: String,
    pub source_agent: AgentKind,
    pub target_agent: Option<AgentKind>,
    /// 任务目标
    pub goal: String,
    /// 主 Agent 摘要（脱敏、限长）
    pub summary: String,
    pub selected_changes: Vec<SelectedChange>,
    pub verification: HandoffVerification,
    pub created_at: Option<String>,
}

/// handoff.preview result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HandoffPreviewResult {
    pub package: HandoffPackage,
}

/// handoff.create params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HandoffCreateParams {
    pub task_id: String,
    pub target_agent: AgentKind,
    pub selected_files: Vec<String>,
}

/// handoff.create result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HandoffCreateResult {
    pub handoff_id: String,
    pub package: HandoffPackage,
}
