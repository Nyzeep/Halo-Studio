//! Pi RPC projection for the Halo Workbench Runtime.
//!
//! The adapter owns the local `pi --mode rpc` subprocess and keeps Pi's
//! session identifiers, conversation entries, tool arguments, credentials,
//! and command output inside this module. The Workbench Runtime receives only
//! Halo lifecycle facts and redacted permission correlation identifiers.

mod framing;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bitfun_runtime_ports::{
    PiRpcCommand, PiRpcEvent, PiRpcFailureKind, PiRpcOperationDecision, PiRpcPort, PiRpcReply,
    PiRpcWorkspace, PortError, PortErrorKind, PortResult, PI_RPC_ADAPTER_IDENTITY,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::framing::{decode_jsonl_record, encode_jsonl};

const EVENT_CAPACITY: usize = 128;
const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

const HALO_PERMISSION_EXTENSION_SOURCE: &str = include_str!("halo_permission_gate.ts");

pub const HALO_PI_EXTENSION_ID: &str = "halo-workbench-permission-gate";
pub const HALO_PI_EXTENSION_VERSION: &str = "1.0.0";
pub const HALO_PI_EXTENSION_PERMISSIONS: &str =
    "Pi tool_call interception and RPC extension_ui_request only";

#[derive(Debug, Clone)]
pub struct PiRpcConfig {
    pub executable: Option<PathBuf>,
    pub extension_path: Option<PathBuf>,
    pub response_timeout: Duration,
    pub operation_timeout: Duration,
}

impl Default for PiRpcConfig {
    fn default() -> Self {
        Self {
            executable: None,
            extension_path: None,
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }
}

#[derive(Clone)]
pub struct PiRpcAdapter {
    events: broadcast::Sender<PiRpcEvent>,
    state: Arc<Mutex<AdapterState>>,
    config: PiRpcConfig,
}

#[derive(Default)]
struct AdapterState {
    generation: Option<u64>,
    workspace: Option<PiRpcWorkspace>,
    executable: Option<PathBuf>,
    extension_path: Option<PathBuf>,
    owned_extension_dir: Option<PathBuf>,
    prepared_session: Option<Arc<PiSession>>,
    sessions: HashMap<String, Arc<PiSession>>,
}

struct InstalledExtension {
    path: PathBuf,
    owned_dir: Option<PathBuf>,
}

impl InstalledExtension {
    fn cleanup(self) -> Result<(), PiRpcFailureKind> {
        let Some(directory) = self.owned_dir else {
            return Ok(());
        };

        match std::fs::remove_dir_all(directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(PiRpcFailureKind::Internal),
        }
    }
}

struct PiSession {
    generation: u64,
    session_id: Mutex<String>,
    events: broadcast::Sender<PiRpcEvent>,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<String, oneshot::Sender<PortResult<Value>>>>,
    operations: Mutex<HashMap<String, PiOperationBinding>>,
    prompt_sent: AtomicBool,
    running: AtomicBool,
    terminated: AtomicBool,
    failure_reported: AtomicBool,
    response_timeout: Duration,
    operation_timeout: Duration,
}

#[derive(Debug, Clone)]
struct PiOperationBinding {
    ui_request_id: String,
}

#[derive(Debug, Deserialize)]
struct PermissionNotice {
    #[serde(rename = "toolCallId")]
    tool_call_id: String,
    #[serde(rename = "toolName")]
    tool_name: String,
}

impl Default for PiRpcAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PiRpcAdapter {
    pub const IDENTITY: &'static str = PI_RPC_ADAPTER_IDENTITY;

    pub fn new() -> Self {
        Self::with_config(PiRpcConfig::default())
    }

    pub fn with_config(config: PiRpcConfig) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            events,
            state: Arc::new(Mutex::new(AdapterState::default())),
            config,
        }
    }

