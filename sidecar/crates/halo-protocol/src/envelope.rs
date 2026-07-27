//! 三种封包类型（请求 / 响应 / 事件）。
//! 公开结构体与 docs/module-contracts.md 第 1 节保持一致（不含 kind 字段）；
//! `kind` 由私有 wire 结构在序列化时注入、反序列化时校验。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ErrorBody;
use crate::PROTOCOL_VERSION;

const KIND_REQUEST: &str = "request";
const KIND_RESPONSE: &str = "response";
const KIND_EVENT: &str = "event";

/// 请求封包（UI → Sidecar）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RequestWire", into = "RequestWire")]
pub struct RequestEnvelope {
    pub v: u32,
    pub id: String,
    pub method: String,
    pub params: Value,
}

#[derive(Serialize, Deserialize)]
struct RequestWire {
    v: u32,
    kind: String,
    id: String,
    method: String,
    params: Value,
}

impl From<RequestEnvelope> for RequestWire {
    fn from(r: RequestEnvelope) -> Self {
        RequestWire {
            v: r.v,
            kind: KIND_REQUEST.to_string(),
            id: r.id,
            method: r.method,
            params: r.params,
        }
    }
}

impl TryFrom<RequestWire> for RequestEnvelope {
    type Error = String;

    fn try_from(w: RequestWire) -> Result<Self, Self::Error> {
        if w.kind != KIND_REQUEST {
            return Err(format!("kind 应为 {KIND_REQUEST}，实际为 {}", w.kind));
        }
        Ok(RequestEnvelope {
            v: w.v,
            id: w.id,
            method: w.method,
            params: w.params,
        })
    }
}

/// 响应封包（Sidecar → UI，与请求同 id；ok=false 时带 error）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ResponseWire", into = "ResponseWire")]
pub struct Response {
    pub v: u32,
    pub id: String,
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<ErrorBody>,
}

impl Response {
    pub fn success(id: impl Into<String>, result: Value) -> Self {
        Response {
            v: PROTOCOL_VERSION,
            id: id.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(id: impl Into<String>, error: ErrorBody) -> Self {
        Response {
            v: PROTOCOL_VERSION,
            id: id.into(),
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ResponseWire {
    v: u32,
    kind: String,
    id: String,
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
}

impl From<Response> for ResponseWire {
    fn from(r: Response) -> Self {
        ResponseWire {
            v: r.v,
            kind: KIND_RESPONSE.to_string(),
            id: r.id,
            ok: r.ok,
            result: r.result,
            error: r.error,
        }
    }
}

impl TryFrom<ResponseWire> for Response {
    type Error = String;

    fn try_from(w: ResponseWire) -> Result<Self, Self::Error> {
        if w.kind != KIND_RESPONSE {
            return Err(format!("kind 应为 {KIND_RESPONSE}，实际为 {}", w.kind));
        }
        Ok(Response {
            v: w.v,
            id: w.id,
            ok: w.ok,
            result: w.result,
            error: w.error,
        })
    }
}

/// 事件封包（Sidecar → UI，全局单调递增 seq，由唯一写线程分配）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "EventWire", into = "EventWire")]
pub struct Event {
    pub v: u32,
    pub seq: u64,
    /// UTC ISO-8601（YYYY-MM-DDThh:mm:ssZ）。
    pub ts: String,
    pub task_id: Option<String>,
    pub event: String,
    pub payload: Value,
}

#[derive(Serialize, Deserialize)]
struct EventWire {
    v: u32,
    kind: String,
    seq: u64,
    ts: String,
    task_id: Option<String>,
    event: String,
    payload: Value,
}

impl From<Event> for EventWire {
    fn from(e: Event) -> Self {
        EventWire {
            v: e.v,
            kind: KIND_EVENT.to_string(),
            seq: e.seq,
            ts: e.ts,
            task_id: e.task_id,
            event: e.event,
            payload: e.payload,
        }
    }
}

impl TryFrom<EventWire> for Event {
    type Error = String;

    fn try_from(w: EventWire) -> Result<Self, Self::Error> {
        if w.kind != KIND_EVENT {
            return Err(format!("kind 应为 {KIND_EVENT}，实际为 {}", w.kind));
        }
        Ok(Event {
            v: w.v,
            seq: w.seq,
            ts: w.ts,
            task_id: w.task_id,
            event: w.event,
            payload: w.payload,
        })
    }
}
