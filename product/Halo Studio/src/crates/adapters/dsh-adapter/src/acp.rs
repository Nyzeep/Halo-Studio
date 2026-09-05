//! JSON-RPC client for the DSH controlled child (ACP v1 over stdio ndjson).
//!
//! Wire facts per `docs/architecture/dsh-adapter-protocol-research-20260905.md`:
//! every frame is one LF-delimited JSON record; `id + method` is a
//! server-to-client request (only `session/request_permission` is consumed),
//! `id` alone is a response, `method` alone is a notification. Unknown
//! notifications and unknown update kinds are filtered client-side and never
//! fail the session. Malformed frames are silently ignored, matching the
//! upstream transport. Native executor session ids, raw tool-call ids and raw
//! tool inputs never cross the port: they are digested at this boundary.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use halo_runtime_ports::{PortError, PortErrorKind, PortResult};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::{broadcast, oneshot, Mutex, Notify};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::framing::{decode_jsonl_record, encode_jsonl};
use crate::DshEvent;

/// ACP server-to-client one-shot permission request (research section 3.2).
pub(crate) const METHOD_REQUEST_PERMISSION: &str = "session/request_permission";
const METHOD_SESSION_UPDATE: &str = "session/update";
const METHOD_SESSION_EVENT: &str = "session/event";
const METHOD_SESSION_STATUS: &str = "session/status";

const MAX_TOOL_NAME_BYTES: usize = 128;
const JSONRPC_UNKNOWN_METHOD: i64 = -32601;

/// The adapter-owned binding for one pending permission request. The Halo
/// operation id is the single-use capability that authorizes exactly one
/// answer; the raw ACP rpc id never leaves this module.
pub(crate) struct PendingPermission {
    pub rpc_id: Value,
    pub allow_option_id: Option<String>,
    pub reject_option_id: Option<String>,
}

pub(crate) struct AcpConnection {
    stdin: Mutex<Option<ChildStdin>>,
    pending: Mutex<HashMap<i64, oneshot::Sender<PortResult<Value>>>>,
    permissions: Mutex<HashMap<String, PendingPermission>>,
    native_session: Mutex<Option<String>>,
    prompt_rpc_id: Mutex<Option<i64>>,
    events: broadcast::Sender<DshEvent>,
    halo_session_id: String,
    next_request_id: AtomicI64,
    idle_epoch: AtomicU64,
    idle: Notify,
    operation_timeout: Duration,
}

impl AcpConnection {
    /// Wires the connection to a freshly spawned child and starts the reader
    /// task. `halo_session_id` is the Halo-local session every emitted event
    /// is correlated with; native ids stay behind this module.
    pub(crate) fn spawn(
        halo_session_id: String,
        stdin: ChildStdin,
        stdout: ChildStdout,
        events: broadcast::Sender<DshEvent>,
        operation_timeout: Duration,
    ) -> Arc<Self> {
        let connection = Arc::new(Self {
            stdin: Mutex::new(Some(stdin)),
            pending: Mutex::new(HashMap::new()),
            permissions: Mutex::new(HashMap::new()),
            native_session: Mutex::new(None),
            prompt_rpc_id: Mutex::new(None),
            events,
            halo_session_id,
            next_request_id: AtomicI64::new(0),
            idle_epoch: AtomicU64::new(0),
            idle: Notify::new(),
            operation_timeout,
        });
        tokio::spawn(read_stdout(connection.clone(), stdout));
        connection
    }