    pub fn extension_source() -> &'static str {
        HALO_PERMISSION_EXTENSION_SOURCE
    }

    pub fn extension_source_digest() -> String {
        stable_digest(HALO_PERMISSION_EXTENSION_SOURCE)
    }

    fn emit(&self, event: PiRpcEvent) {
        let _ = self.events.send(event);
    }

    async fn probe_pi(&self) -> Result<PathBuf, PiRpcFailureKind> {
        let executable = self.resolve_executable().await?;
        let output = build_pi_command(&executable, &["--version".to_string()])
            .output()
            .await
            .map_err(|_| PiRpcFailureKind::NotInstalled)?;
        if !output.status.success() {
            return Err(PiRpcFailureKind::UnsupportedVersion);
        }
        if output.stdout.is_empty() && output.stderr.is_empty() {
            return Err(PiRpcFailureKind::Protocol);
        }
        Ok(executable)
    }

    async fn resolve_executable(&self) -> Result<PathBuf, PiRpcFailureKind> {
        if let Some(executable) = &self.config.executable {
            return Ok(executable.clone());
        }

        #[cfg(windows)]
        {
            let where_output = Command::new("where.exe").arg("pi").output().await;
            if let Ok(output) = where_output {
                if output.status.success() {
                    if let Some(path) = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(PathBuf::from)
                        .find(|path| {
                            path.extension().and_then(|extension| extension.to_str()) != Some("ps1")
                        })
                    {
                        return Ok(path);
                    }
                }
            }

            // PowerShell can resolve npm's .ps1 shim even when the child
            // environment cannot resolve it through where.exe. Prefer the
            // executable shim that can be invoked through cmd.exe.
            for candidate in ["pi.cmd", "pi.exe", "pi"] {
                let output = build_pi_command(Path::new(candidate), &["--version".to_string()])
                    .output()
                    .await;
                if output.as_ref().is_ok_and(|output| output.status.success()) {
                    return Ok(PathBuf::from(candidate));
                }
            }
            return Err(PiRpcFailureKind::NotInstalled);
        }

        #[cfg(not(windows))]
        {
            Ok(PathBuf::from("pi"))
        }
    }

    fn install_first_party_extension(&self) -> Result<InstalledExtension, PiRpcFailureKind> {
        if let Some(path) = &self.config.extension_path {
            let source =
                std::fs::read_to_string(path).map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
            if source != HALO_PERMISSION_EXTENSION_SOURCE {
                return Err(PiRpcFailureKind::CapabilityMismatch);
            }
            return Ok(InstalledExtension {
                path: path.clone(),
                owned_dir: None,
            });
        }

        let extension_dir = std::env::temp_dir()
            .join("halo-studio")
            .join("pi-extensions")
            .join(format!("{HALO_PI_EXTENSION_ID}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&extension_dir).map_err(|_| PiRpcFailureKind::Internal)?;
        let extension_path = extension_dir.join(format!(
            "{HALO_PI_EXTENSION_ID}-{}.ts",
            stable_digest(HALO_PERMISSION_EXTENSION_SOURCE)
        ));
        if std::fs::write(&extension_path, HALO_PERMISSION_EXTENSION_SOURCE).is_err() {
            let _ = std::fs::remove_dir_all(&extension_dir);
            return Err(PiRpcFailureKind::Internal);
        }
        Ok(InstalledExtension {
            path: extension_path,
            owned_dir: Some(extension_dir),
        })
    }

    async fn spawn_session_process(
        &self,
        generation: u64,
        session_id: String,
        workspace: &PiRpcWorkspace,
        executable: &Path,
        extension_path: &Path,
    ) -> Result<Arc<PiSession>, PiRpcFailureKind> {
        let mut child = build_pi_command(executable, &pi_rpc_args(extension_path))
            .current_dir(&workspace.canonical_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| PiRpcFailureKind::NotInstalled)?;
        let stdin = child.stdin.take().ok_or(PiRpcFailureKind::Transport)?;
        let stdout = child.stdout.take().ok_or(PiRpcFailureKind::Transport)?;
        let session = Arc::new(PiSession {
            generation,
            session_id: Mutex::new(session_id),
            events: self.events.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            operations: Mutex::new(HashMap::new()),
            prompt_sent: AtomicBool::new(false),
            running: AtomicBool::new(false),
            terminated: AtomicBool::new(false),
            failure_reported: AtomicBool::new(false),
            response_timeout: self.config.response_timeout,
            operation_timeout: self.config.operation_timeout,
        });
        tokio::spawn(read_pi_stdout(session.clone(), stdout));
        Ok(session)
    }

    async fn handshake(&self, session: &Arc<PiSession>) -> Result<(), PiRpcFailureKind> {
        let state = session
            .request("get_state", json!({ "type": "get_state" }))
            .await
            .map_err(map_handshake_error)?;
        if !state
            .as_object()
            .is_some_and(|state| state.get("isStreaming").and_then(Value::as_bool).is_some())
        {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
            return Err(PiRpcFailureKind::Protocol);
        }

        let entries = session
            .request("get_entries", json!({ "type": "get_entries" }))
            .await
            .map_err(map_handshake_error)?;
        if entries
            .as_object()
            .and_then(|entries| entries.get("entries"))
            .and_then(Value::as_array)
            .is_none()
        {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
            return Err(PiRpcFailureKind::Protocol);
        }
        Ok(())
    }

    async fn start(&self, generation: u64, workspace: PiRpcWorkspace) -> PiRpcReply {
        let extension = match self.install_first_party_extension() {
            Ok(extension) => extension,
            Err(reason) => return PiRpcReply::Unavailable { reason },
        };
        let extension_path = extension.path.clone();
        let executable = match self.probe_pi().await {
            Ok(path) => path,
            Err(reason) => {
                let reason = extension.cleanup().err().unwrap_or(reason);
                return PiRpcReply::Unavailable { reason };
            }
        };

        let prepared_session = match self
            .spawn_session_process(
                generation,
                "__halo_workbench_prepared__".to_string(),
                &workspace,
                &executable,
                &extension_path,
            )
            .await
        {
            Ok(session) => session,
            Err(reason) => {
                let reason = extension.cleanup().err().unwrap_or(reason);
                return PiRpcReply::Unavailable { reason };
            }
        };
        if let Err(reason) = self.handshake(&prepared_session).await {
            prepared_session.terminate().await;
            let reason = extension.cleanup().err().unwrap_or(reason);
            return PiRpcReply::Unavailable { reason };
        }

        let mut state = self.state.lock().await;
        state.generation = Some(generation);
        state.workspace = Some(workspace);
        state.executable = Some(executable);
        state.extension_path = Some(extension.path);
        state.owned_extension_dir = extension.owned_dir;
        state.prepared_session = Some(prepared_session);
        drop(state);

        self.emit(PiRpcEvent::Ready { generation });
        PiRpcReply::Accepted
    }

    async fn create_session(
        &self,
        generation: u64,
        session_id: String,
    ) -> Result<(), PiRpcFailureKind> {
        let (workspace, executable, extension_path, prepared_session) = {
            let mut state = self.state.lock().await;
            if state.generation != Some(generation) {
                return Err(PiRpcFailureKind::Transport);
            }
            if state.sessions.contains_key(&session_id) {
                return Err(PiRpcFailureKind::Internal);
            }
            (
                state.workspace.clone().ok_or(PiRpcFailureKind::Transport)?,
                state
                    .executable
                    .clone()
                    .ok_or(PiRpcFailureKind::Transport)?,
                state
                    .extension_path
                    .clone()
                    .ok_or(PiRpcFailureKind::CapabilityMismatch)?,
                state.prepared_session.take(),
            )
        };

        let session = match prepared_session {
            Some(session) => session,
            None => {
                let session = self
                    .spawn_session_process(
                        generation,
                        session_id.clone(),
                        &workspace,
                        &executable,
                        &extension_path,
                    )
                    .await?;
                if let Err(reason) = self.handshake(&session).await {
                    session.terminate().await;
                    return Err(reason);
                }
                session
            }
        };
        session.set_session_id(session_id.clone()).await;

        let mut state = self.state.lock().await;
        if state.generation != Some(generation) {
            drop(state);
            session.terminate().await;
            return Err(PiRpcFailureKind::Transport);
        }
        state.sessions.insert(session_id.clone(), session);
        drop(state);
        self.emit(PiRpcEvent::SessionCreated {
            generation,
            session_id: session_id.clone(),
        });
        self.emit(PiRpcEvent::SessionIdle {
            generation,
            session_id,
        });
        Ok(())
    }

    async fn session(&self, generation: u64, session_id: &str) -> PortResult<Arc<PiSession>> {
        let state = self.state.lock().await;
        if state.generation != Some(generation) {
            return Err(PortError::new(
                PortErrorKind::NotAvailable,
                "Pi RPC generation is no longer active",
            ));
        }
        state.sessions.get(session_id).cloned().ok_or_else(|| {
            PortError::new(PortErrorKind::NotFound, "Pi RPC session is not available")
        })
    }

    async fn shutdown_sessions(&self) -> Result<(), PiRpcFailureKind> {
        let (sessions, owned_extension_dir) = {
            let mut state = self.state.lock().await;
            state.generation = None;
            state.workspace = None;
            state.executable = None;
            state.extension_path = None;
            let owned_extension_dir = state.owned_extension_dir.take();
            let mut sessions = state
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>();
            if let Some(session) = state.prepared_session.take() {
                sessions.push(session);
            }
            (sessions, owned_extension_dir)
        };
        for session in sessions {
            session.terminate().await;
        }
        if let Some(directory) = owned_extension_dir {
            match std::fs::remove_dir_all(directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(PiRpcFailureKind::Internal),
            }
        }
        Ok(())
    }
}

#[async_trait]
impl PiRpcPort for PiRpcAdapter {
    async fn execute(&self, command: PiRpcCommand) -> PortResult<PiRpcReply> {
        match command {
            PiRpcCommand::Probe { .. } => match self.probe_pi().await {
                Ok(_) => Ok(PiRpcReply::Available),
                Err(reason) => Ok(PiRpcReply::Unavailable { reason }),
            },
            PiRpcCommand::Start {
                generation,
                workspace,
            } => Ok(self.start(generation, workspace).await),
            PiRpcCommand::CreateSession {
                generation,
                session_id,
                ..
            } => match self.create_session(generation, session_id).await {
                Ok(()) => Ok(PiRpcReply::Accepted),
                Err(reason) => Ok(PiRpcReply::Unavailable { reason }),
            },
            PiRpcCommand::SendUserInput {
                generation,
                session_id,
                content,
            } => {
                let session = self.session(generation, &session_id).await?;
                let command = next_input_command(&session.prompt_sent);
                session
                    .request(command, json!({ "type": command, "message": content }))
                    .await?;
                session.running.store(true, Ordering::Release);
                self.emit(PiRpcEvent::SessionRunning {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::StopSession {
                generation,
                session_id,
            } => {
                let session = self.session(generation, &session_id).await?;
                session.request("abort", json!({ "type": "abort" })).await?;
                session.running.store(false, Ordering::Release);
                self.emit(PiRpcEvent::SessionStopped {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::EndSession {
                generation,
                session_id,
            } => {
                let session = self.session(generation, &session_id).await?;
                if session.running.load(Ordering::Acquire) {
                    let _ = session.request("abort", json!({ "type": "abort" })).await;
                }
                session.terminate().await;
                self.state.lock().await.sessions.remove(&session_id);
                self.emit(PiRpcEvent::SessionEnded {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::ResolveOperation {
                generation,
                session_id,
                operation_id,
                decision,
            } => {
                let session = self.session(generation, &session_id).await?;
                let confirmed = match decision {
                    PiRpcOperationDecision::AllowOnce => true,
                    PiRpcOperationDecision::Deny => false,
                    PiRpcOperationDecision::Answer { .. } => {
                        return Err(PortError::new(
                            PortErrorKind::InvalidRequest,
                            "Pi RPC permission operations accept allowOnce or deny only",
                        ));
                    }
                };
                let binding = session
                    .operations
                    .lock()
                    .await
                    .remove(&operation_id)
                    .ok_or_else(|| {
                        PortError::new(
                            PortErrorKind::NotFound,
                            "Pi RPC permission operation is no longer pending",
                        )
                    })?;
                session
                    .send_extension_ui_response(&binding.ui_request_id, confirmed)
                    .await?;
                self.emit(PiRpcEvent::OperationResolved {
                    generation,
                    session_id,
                    operation_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::Shutdown { .. } => match self.shutdown_sessions().await {
                Ok(()) => Ok(PiRpcReply::Accepted),
                Err(reason) => Ok(PiRpcReply::Unavailable { reason }),
            },
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<PiRpcEvent> {
        self.events.subscribe()
    }
}

impl PiSession {
    async fn current_session_id(&self) -> String {
        self.session_id.lock().await.clone()
    }

    async fn set_session_id(&self, session_id: String) {
        *self.session_id.lock().await = session_id;
    }

    async fn request(&self, command: &str, payload: Value) -> PortResult<Value> {
        let mut payload = payload.as_object().cloned().ok_or_else(|| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC command must be an object",
            )
        })?;
        let request_id = Uuid::new_v4().to_string();
        payload.insert("id".to_string(), Value::String(request_id.clone()));
        let encoded = encode_jsonl(&Value::Object(payload)).map_err(|_| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC command could not be encoded",
            )
        })?;
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(request_id.clone(), sender);

        let write_result = async {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(&encoded).await?;
            stdin.flush().await
        }
        .await;
        if write_result.is_err() {
            self.pending.lock().await.remove(&request_id);
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC stdin is unavailable",
            ));
        }

        let response = match timeout(self.response_timeout, receiver).await {
            Ok(response) => response.map_err(|_| {
                PortError::new(PortErrorKind::Backend, "Pi RPC response stream closed")
            })??,
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                return Err(PortError::new(
                    PortErrorKind::Timeout,
                    "Pi RPC response timed out",
                ));
            }
        };
        if response.get("command").and_then(Value::as_str) != Some(command) {
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC response command did not match the request",
            ));
        }
        if response.get("success").and_then(Value::as_bool) != Some(true) {
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC command was rejected",
            ));
        }
        Ok(response.get("data").cloned().unwrap_or(Value::Null))
    }

    async fn send_extension_ui_response(
        &self,
        request_id: &str,
        confirmed: bool,
    ) -> PortResult<()> {
        let value = json!({
            "type": "extension_ui_response",
            "id": request_id,
            "confirmed": confirmed,
        });
        let encoded = encode_jsonl(&value).map_err(|_| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC extension response could not be encoded",
            )
        })?;
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(&encoded)
            .await
            .map_err(|_| PortError::new(PortErrorKind::Backend, "Pi RPC stdin is unavailable"))?;
        stdin
            .flush()
            .await
            .map_err(|_| PortError::new(PortErrorKind::Backend, "Pi RPC stdin is unavailable"))
    }

    async fn handle_extension_ui_request(self: &Arc<Self>, value: &Value) {
        let Some(request_id) = value.get("id").and_then(Value::as_str) else {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        };
        if value.get("method").and_then(Value::as_str) != Some("confirm") {
            let _ = self.send_extension_ui_response(request_id, false).await;
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        }
        let Some(message) = value.get("message").and_then(Value::as_str) else {
            let _ = self.send_extension_ui_response(request_id, false).await;
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        };
        let notice = match serde_json::from_str::<PermissionNotice>(message) {
            Ok(notice) if !notice.tool_call_id.is_empty() && !notice.tool_name.is_empty() => notice,
            _ => {
                let _ = self.send_extension_ui_response(request_id, false).await;
                self.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            }
        };

        let operation_id = format!("pi-operation-{}", Uuid::new_v4());
        self.operations.lock().await.insert(
            operation_id.clone(),
            PiOperationBinding {
                ui_request_id: request_id.to_string(),
            },
        );
        let session_id = self.current_session_id().await;
        let _ = self.events.send(PiRpcEvent::OperationRequested {
            generation: self.generation,
            session_id: session_id.clone(),
            operation_id: operation_id.clone(),
            kind: bitfun_runtime_ports::PiRpcOperationKind::Permission,
            redacted_tool_call_id: Some(redact_tool_call_id(&session_id, &notice.tool_call_id)),
        });

        let timeout_duration = bounded_operation_timeout(value, self.operation_timeout);
        let session = Arc::clone(self);
        tokio::spawn(async move {
            sleep(timeout_duration).await;
            let binding = session.operations.lock().await.remove(&operation_id);
            let Some(binding) = binding else { return };
            let response = session
                .send_extension_ui_response(&binding.ui_request_id, false)
                .await;
            if response.is_err() {
                session.fail_closed(PiRpcFailureKind::Transport).await;
                return;
            }
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::OperationResolved {
                generation: session.generation,
                session_id,
                operation_id,
            });
        });
    }

    async fn fail_protocol(&self, reason: PiRpcFailureKind) {
        if self.failure_reported.swap(true, Ordering::AcqRel) {
            return;
        }
        self.running.store(false, Ordering::Release);
        self.fail_pending("Pi RPC protocol error").await;
        self.operations.lock().await.clear();
        let session_id = self.current_session_id().await;
        let _ = self.events.send(PiRpcEvent::SessionFailed {
            generation: self.generation,
            session_id,
            reason,
        });
    }

    async fn fail_closed(self: &Arc<Self>, reason: PiRpcFailureKind) {
        self.fail_protocol(reason).await;
        self.terminate().await;
    }

    async fn fail_pending(&self, message: &str) {
        let pending = std::mem::take(&mut *self.pending.lock().await);
        for (_, sender) in pending {
            let _ = sender.send(Err(PortError::new(PortErrorKind::Backend, message)));
        }
    }

    async fn terminate(&self) {
        if self.terminated.swap(true, Ordering::AcqRel) {
            return;
        }
        self.fail_pending("Pi RPC session closed").await;
        self.operations.lock().await.clear();
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

async fn read_pi_stdout(session: Arc<PiSession>, stdout: ChildStdout) {
    let mut reader = BufReader::new(stdout);
    let mut record = Vec::new();
    loop {
        record.clear();
        let read = match reader.read_until(b'\n', &mut record).await {
            Ok(read) => read,
            Err(_) => {
                session.fail_closed(PiRpcFailureKind::Transport).await;
                break;
            }
        };
        if read == 0 {
            if !session.terminated.load(Ordering::Acquire) {
                session.fail_closed(PiRpcFailureKind::Transport).await;
            }
            break;
        }
        if record.last() != Some(&b'\n') || record.len() == 1 {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
            break;
        }
        record.pop();
        match decode_jsonl_record(&record) {
            Ok(value) => handle_pi_message(&session, &value).await,
            Err(_) => {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                break;
            }
        }
    }
}

async fn handle_pi_message(session: &Arc<PiSession>, value: &Value) {
    match value.get("type").and_then(Value::as_str) {
        Some("response") => {
            let Some(request_id) = value.get("id").and_then(Value::as_str) else {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            };
            if let Some(sender) = session.pending.lock().await.remove(request_id) {
                let _ = sender.send(Ok(value.clone()));
            } else {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
            }
        }
        Some("agent_start") => {
            session.running.store(true, Ordering::Release);
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::SessionRunning {
                generation: session.generation,
                session_id,
            });
        }
        Some("agent_settled") => {
            session.running.store(false, Ordering::Release);
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::AgentSettled {
                generation: session.generation,
                session_id,
            });
        }
        Some("message_update") => {
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::MessageUpdated {
                generation: session.generation,
                session_id,
            });
        }
        Some("tool_execution_start") => {
            emit_tool_event(session, value, ToolEventKind::Started).await;
        }
        Some("tool_execution_update") => {
            emit_tool_event(session, value, ToolEventKind::Updated).await;
        }
        Some("tool_execution_end") => {
            emit_tool_event(session, value, ToolEventKind::Ended).await;
        }
        Some("extension_ui_request") => session.handle_extension_ui_request(value).await,
        Some("extension_error") => {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
        }
        _ => {}
    }
}

