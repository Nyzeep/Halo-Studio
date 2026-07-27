//! IPC v1 消息契约层。权威定义见 docs/ipc-protocol.md 与 docs/module-contracts.md 第 1 节。
//!
//! 本 crate 只包含纯类型 + 封包 IO 助手，不含任何业务逻辑；
//! `halo-sidecar` 负责协议 DTO 与各业务 crate 自有类型之间的映射。

pub mod envelope;
pub mod error;
pub mod io;
pub mod methods;

/// 协议主版本；当前唯一版本为 1。
pub const PROTOCOL_VERSION: u32 = 1;

/// 单行 JSONL 上限（1 MiB），超限即协议错误。
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

pub use envelope::{Event, RequestEnvelope, Response};
pub use error::{ErrorBody, ErrorCode, ProtocolError};
pub use io::{read_message, write_message, Inbound};