    pub(crate) fn allocate_request_id(&self) -> i64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) async fn set_native_session(&self, native_session: String) {
        *self.native_session.lock().await = Some(native_session);
    }

    pub(crate) async fn native_session(&self) -> Option<String> {
        self.native_session.lock().await.clone()
    }

    pub(crate) async fn set_prompt_rpc_id(&self, rpc_id: Option<i64>) {
        *self.prompt_rpc_id.lock().await = rpc_id;
    }

    pub(crate) async fn prompt_rpc_id(&self) -> Option<i64> {
        *self.prompt_rpc_id.lock().await
    }

    pub(crate) async fn request(
        &self,
        method: &str,
        params: Value,
        response_timeout: Duration,
    ) -> PortResult<Value> {
        let id = self.allocate_request_id();
        self.request_with_id(id, method, params, response_timeout)
            .await
    }

    pub(crate) async fn request_with_id(
        &self,
        id: i64,
        method: &str,
        params: Value,
        response_timeout: Duration,
    ) -> PortResult<Value> {
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let encoded = encode_jsonl(&message).map_err(|_| {
            PortError::new(PortErrorKind::InvalidRequest, "DSH request is not encodable")
        })?;
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);

        let write_result = self.write_frame(&encoded).await;
        if write_result.is_err() {
            self.pending.lock().await.remove(&id);
            return Err(PortError::new(
                PortErrorKind::Backend,
                "DSH stdin is unavailable",
            ));
        }

        match timeout(response_timeout, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(PortError::new(
                PortErrorKind::Backend,
                "DSH response stream closed",
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(PortError::new(
                    PortErrorKind::Timeout,
                    "DSH response timed out",
                ))
            }
        }
    }

    pub(crate) async fn notify(&self, method: &str, params: Value) -> PortResult<()> {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let encoded = encode_jsonl(&message).map_err(|_| {
            PortError::new(PortErrorKind::InvalidRequest, "DSH notification is not encodable")
        })?;
        self.write_frame(&encoded).await.map_err(|_| {
            PortError::new(PortErrorKind::Backend, "DSH stdin is unavailable")
        })
    }

    /// Answers a server-to-client request. The result envelope is exactly the
    /// JSON-RPC response frame for `rpc_id`.
    pub(crate) async fn send_result(&self, rpc_id: Value, result: Value) -> PortResult<()> {
        let message = json!({ "jsonrpc": "2.0", "id": rpc_id, "result": result });
        let encoded = encode_jsonl(&message).map_err(|_| {
            PortError::new(PortErrorKind::InvalidRequest, "DSH reply is not encodable")
        })?;
        self.write_frame(&encoded).await.map_err(|_| {
            PortError::new(PortErrorKind::Backend, "DSH stdin is unavailable")
        })
    }

    async fn send_error(&self, rpc_id: Value, code: i64, message: &str) {
        let frame = json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": code, "message": message }
        });
        if let Ok(encoded) = encode_jsonl(&frame) {
            let _ = self.write_frame(&encoded).await;
        }
    }

    async fn write_frame(&self, encoded: &[u8]) -> std::io::Result<()> {
        let mut guard = self.stdin.lock().await;
        let Some(stdin) = guard.as_mut() else {
            return Err(std::io::Error::other("DSH stdin is closed"));
        };
        stdin.write_all(encoded).await?;
        stdin.flush().await
    }

    /// Closes the owned transport: the first step of the reclaim ladder.
    pub(crate) async fn close_stdin(&self) {
        *self.stdin.lock().await = None;
    }

    pub(crate) async fn take_permission(&self, operation_id: &str) -> Option<PendingPermission> {
        self.permissions.lock().await.remove(operation_id)
    }

    /// Waits for the SDK channel's ordered idle settlement (`session/status:
    /// idle`) after an accepted prompt; the ACP channel settles with the
    /// `session/prompt` response itself and never waits here.
    pub(crate) async fn wait_idle(&self, settle_timeout: Duration) -> PortResult<()> {
        let observed = self.idle_epoch.load(Ordering::Acquire);
        let wait = async {
            loop {
                let notified = self.idle.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.idle_epoch.load(Ordering::Acquire) != observed {
                    return;
                }
                notified.await;
            }
        };
        timeout(settle_timeout, wait)
            .await
            .map_err(|_| PortError::new(PortErrorKind::Timeout, "DSH idle settlement timed out"))
    }

    fn emit(&self, event: DshEvent) {
        let _ = self.events.send(event);
    }
}

async fn read_stdout(connection: Arc<AcpConnection>, stdout: ChildStdout) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // Malformed frames are ignored, never fatal (upstream transport does
        // the same; Halo keeps its strict framing for records it does read).
        if let Ok(message) = decode_jsonl_record(line.as_bytes()) {
            handle_message(&connection, message).await;
        }
    }
    transport_ended(&connection).await;
}

async fn handle_message(connection: &Arc<AcpConnection>, message: Value) {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    match (method, message.get("id").cloned()) {
        (Some(method), Some(id)) => {
            if id.is_i64() || id.is_u64() || id.is_string() {
                handle_server_request(connection, &method, id, &message).await;
            } else {
                // An id that is neither number nor string cannot be echoed
                // back; the frame is filtered, never fatal.
            }
        }
        (None, Some(id)) => {
            if let Some(id) = id.as_i64() {
                resolve_response(connection, id, &message).await;
            }
        }
        (Some(method), None) => handle_notification(connection, &method, &message).await,
        (None, None) => {}
    }
}

async fn handle_server_request(
    connection: &Arc<AcpConnection>,
    method: &str,
    rpc_id: Value,
    message: &Value,
) {
    if method != METHOD_REQUEST_PERMISSION {
        // Only the one-shot permission flow is consumed; every other
        // server-to-client request is refused without touching the session.
        connection
            .send_error(rpc_id, JSONRPC_UNKNOWN_METHOD, "method is not consumed")
            .await;
        return;
    }
    let Some(params) = message.get("params") else {
        connection.send_error(rpc_id, JSONRPC_UNKNOWN_METHOD, "missing params").await;
        return;
    };
    handle_permission_request(connection, rpc_id, params).await;
}