enum ToolEventKind {
    Started,
    Updated,
    Ended,
}

async fn emit_tool_event(session: &Arc<PiSession>, value: &Value, kind: ToolEventKind) {
    let Some(tool_call_id) = value.get("toolCallId").and_then(Value::as_str) else {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    };
    let Some(tool_name) = value.get("toolName").and_then(Value::as_str) else {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    };
    let session_id = session.current_session_id().await;
    let redacted_tool_call_id = redact_tool_call_id(&session_id, tool_call_id);
    let event = match kind {
        ToolEventKind::Started => PiRpcEvent::ToolExecutionStarted {
            generation: session.generation,
            session_id: session_id.clone(),
            redacted_tool_call_id,
            tool_name: tool_name.to_string(),
        },
        ToolEventKind::Updated => PiRpcEvent::ToolExecutionUpdated {
            generation: session.generation,
            session_id: session_id.clone(),
            redacted_tool_call_id,
            tool_name: tool_name.to_string(),
        },
        ToolEventKind::Ended => PiRpcEvent::ToolExecutionEnded {
            generation: session.generation,
            session_id,
            redacted_tool_call_id,
            tool_name: tool_name.to_string(),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        },
    };
    let _ = session.events.send(event);
}

fn stable_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn next_input_command(prompt_sent: &AtomicBool) -> &'static str {
    if prompt_sent.swap(true, Ordering::AcqRel) {
        "follow_up"
    } else {
        "prompt"
    }
}

