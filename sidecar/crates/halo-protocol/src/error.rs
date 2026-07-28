//! 错误码、错误体与协议层错误类型。
//! `ErrorCode` 的序列化字符串必须与 protocol/v1/envelope.schema.json 的枚举一字不差。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{MAX_LINE_BYTES, PROTOCOL_VERSION};

/// IPC 文档第 5 节全部错误码；serde 输出 SCREAMING_SNAKE_CASE 稳定字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    HelloRequired,
    ProtocolVersionUnsupported,
    MethodNotFound,
    InvalidParams,
    Internal,
    WorkspacePathInvalid,
    WorkspaceNotReadable,
    WorkspaceNotGit,
    WorkspaceNotTrusted,
    WorkspaceNotActive,
    WorkspaceIdentityChanged,
    CredentialStoreUnavailable,
    CredentialNotFound,
    EnvNotWhitelisted,
    ConfigNotFound,
    ConfigConflict,
    RuntimeNotReady,
    RuntimeProbeFailed,
    RuntimeVersionMismatch,
    RuntimeAlreadyRunning,
    RuntimeCapabilityUnavailable,
    TaskAlreadyRunning,
    TaskRunning,
    TaskNotFound,
    TaskStillRunning,
    TaskNotReviewable,
    EvidenceNotFound,
    EvidenceNotLatest,
    EventGap,
    HandoffNotFound,
    LineTooLong,
    ParseError,
    FsPathOutsideWorkspace,
    FsTooLarge,
    FsBinary,
    FsConflict,
    FsNotFound,
    FsAlreadyExists,
    FsGitProtected,
}

/// 响应中的错误体；`message` 为中文用户可读文案，绝不携带凭据明文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// details 为 Null 时不输出该字段（schema 中 details 可选且必须为对象）。
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ErrorBody {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }

    pub fn with_details(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        ErrorBody {
            code,
            message: message.into(),
            details,
        }
    }
}

/// 封包读写与校验失败；错误文案不回显行内容，避免泄露任何敏感数据。
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("单行长度 {actual} 字节，超过上限 {max} 字节", max = MAX_LINE_BYTES)]
    LineTooLong { actual: usize },

    #[error("JSON 解析失败：{detail}")]
    Parse { detail: String },

    #[error("协议版本不受支持：期望 {expected}，实际 {found}", expected = PROTOCOL_VERSION)]
    UnsupportedVersion { found: u64 },

    #[error("封包 kind 不受支持：{found}")]
    UnexpectedKind { found: String },

    #[error("消息序列化失败：{detail}")]
    Serialize { detail: String },

    #[error("封包写入失败：{0}")]
    Io(#[from] std::io::Error),
}