async fn handle_permission_request(
    connection: &Arc<AcpConnection>,
    rpc_id: Value,
    params: &Value,
) {
    let options = params.get("options").and_then(Value::as_array);
    let mut allow_option_id = None;
    let mut reject_option_id = None;
    for option in options.into_iter().flatten() {
        let option_id = option.get("optionId").and_then(Value::as_str);
        match option.get("kind").and_then(Value::as_str) {
            Some("allow_once") => allow_option_id = option_id.map(str::to_string),
            Some("reject_once") => reject_option_id = option_id.map(str::to_string),
            _ => {}
        }
    }
    // Fail-closed: without a one-shot allow option there is nothing Halo's
    // closed decision vocabulary can express, so the request is answered
    // "cancelled" and no approval fact is fabricated.
    let Some(allow_option_id) = allow_option_id else {
        let _ = connection
            .send_result(rpc_id, json!({ "outcome": { "outcome": "cancelled" } }))
            .await;
        return;
    };

    let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let native_tool_call_id = tool_call
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if native_tool_call_id.is_empty() {
        let _ = connection
            .send_result(rpc_id, json!({ "outcome": { "outcome": "cancelled" } }))
            .await;
        return;
    }
    let tool_name = bounded(
        tool_call
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(native_tool_call_id),
        MAX_TOOL_NAME_BYTES,
    );
    let redacted_arguments = match tool_call.get("rawInput") {
        Some(raw_input) => format!(
            "dsh-input-{}",
            hex_digest(&serde_json::to_vec(raw_input).unwrap_or_default())[..16].to_string()
        ),
        None => String::new(),
    };

    let operation_id = format!("dsh-operation-{}", Uuid::new_v4());
    connection.permissions.lock().await.insert(
        operation_id.clone(),
        PendingPermission {
            rpc_id,
            allow_option_id: Some(allow_option_id),
            reject_option_id,
        },
    );
    connection.emit(DshEvent::PermissionRequested {
        session_id: connection.halo_session_id.clone(),
        operation_id: operation_id.clone(),
        tool_name,
        redacted_arguments,
    });

    // Bounded operation window: an unanswered request is answered
    // "cancelled" on the wire and audited honestly as `Unavailable` — the
    // fail-closed default, never an inferred allow.
    let timed_connection = connection.clone();
    let timed_operation_id = operation_id.clone();
    let operation_timeout = connection.operation_timeout;
    tokio::spawn(async move {
        sleep(operation_timeout).await;
        let binding = timed_connection
            .permissions
            .lock()
            .await
            .remove(&timed_operation_id);
        if let Some(binding) = binding {
            let _ = timed_connection
                .send_result(
                    binding.rpc_id.clone(),
                    json!({ "outcome": { "outcome": "cancelled" } }),
                )
                .await;
            timed_connection.emit(DshEvent::PermissionResolved {
                session_id: timed_connection.halo_session_id.clone(),
                operation_id: timed_operation_id,
                outcome: None,
            });
        }
    });
}

async fn resolve_response(connection: &Arc<AcpConnection>, id: i64, message: &Value) {
    let sender = connection.pending.lock().await.remove(&id);
    if let Some(sender) = sender {
        if let Some(error) = message.get("error") {
            let _ = sender.send(Err(PortError::new(
                PortErrorKind::Backend,
                format!("DSH request failed: {error}"),
            )));
        } else {
            let _ = sender.send(Ok(
                message.get("result").cloned().unwrap_or(Value::Null)
            ));
        }
    }
    // Unmatched responses are filtered, never fatal.
}

async fn handle_notification(connection: &Arc<AcpConnection>, method: &str, message: &Value) {
    let Some(params) = message.get("params") else {
        return;
    };
    match method {
        METHOD_SESSION_UPDATE => project_session_update(connection, params),
        METHOD_SESSION_EVENT => project_session_event(connection, params),
        METHOD_SESSION_STATUS => {
            // SDK channel settlement: the ordered idle status after the
            // accepted prompt drains its updates.
            if params.get("status").and_then(Value::as_str) == Some("idle") {
                connection.idle_epoch.fetch_add(1, Ordering::AcqRel);
                connection.idle.notify_waiters();
            }
        }
        // Client-side filter: unknown notifications never fail the session.
        _ => {}
    }
}

