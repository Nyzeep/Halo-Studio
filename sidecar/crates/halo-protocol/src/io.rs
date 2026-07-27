//! 封包 IO：单行 JSONL 的写出与读入校验（v == 1、kind、单行 ≤ MAX_LINE_BYTES）。

use serde::Serialize;
use serde_json::Value;

use crate::envelope::RequestEnvelope;
use crate::error::ProtocolError;
use crate::{MAX_LINE_BYTES, PROTOCOL_VERSION};

/// Sidecar 入站只接受请求封包；响应与事件方向相反，出现即协议错误。
#[derive(Debug, Clone, PartialEq)]
pub enum Inbound {
    Request(RequestEnvelope),
}

/// 序列化为单行 JSON 并追加 `\n` 写出；超过单行上限拒绝写出。
pub fn write_message<W: std::io::Write>(
    w: &mut W,
    msg: &impl Serialize,
) -> Result<(), ProtocolError> {
    let line = serde_json::to_string(msg).map_err(|e| ProtocolError::Serialize {
        detail: e.to_string(),
    })?;
    if line.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::LineTooLong { actual: line.len() });
    }
    w.write_all(line.as_bytes())?;
    w.write_all(b"\n")?;
    w.flush()?;
    Ok(())
}

/// 解析一行入站消息：先查长度，再解析 JSON，然后校验 v 与 kind。
/// 错误文案不回显行内容，避免把可能包含的敏感数据带进日志。
pub fn read_message(line: &str) -> Result<Inbound, ProtocolError> {
    let trimmed = line.trim_end_matches(|c| c == '\r' || c == '\n');
    if trimmed.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::LineTooLong {
            actual: trimmed.len(),
        });
    }

    let value: Value = serde_json::from_str(trimmed).map_err(|e| ProtocolError::Parse {
        detail: e.to_string(),
    })?;

    let v = match value.get("v") {
        None => {
            return Err(ProtocolError::Parse {
                detail: "缺少字段 v".to_string(),
            })
        }
        Some(raw) => raw.as_u64().ok_or_else(|| ProtocolError::Parse {
            detail: "字段 v 必须是非负整数".to_string(),
        })?,
    };
    if v != u64::from(PROTOCOL_VERSION) {
        return Err(ProtocolError::UnsupportedVersion { found: v });
    }

    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ProtocolError::Parse {
            detail: "缺少字段 kind 或其类型不是字符串".to_string(),
        })?;

    match kind {
        "request" => {
            let req: RequestEnvelope =
                serde_json::from_value(value).map_err(|e| ProtocolError::Parse {
                    detail: e.to_string(),
                })?;
            Ok(Inbound::Request(req))
        }
        other => Err(ProtocolError::UnexpectedKind {
            found: other.to_string(),
        }),
    }
}
