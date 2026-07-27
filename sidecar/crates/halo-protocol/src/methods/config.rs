//! config.* 方法（IPC 文档 3.2 节）。
//! 凭据明文永不出现在本模块任何字段中；只承载凭据引用名。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::AgentKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingLevel {
    Off,
    Low,
    Medium,
    High,
}

/// config.save params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LaunchConfigInput {
    pub name: String,
    pub agent: AgentKind,
    pub executable_path: String,
    pub model: String,
    pub thinking_level: ThinkingLevel,
    /// Windows 凭据管理器条目名（引用名），或 null
    pub credential_ref: Option<String>,
    pub extra_args: Vec<String>,
    /// 仅白名单内变量名可出现，违规返回 ENV_NOT_WHITELISTED
    pub env_overrides: BTreeMap<String, String>,
}

/// LaunchConfig = LaunchConfigInput + config_id/created_at/updated_at
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LaunchConfig {
    pub config_id: String,
    pub name: String,
    pub agent: AgentKind,
    pub executable_path: String,
    pub model: String,
    pub thinking_level: ThinkingLevel,
    pub credential_ref: Option<String>,
    pub extra_args: Vec<String>,
    pub env_overrides: BTreeMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

/// config.list params（空对象）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListConfigsParams {}

/// config.list result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListConfigsResult {
    pub configs: Vec<LaunchConfig>,
}

/// config.save result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SaveConfigResult {
    pub config: LaunchConfig,
}

/// config.delete params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeleteConfigParams {
    pub config_id: String,
}

/// config.delete result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeleteConfigResult {
    pub deleted: bool,
}

/// config.credential_check params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CredentialCheckParams {
    pub credential_ref: String,
}

/// config.credential_check result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CredentialCheckResult {
    pub exists: bool,
    pub store_available: bool,
}
