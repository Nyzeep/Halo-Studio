//! runtime.* 方法（IPC 文档 3.3 节）。
//! OpenCode 的端口与认证信息不出现在任何 result/event 中，因此本模块没有对应字段。

use serde::{Deserialize, Serialize};

use super::AgentKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    NotProbed,
    Probing,
    Starting,
    Ready,
    Failed,
    Stopping,
    Stopped,
}

/// 每个受管应用独立健康状态，绝不合并为“全局在线”。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStateInfo {
    pub state: RuntimeState,
    /// failed 时用户可读原因
    pub reason: Option<String>,
    /// failed 时恢复建议
    pub recovery_hint: Option<String>,
    pub version: Option<String>,
}

/// runtime.probe params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeProbeParams {
    pub agent: AgentKind,
    pub config_id: String,
}

/// runtime.probe result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeProbeResult {
    pub agent: AgentKind,
    pub version: String,
    pub supported: bool,
}

/// runtime.start params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStartParams {
    pub agent: AgentKind,
    pub config_id: String,
}

/// runtime.start result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStartResult {
    pub state: RuntimeState,
}

/// runtime.stop params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStopParams {
    pub agent: AgentKind,
}

/// runtime.stop result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStopResult {
    pub state: RuntimeState,
}

/// runtime.status params（空对象）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStatusParams {}

/// runtime.status result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeStatusResult {
    pub pi: RuntimeStateInfo,
    pub opencode: RuntimeStateInfo,
}