fn bounded_operation_timeout(value: &Value, configured: Duration) -> Duration {
    let requested = value
        .get("timeout")
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(configured);
    requested.min(configured)
}

fn redact_tool_call_id(session_id: &str, value: &str) -> String {
    format!("tool-{}", stable_digest(&format!("{session_id}:{value}")))
}

fn pi_rpc_args(extension_path: &Path) -> Vec<String> {
    vec![
        "--mode".to_string(),
        "rpc".to_string(),
        "--no-session".to_string(),
        "--no-extensions".to_string(),
        "--extension".to_string(),
        extension_path.to_string_lossy().into_owned(),
    ]
}

fn map_handshake_error(error: PortError) -> PiRpcFailureKind {
    match error.kind {
        PortErrorKind::Timeout
        | PortErrorKind::NotAvailable
        | PortErrorKind::NotFound
        | PortErrorKind::Cancelled
        | PortErrorKind::SessionInUse
        | PortErrorKind::CleanupRequired => PiRpcFailureKind::Transport,
        PortErrorKind::InvalidRequest | PortErrorKind::PermissionDenied => {
            PiRpcFailureKind::Protocol
        }
        PortErrorKind::Backend => PiRpcFailureKind::Protocol,
    }
}

fn build_pi_command(executable: &Path, args: &[String]) -> Command {
    #[cfg(windows)]
    if matches!(
        executable
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("cmd") | Some("bat")
    ) {
        let mut command = Command::new("cmd.exe");
        command.arg("/D").arg("/S").arg("/C");
        let mut command_line = quote_windows_arg(&executable.to_string_lossy());
        for argument in args {
            command_line.push(' ');
            command_line.push_str(&quote_windows_arg(argument));
        }
        command.arg(command_line);
        return command;
    }

    let mut command = Command::new(executable);
    command.args(args);
    command
}

