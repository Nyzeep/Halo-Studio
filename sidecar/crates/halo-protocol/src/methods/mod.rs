//! 各方法的 typed params/result 结构体，字段与 docs/ipc-protocol.md 一字不差。
//! 结构体字段 snake_case；枚举值一律小写蛇形。
//! 本文件承载 `sidecar.hello` 与多个子模块共享的枚举。

pub mod config;
pub mod fs;
pub mod handoff;
pub mod history;
pub mod review;
pub mod runtime;
pub mod task;
pub mod workspace;

use serde::{Deserialize, Serialize};

/// sidecar.hello params
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HelloParams {
    pub app_protocol_versions: Vec<u32>,
    pub app_version: String,
}

/// sidecar.hello result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HelloResult {
    pub protocol_version: u32,
    pub sidecar_version: String,
    pub capabilities: Vec<String>,
}

/// 受管应用只有 Pi 与 OpenCode 两种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Pi,
    Opencode,
}

/// 任务归因：基线前修改永不归因 Agent；人工介入 → mixed。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribution {
    AgentOnly,
    Mixed,
}

/// 验证结论只有通过、失败或未执行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Passed,
    Failed,
    NotRun,
}

/// 验证结论来源：Agent 原生运行时或用户显式标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSource {
    Agent,
    UserMarked,
}