/// Projects one committed ACP `session/update` (research section 3.2). Only
/// the fact-bearing vocabulary crosses: message chunks accumulate toward the
/// reply summary, tool calls project their lifecycle, everything else
/// (thought chunks, config/usage updates, future kinds) is filtered here.
fn project_session_update(connection: &Arc<AcpConnection>, params: &Value) {
    let update = params.get("update").unwrap_or(params);
    match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("agent_message_chunk") => {
            if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
                connection.emit(DshEvent::MessageChunk {
                    session_id: connection.halo_session_id.clone(),
                    text: text.to_string(),
                });
            }
        }
        Some("tool_call") => {
            let Some(native_tool_call_id) = update.get("toolCallId").and_then(Value::as_str)
            else {
                return;
            };
            let tool_name = bounded(
                update
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(native_tool_call_id),
                MAX_TOOL_NAME_BYTES,
            );
            connection.emit(DshEvent::ToolCallStarted {
                session_id: connection.halo_session_id.clone(),
                call_id: redact_tool_call_id(&connection.halo_session_id, native_tool_call_id),
                tool_name,
            });
        }
        Some("tool_call_update") => {
            let Some(native_tool_call_id) = update.get("toolCallId").and_then(Value::as_str)
            else {
                return;
            };
            let is_error = match update.get("status").and_then(Value::as_str) {
                Some("completed") => false,
                Some("failed") => true,
                // in_progress and future statuses carry no committed fact.
                _ => return,
            };
            connection.emit(DshEvent::ToolCallEnded {
                session_id: connection.halo_session_id.clone(),
                call_id: redact_tool_call_id(&connection.halo_session_id, native_tool_call_id),
                is_error,
            });
        }
        // agent_thought_chunk, config_option_update, usage_update and future
        // kinds are intentionally filtered: they are not fact vocabulary.
        _ => {}
    }
}

/// Projects the SDK canary channel's committed `session.event` stream
/// (research section 3.1: `assistant/message`, `tool/call`, `tool/result`
/// SessionEvents — the same committed events the ACP update stream projects
/// from, so the degraded channel keeps the fact chain intact).
fn project_session_event(connection: &Arc<AcpConnection>, params: &Value) {
    let event = params.get("event").unwrap_or(params);
    match event.get("type").and_then(Value::as_str) {
        Some("assistant/message") => {
            let blocks = event
                .get("content")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        connection.emit(DshEvent::MessageChunk {
                            session_id: connection.halo_session_id.clone(),
                            text: text.to_string(),
                        });
                    }
                }
            }
        }
        Some("tool/call") => {
            let Some(native_tool_call_id) = event.get("toolCallId").and_then(Value::as_str)
            else {
                return;
            };
            let tool_name = bounded(
                event
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(native_tool_call_id),
                MAX_TOOL_NAME_BYTES,
            );
            connection.emit(DshEvent::ToolCallStarted {
                session_id: connection.halo_session_id.clone(),
                call_id: redact_tool_call_id(&connection.halo_session_id, native_tool_call_id),
                tool_name,
            });
        }
        Some("tool/result") => {
            let Some(native_tool_call_id) = event.get("toolCallId").and_then(Value::as_str)
            else {
                return;
            };
            connection.emit(DshEvent::ToolCallEnded {
                session_id: connection.halo_session_id.clone(),
                call_id: redact_tool_call_id(&connection.halo_session_id, native_tool_call_id),
                is_error: event.get("isError").and_then(Value::as_bool) == Some(true),
            });
        }
        _ => {}
    }
}

async fn transport_ended(connection: &Arc<AcpConnection>) {
    // Fail every pending request; answered prompts are unaffected.
    let pending = std::mem::take(&mut *connection.pending.lock().await);
    for (_, sender) in pending {
        let _ = sender.send(Err(PortError::new(
            PortErrorKind::Backend,
            "DSH transport closed",
        )));
    }
    // Pending permission requests lose their answer channel: audit them
    // honestly as `Unavailable` instead of leaving the decision dangling.
    let permissions = std::mem::take(&mut *connection.permissions.lock().await);
    for (operation_id, _) in permissions {
        connection.emit(DshEvent::PermissionResolved {
            session_id: connection.halo_session_id.clone(),
            operation_id,
            outcome: None,
        });
    }
    connection.emit(DshEvent::TransportEnded);
}

/// Redacts a native ACP tool-call id into a stable adapter-owned digest. Raw
/// executor ids never cross the port (ADR-0078).
pub(crate) fn redact_tool_call_id(halo_session_id: &str, native_tool_call_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(halo_session_id.as_bytes());
    digest.update(&[0x01u8]);
    digest.update(native_tool_call_id.as_bytes());
    format!("dsh-call-{}", hex_digest(&digest.finalize())[..16].to_string())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn bounded(value: &str, max_bytes: usize) -> String {
    let take = value.len().min(max_bytes);
    let take = value.floor_char_boundary(take);
    value[..take].to_string()
}