#[cfg(windows)]
fn quote_windows_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use bitfun_runtime_ports::{
        PiRpcCommand, PiRpcFailureKind, PiRpcPort, PiRpcReply, PiRpcWorkspace,
        PI_RPC_ADAPTER_IDENTITY,
    };

    use super::{
        bounded_operation_timeout, next_input_command, redact_tool_call_id, PiRpcAdapter,
        PiRpcConfig,
    };

    fn workspace() -> PiRpcWorkspace {
        PiRpcWorkspace {
            workspace_id: "workspace-1".to_string(),
            canonical_root: PathBuf::from("C:/workspace"),
        }
    }

    #[test]
    fn identity_and_first_party_extension_are_stable() {
        assert_eq!(PiRpcAdapter::IDENTITY, PI_RPC_ADAPTER_IDENTITY);
        assert_eq!(PiRpcAdapter::extension_source_digest().len(), 64);
        assert!(PiRpcAdapter::extension_source().contains("pi.on(\"tool_call\""));
        assert!(PiRpcAdapter::extension_source().contains("block: true"));
        assert!(PiRpcAdapter::extension_source().contains("timeout: DECISION_TIMEOUT_MS"));
    }

    #[test]
    fn tool_call_ids_are_redacted_before_projection() {
        let redacted = redact_tool_call_id("session-1", "call-secret-value");
        assert!(redacted.starts_with("tool-"));
        assert!(!redacted.contains("secret"));
        assert_eq!(
            redacted,
            redact_tool_call_id("session-1", "call-secret-value")
        );
        assert_ne!(
            redacted,
            redact_tool_call_id("session-2", "call-secret-value")
        );
    }

    #[test]
    fn first_input_is_prompt_and_later_inputs_are_follow_ups_after_settling() {
        let prompt_sent = AtomicBool::new(false);

        assert_eq!(next_input_command(&prompt_sent), "prompt");
        // `agent_settled` clears `running`, but it must not make the next
        // input start a second Pi conversation.
        assert_eq!(next_input_command(&prompt_sent), "follow_up");
    }

    #[test]
    fn configured_operation_timeout_is_an_upper_bound() {
        let configured = Duration::from_secs(5);
        assert_eq!(
            bounded_operation_timeout(&serde_json::json!({}), configured),
            configured
        );
        assert_eq!(
            bounded_operation_timeout(&serde_json::json!({ "timeout": 10_000 }), configured),
            configured
        );
        assert_eq!(
            bounded_operation_timeout(&serde_json::json!({ "timeout": 1_000 }), configured),
            Duration::from_secs(1)
        );
    }

    #[tokio::test]
    async fn probe_fails_closed_when_pi_is_not_installed() {
        let adapter = PiRpcAdapter::with_config(PiRpcConfig {
            executable: Some(PathBuf::from("C:/does-not-exist/pi.exe")),
            ..PiRpcConfig::default()
        });

        assert_eq!(
            adapter
                .execute(PiRpcCommand::Probe {
                    generation: 1,
                    workspace: workspace(),
                })
                .await
                .unwrap(),
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::NotInstalled
            }
        );
    }

    #[tokio::test]
    async fn shutdown_is_idempotently_accepted() {
        let adapter = PiRpcAdapter::new();
        for generation in [1, 1, 2] {
            assert_eq!(
                adapter
                    .execute(PiRpcCommand::Shutdown { generation })
                    .await
                    .unwrap(),
                PiRpcReply::Accepted
            );
        }
    }
}
