//! workspace.* 方法（IPC 文档 3.1 节）。

use serde::{Deserialize, Serialize};

/// workspace.open params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OpenWorkspaceParams {
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustDecision {
    Trust,
    Revoke,
}

/// workspace.trust params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TrustWorkspaceParams {
    pub workspace_id: String,
    pub decision: TrustDecision,
}

/// workspace.close params（空对象）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CloseWorkspaceParams {}

/// workspace.close result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CloseWorkspaceResult {
    pub closed: bool,
}

/// workspace.status params（空对象）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceStatusParams {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    Untrusted,
    Trusted,
}

/// WorkspaceStatus（workspace.open / workspace.trust 的 result，亦是 workspace.changed 事件的 payload）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceStatus {
    pub active: bool,
    pub workspace_id: String,
    /// canonicalize 后的真实路径
    pub real_path: String,
    pub git_root: String,
    /// 仓库首个提交，用于目录替换检测
    pub root_commit: Option<String>,
    pub trust: TrustState,
    /// true 时信任已被降级，需要重新确认
    pub identity_changed: bool,
}

/// 无活动工作区时 workspace.status 返回 {"active": false}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InactiveWorkspace {
    pub active: bool,
}

/// workspace.status result：WorkspaceStatus 或 {"active": false}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceStatusResult {
    Active(WorkspaceStatus),
    Inactive(InactiveWorkspace),
}
