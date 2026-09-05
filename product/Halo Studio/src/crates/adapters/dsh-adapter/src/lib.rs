//! The Halo Workbench Runtime DSH execution adapter (ADR-0078).
//!
//! This crate owns one controlled `dsh --profile <channel>` child per managed
//! session and translates the DeepSeek Harness wire into the narrow
//! `ManagedExecutorPort` seam. The ACP channel is the production path (one-shot
//! `session/request_permission` decisions, committed `session/update` facts);
//! the SDK channel exists only as the protocol canary / degraded channel and
//! keeps the same fact vocabulary. Native executor session ids, credentials,
//! raw tool-call ids and raw protocol records never leave this crate.
//!
//! Reclaim ladder (ADR-0078, pi-adapter semantics): wire cancel → bounded
//! grace → close stdin → cooperative exit window → force reclaim (Windows
//! Job-Object kill). Version profiles are anchored in
//! [`profile::SUPPORTED_DSH_PROFILES`] and validated at `initialize`,
//! fail-closed.

mod acp;
mod credentials;
mod framing;
mod managed_executor;
mod profile;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use halo_runtime_ports::{
    ManagedExecutorAbortOutcome, ManagedExecutorApprovalOutcome, ManagedExecutorPromptRequest,
    ManagedExecutorTarget, PortError, PortErrorKind, PortResult,
};
use halo_services_core::process_tree::ProcessTreeChild;
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::sync::{broadcast, Mutex, Notify};
use tokio::time::{sleep, timeout};

pub use credentials::{
    build_child_environment, DshCredentialRef, DshCredentialStore, MemoryDshCredentialStore,
    DSH_API_KEY_ENV, DSH_HOME_ENV,
};
pub use managed_executor::{managed_executor_failure_kind, DshManagedExecutor};
pub use profile::{
    supported_profile, DshChannelKind, DshProfile, SUPPORTED_DSH_PROFILES,
};

pub use crate::profile::validate_initialize_result;

/// Stable adapter identity surfaced through the capability profile.
pub const DSH_ADAPTER_IDENTITY: &str = "halo-dsh-adapter";

/// The reviewed compatibility profile anchor (ADR-0078).
pub const DSH_COMPATIBILITY_PROFILE: &str = "0.1.3-alpha.1";

const EVENT_CAPACITY: usize = 128;
const TERMINAL_OPEN: u8 = 0;
const TERMINAL_CANCELLING: u8 = 1;
const TERMINAL_FAILED: u8 = 2;

/// Executor-side failure classification kept inside the adapter seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DshFailureKind {
    NotInstalled,
    UnsupportedVersion,
    Protocol,
    Transport,
    Authentication,
    Internal,
}

impl DshFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::UnsupportedVersion => "unsupported_version",
            Self::Protocol => "protocol",
            Self::Transport => "transport",
            Self::Authentication => "authentication",
            Self::Internal => "internal",
        }
    }
}

fn port_error(kind: DshFailureKind, message: impl Into<String>) -> PortError {
    let port_kind = match kind {
        DshFailureKind::NotInstalled | DshFailureKind::UnsupportedVersion => {
            PortErrorKind::NotAvailable
        }
        DshFailureKind::Authentication => PortErrorKind::PermissionDenied,
        DshFailureKind::Protocol
        | DshFailureKind::Transport
        | DshFailureKind::Internal => PortErrorKind::Backend,
    };
    PortError::new(port_kind, message)
}

/// Adapter configuration. Credentials only ever cross as a
/// [`DshCredentialRef`] plus a [`DshCredentialStore`] read at the child
/// creation boundary; literal values never belong in this structure.
#[derive(Clone)]
pub struct DshConfig {
    /// Explicit executable used by tests or a reviewed deployment; defaults
    /// to `dsh` on PATH.
    pub executable: Option<PathBuf>,
    /// The managed wire: `Acp` (production) or `Sdk` (canary/degraded).
    pub channel: DshChannelKind,
    /// Declared DSH version; must be an anchored entry of
    /// [`SUPPORTED_DSH_PROFILES`] or every session fails closed.
    pub declared_version: String,
    /// Absolute workspace the controlled child runs in (session cwd).
    pub workspace: Option<PathBuf>,
    /// Halo-managed `DSH_HOME`. When absent, the adapter owns a temporary
    /// directory that is removed with the session.
    pub dsh_home: Option<PathBuf>,
    /// Test/deployment root for adapter-owned temporary directories.
    pub temporary_root: Option<PathBuf>,
    /// CredentialRef (environment-variable name) resolved at spawn time.
    pub credential_ref: Option<DshCredentialRef>,
    /// Credential value source; read only at the controlled child boundary.
    pub credential_store: Option<Arc<dyn DshCredentialStore>>,
    /// Deployment-controlled launcher environment facts merged into the
    /// controlled child environment. Credentials never belong here — they
    /// have their dedicated CredentialRef/store path.
    pub extra_environment: HashMap<String, String>,
    pub response_timeout: Duration,
    pub operation_timeout: Duration,
    pub abort_grace_period: Duration,
}

impl Default for DshConfig {
    fn default() -> Self {
        Self {
            executable: None,
            channel: DshChannelKind::Acp,
            declared_version: DSH_COMPATIBILITY_PROFILE.to_string(),
            workspace: None,
            dsh_home: None,
            temporary_root: None,
            credential_ref: None,
            credential_store: None,
            extra_environment: HashMap::new(),
            response_timeout: Duration::from_secs(30),
            operation_timeout: Duration::from_secs(30),
            abort_grace_period: Duration::from_secs(3),
        }
    }
}

/// Internal adapter event vocabulary. Everything here stays in this crate and
/// is normalized into the unified fact vocabulary by the managed executor.
#[derive(Debug, Clone)]
pub(crate) enum DshEvent {
    MessageChunk {
        session_id: String,
        text: String,
    },
    ToolCallStarted {
        session_id: String,
        call_id: String,
        tool_name: String,
    },
    ToolCallEnded {
        session_id: String,
        call_id: String,
        is_error: bool,
    },
    PermissionRequested {
        session_id: String,
        operation_id: String,
        tool_name: String,
        redacted_arguments: String,
    },
    PermissionResolved {
        session_id: String,
        operation_id: String,
        /// `None` means no obtainable decision: audited as `Unavailable`.
        outcome: Option<ManagedExecutorApprovalOutcome>,
    },
    PromptSettled {
        session_id: String,
        /// ACP stop reason: `end_turn`/`max_tokens`/`max_turn_requests`/
        /// `refusal`/`cancelled`.
        stop_reason: &'static str,
    },
    /// The abort path closed the owned transport; interrupted turns project
    /// from here when the wire never confirmed the cancellation.
    TurnAborted {
        session_id: String,
    },
    TransportEnded,

    SessionFailed {
        session_id: String,
        reason: DshFailureKind,
    },
}

#[derive(Default)]
struct AdapterState {
    sessions: HashMap<String, Arc<DshSession>>,
    settled_sessions: HashSet<String>,
}

/// The DSH execution adapter: controlled child lifecycle, handshake and the
/// wire command paths behind [`crate::DshManagedExecutor`].
pub struct DshAdapter {
    events: broadcast::Sender<DshEvent>,
    state: Arc<Mutex<AdapterState>>,
    lifecycle: Arc<Mutex<()>>,
    config: DshConfig,
}

impl DshAdapter {
    pub fn with_config(config: DshConfig) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            events,
            state: Arc::new(Mutex::new(AdapterState::default())),
            lifecycle: Arc::new(Mutex::new(())),
            config,
        }
    }

    pub(crate) fn channel(&self) -> DshChannelKind {
        self.config.channel
    }

    pub(crate) fn declared_version(&self) -> &str {
        &self.config.declared_version
    }

    /// Internal event feed for the managed-executor forwarder.
    pub(crate) fn subscribe_internal(&self) -> broadcast::Receiver<DshEvent> {
        self.events.subscribe()
    }

    fn emit(&self, event: DshEvent) {
        let _ = self.events.send(event);
    }

    /// Starts a managed turn. The first prompt for a target spawns and
    /// handshakes the controlled child; ACP settles with the `session/prompt`
    /// response (quiescence-before-settlement upstream), SDK with the ordered
    /// idle status.
    pub(crate) async fn prompt_turn(
        &self,
        request: &ManagedExecutorPromptRequest,
        follow_up: bool,
    ) -> PortResult<()> {
        let session = self.ensure_session(&request.target).await?;
        session.bind_task(&request.target.task_id).await?;
        if session
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "a DSH turn is already running for this session",
            ));
        }
        if follow_up && !self.has_settled_session(&request.target.session_id).await {
            session.running.store(false, Ordering::Release);
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "DSH follow-up requires a settled prompt",
            ));
        }

        let connection = &session.connection;
        let params = match self.config.channel {
            DshChannelKind::Acp => {
                let Some(native) = connection.native_session().await else {
                    session.running.store(false, Ordering::Release);
                    return Err(port_error(
                        DshFailureKind::Protocol,
                        "DSH native session is missing",
                    ));
                };
                json!({
                    "sessionId": native,
                    "prompt": [ { "type": "text", "text": &request.content } ],
                })
            }
            DshChannelKind::Sdk => json!({
                "sessionId": request.target.session_id,
                "contentBlocks": [ { "type": "text", "text": &request.content } ],
            }),
        };

        let rpc_id = connection.allocate_request_id();
        connection.set_prompt_rpc_id(Some(rpc_id)).await;
        let result = connection
            .request_with_id(rpc_id, "session/prompt", params, self.config.response_timeout)
            .await;

        match result {
            Ok(value) => match self.config.channel {
                DshChannelKind::Acp => {
                    // stopReason mapping per the ACP codec (research section
                    // 3.2): upstream `interrupted` arrives as `cancelled`.
                    let stop_reason: &'static str =
                        match value.get("stopReason").and_then(Value::as_str) {
                            Some("end_turn") => "end_turn",
                            Some("max_tokens") => "max_tokens",
                            Some("max_turn_requests") => "max_turn_requests",
                            Some("refusal") => "refusal",
                            Some("cancelled") => "cancelled",
                            _ => {
                                self.settle_turn(&session, &request.target.session_id).await;
                                session.fail_closed(DshFailureKind::Protocol).await;
                                return Err(port_error(
                                    DshFailureKind::Protocol,
                                    "DSH stop reason failed validation",
                                ));
                            }
                        };
                    self.settle_turn(&session, &request.target.session_id).await;
                    self.emit(DshEvent::PromptSettled {
                        session_id: request.target.session_id.clone(),
                        stop_reason,
                    });
                    Ok(())
                }
                DshChannelKind::Sdk => match connection.wait_idle(self.config.response_timeout).await
                {
                    Ok(()) => {
                        self.settle_turn(&session, &request.target.session_id).await;
                        self.emit(DshEvent::PromptSettled {
                            session_id: request.target.session_id.clone(),
                            stop_reason: "end_turn",
                        });
                        Ok(())
                    }
                    Err(error) => {
                        self.settle_turn(&session, &request.target.session_id).await;
                        session.fail_closed(DshFailureKind::Transport).await;
                        Err(error)
                    }
                },
            },
            Err(error) => {
                self.settle_turn(&session, &request.target.session_id).await;
                if session.is_closed_by_abort().await {
                    // The abort path owns the outcome of a turn it reclaimed.
                    return Err(PortError::new(
                        PortErrorKind::Cancelled,
                        "DSH turn was cancelled",
                    ));
                }
                session.fail_closed(DshFailureKind::Transport).await;
                Err(error)
            }
        }
    }

    async fn settle_turn(&self, session: &Arc<DshSession>, session_id: &str) {
        session.running.store(false, Ordering::Release);
        session.settled_epoch.fetch_add(1, Ordering::AcqRel);
        session.settled.notify_waiters();
        session.connection.set_prompt_rpc_id(None).await;
        self.state
            .lock()
            .await
            .settled_sessions
            .insert(session_id.to_string());
    }

    async fn has_settled_session(&self, session_id: &str) -> bool {
        self.state
            .lock()
            .await
            .settled_sessions
            .contains(session_id)
    }

    /// Aborts the running turn and reclaims the executor session: wire cancel
    /// (`session/cancel` + `$/cancelRequest`) → bounded grace → close stdin →
    /// cooperative exit window → force reclaim.
    pub(crate) async fn abort_turn(
        &self,
        target: &ManagedExecutorTarget,
    ) -> PortResult<ManagedExecutorAbortOutcome> {
        let session = self.get_session(target).await?;
        session.claim_cancellation()?;
        let mut cooperative = true;
        if session.running.load(Ordering::Acquire) {
            let observed = session.settled_epoch.load(Ordering::Acquire);
            let connection = &session.connection;
            if self.config.channel == DshChannelKind::Acp {
                // Settles the in-flight prompt with stopReason `cancelled`;
                // unknown sessions are a no-op upstream.
                if let Some(native) = connection.native_session().await {
                    let _ = connection
                        .notify("session/cancel", json!({ "sessionId": native }))
                        .await;
                }
            }
            if let Some(rpc_id) = connection.prompt_rpc_id().await {
                let _ = connection
                    .notify("$/cancelRequest", json!({ "requestId": rpc_id }))
                    .await;
            }
            let settled = timeout(self.config.abort_grace_period, async {
                loop {
                    let notified = session.settled.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();
                    if !session.running.load(Ordering::Acquire)
                        || session.settled_epoch.load(Ordering::Acquire) != observed
                    {
                        break;
                    }
                    notified.await;
                }
            })
            .await
            .is_ok();
            cooperative = settled && !session.running.load(Ordering::Acquire);
        }
        session.terminate().await;
        self.state
            .lock()
            .await
            .sessions
            .remove(&target.session_id);
        self.emit(DshEvent::TurnAborted {
            session_id: target.session_id.clone(),
        });
        Ok(if cooperative {
            ManagedExecutorAbortOutcome::Cooperative
        } else {
            ManagedExecutorAbortOutcome::Reclaimed
        })
    }

    /// Forwards one one-shot approval decision to the pending ACP
    /// `session/request_permission` request. Only outcomes the wire can
    /// express are forwarded; everything else fails closed untouched.
    pub(crate) async fn send_approval_decision(
        &self,
        target: &ManagedExecutorTarget,
        operation_id: &str,
        outcome: ManagedExecutorApprovalOutcome,
    ) -> PortResult<()> {
        debug_assert!(
            !matches!(outcome, ManagedExecutorApprovalOutcome::Unavailable),
            "unavailable decisions are blocked at the executor seam"
        );
        let session = self.get_session(target).await?;
        let connection = &session.connection;
        let Some(binding) = connection.take_permission(operation_id).await else {
            // A stale, cross-task, or duplicated decision must never be a
            // harmless lookup miss: the operation id is the capability that
            // authorizes one answer, so this session fails closed.
            session.fail_closed(DshFailureKind::Protocol).await;
            return Err(PortError::new(
                PortErrorKind::NotFound,
                "DSH permission operation is no longer pending",
            ));
        };
        let answer = match outcome {
            ManagedExecutorApprovalOutcome::AllowedOnce => {
                let Some(allow) = binding.allow_option_id.as_deref() else {
                    session.fail_closed(DshFailureKind::Protocol).await;
                    return Err(port_error(
                        DshFailureKind::Protocol,
                        "DSH permission request carries no allow-once option",
                    ));
                };
                json!({ "outcome": { "outcome": "selected", "optionId": allow } })
            }
            ManagedExecutorApprovalOutcome::Rejected => {
                let Some(reject) = binding.reject_option_id.as_deref() else {
                    session.fail_closed(DshFailureKind::Protocol).await;
                    return Err(port_error(
                        DshFailureKind::Protocol,
                        "DSH permission request carries no reject-once option",
                    ));
                };
                json!({ "outcome": { "outcome": "selected", "optionId": reject } })
            }
            // A cancelled decision is expressible on the wire; it is recorded
            // as `cancelled`, never as an allow.
            ManagedExecutorApprovalOutcome::Cancelled => {
                json!({ "outcome": { "outcome": "cancelled" } })
            }
            ManagedExecutorApprovalOutcome::Unavailable => {
                return Err(PortError::new(
                    PortErrorKind::InvalidRequest,
                    "dsh cannot express this approval outcome; the decision was not forwarded",
                ));
            }
        };
        if connection.send_result(binding.rpc_id.clone(), answer).await.is_err() {
            session.fail_closed(DshFailureKind::Transport).await;
            return Err(PortError::new(
                PortErrorKind::Backend,
                "DSH permission response could not be sent",
            ));
        }
        self.emit(DshEvent::PermissionResolved {
            session_id: target.session_id.clone(),
            operation_id: operation_id.to_string(),
            outcome: Some(outcome),
        });
        Ok(())
    }

    /// Reclaims every live child through the standard ladder.
    pub async fn shutdown(&self) {
        let sessions: Vec<Arc<DshSession>> = {
            let mut state = self.state.lock().await;
            state.settled_sessions.clear();
            state
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect()
        };
        for session in sessions {
            session.terminate().await;
        }
    }

    async fn get_session(&self, target: &ManagedExecutorTarget) -> PortResult<Arc<DshSession>> {
        let session = self
            .state
            .lock()
            .await
            .sessions
            .get(&target.session_id)
            .cloned()
            .ok_or_else(|| {
                PortError::new(
                    PortErrorKind::NotFound,
                    "no managed DSH session for this target",
                )
            })?;
        if session.terminated.load(Ordering::Acquire)
            || session.terminal.load(Ordering::Acquire) == TERMINAL_FAILED
        {
            return Err(PortError::new(
                PortErrorKind::NotAvailable,
                "DSH session has failed closed",
            ));
        }
        session.bind_task(&target.task_id).await?;
        Ok(session)
    }

    async fn ensure_session(
        &self,
        target: &ManagedExecutorTarget,
    ) -> PortResult<Arc<DshSession>> {
        if let Some(session) = self.try_existing_session(target).await {
            return Ok(session);
        }

        let _lifecycle = self.lifecycle.lock().await;
        if let Some(session) = self.try_existing_session(target).await {
            return Ok(session);
        }

        // Fail-closed profile gate: nothing is spawned for an unanchored
        // version/channel combination.
        let profile = supported_profile(self.config.channel, &self.config.declared_version)
            .map_err(|kind| {
                port_error(
                    kind,
                    "DSH compatibility profile is not supported; upgrade requires a reviewed profile",
                )
            })?;
        let workspace = self
            .config
            .workspace
            .as_ref()
            .filter(|workspace| workspace.is_absolute())
            .ok_or_else(|| {
                PortError::new(
                    PortErrorKind::InvalidRequest,
                    "DSH adapter requires an absolute workspace path",
                )
            })?
            .clone();
        if !workspace.exists() {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "DSH workspace path does not exist",
            ));
        }

        let (dsh_home, owned_home) = match self.config.dsh_home.as_ref() {
            Some(home) => {
                std::fs::create_dir_all(home).map_err(|_| {
                    port_error(DshFailureKind::Internal, "DSH home could not be created")
                })?;
                (home.clone(), None)
            }
            None => {
                let mut builder = tempfile::Builder::new();
                builder.prefix(credentials::DSH_MANAGED_HOME_PREFIX);
                let home = match self.config.temporary_root.as_ref() {
                    Some(root) => {
                        std::fs::create_dir_all(root).map_err(|_| {
                            port_error(
                                DshFailureKind::Internal,
                                "DSH adapter temporary root is unavailable",
                            )
                        })?;
                        builder.tempdir_in(root)
                    }
                    None => builder.tempdir(),
                }
                .map_err(|_| {
                    port_error(DshFailureKind::Internal, "DSH home could not be created")
                })?;
                (home.path().to_path_buf(), Some(home))
            }
        };

        let mut credentials: Vec<(DshCredentialRef, String)> = Vec::new();
        if let Some(reference) = self.config.credential_ref.as_ref() {
            let store = self.config.credential_store.as_ref().ok_or_else(|| {
                PortError::new(
                    PortErrorKind::PermissionDenied,
                    "DSH credential store is not configured",
                )
            })?;
            let value = store.resolve(reference).await.map_err(|_| {
                PortError::new(
                    PortErrorKind::PermissionDenied,
                    "DSH credential could not be resolved",
                )
            })?;
            if value.is_empty() {
                return Err(PortError::new(
                    PortErrorKind::PermissionDenied,
                    "DSH credential value is empty",
                ));
            }
            credentials.push((reference.clone(), value));
        }

        let executable = self
            .config
            .executable
            .clone()
            .unwrap_or_else(|| PathBuf::from("dsh"));
        let mut command = Command::new(&executable);
        command
            .args(["--profile", self.config.channel.profile_arg()])
            // Full environment replacement: the child policy is exactly the
            // allowlist, the managed home, injected credentials and the
            // reviewed extra environment.
            .env_clear()
            .envs(build_child_environment(&dsh_home, &credentials))
            .envs(self.config.extra_environment.clone())
            .current_dir(&workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // DSH protocol output is stdout; diagnostics must never leak
            // into the protocol channel (same contract as the pi adapter).
            .stderr(Stdio::null());
        let mut child = ProcessTreeChild::spawn(&mut command).await.map_err(|_| {
            PortError::new(
                PortErrorKind::NotAvailable,
                "DSH executable could not be started",
            )
        })?;
        let stdin = child.take_stdin().ok_or_else(|| {
            port_error(DshFailureKind::Transport, "DSH child stdin is unavailable")
        })?;
        let stdout = child.take_stdout().ok_or_else(|| {
            port_error(DshFailureKind::Transport, "DSH child stdout is unavailable")
        })?;
        let connection = acp::AcpConnection::spawn(
            target.session_id.clone(),
            stdin,
            stdout,
            self.events.clone(),
            self.config.operation_timeout,
        );
        let session = Arc::new(DshSession {
            halo_session_id: target.session_id.clone(),
            task_id: Mutex::new(None),
            connection: connection.clone(),
            child: Mutex::new(child),
            running: AtomicBool::new(false),
            terminal: AtomicU8::new(TERMINAL_OPEN),
            terminated: AtomicBool::new(false),
            settled_epoch: AtomicU64::new(0),
            settled: Notify::new(),
            events: self.events.clone(),
            _dsh_home: owned_home,
            abort_grace_period: self.config.abort_grace_period,
            adapter_state: Arc::downgrade(&self.state),
        });

        if let Err(kind) = self
            .perform_handshake(&session, profile, &workspace)
            .await
        {
            session.fail_closed(kind).await;
            return Err(port_error(kind, "DSH handshake failed closed"));
        }

        self.state
            .lock()
            .await
            .sessions
            .insert(target.session_id.clone(), session.clone());
        Ok(session)
    }

    async fn try_existing_session(&self, target: &ManagedExecutorTarget) -> Option<Arc<DshSession>> {
        let session = self
            .state
            .lock()
            .await
            .sessions
            .get(&target.session_id)
            .cloned()?;
        if session.terminated.load(Ordering::Acquire)
            || session.terminal.load(Ordering::Acquire) == TERMINAL_FAILED
        {
            return None;
        }
        session.bind_task(&target.task_id).await.ok()?;
        Some(session)
    }

    /// `initialize` readiness/capability probe plus session establishment.
    /// The upstream ACP answer only arrives after its Loader tree has settled,
    /// so a successful handshake is a genuine readiness gate; the answer is
    /// validated against the anchored profile, fail-closed.
    async fn perform_handshake(
        &self,
        session: &Arc<DshSession>,
        profile: &DshProfile,
        workspace: &Path,
    ) -> Result<(), DshFailureKind> {
        let connection = &session.connection;
        let response_timeout = self.config.response_timeout;
        let initialized = match self.config.channel {
            DshChannelKind::Acp => {
                connection
                    .request(
                        "initialize",
                        json!({
                            "protocolVersion": profile.protocol_version,
                            "clientCapabilities": {},
                        }),
                        response_timeout,
                    )
                    .await
            }
            DshChannelKind::Sdk => {
                connection
                    .request(
                        "initialize",
                        json!({
                            "cwd": workspace.to_string_lossy(),
                            "provider": "deepseek-official",
                        }),
                        response_timeout,
                    )
                    .await
            }
        };
        let initialized = match initialized {
            Ok(value) => value,
            Err(error) => return Err(handshake_failure(session, &error).await),
        };
        validate_initialize_result(&initialized, profile)?;
        if self.config.channel == DshChannelKind::Acp {
            let created = connection
                .request(
                    "session/new",
                    json!({
                        "cwd": workspace.to_string_lossy(),
                        "mcpServers": [],
                    }),
                    response_timeout,
                )
                .await;
            let created = match created {
                Ok(value) => value,
                Err(error) => return Err(handshake_failure(session, &error).await),
            };
            let native = created
                .get("sessionId")
                .and_then(Value::as_str)
                .filter(|session_id| !session_id.is_empty())
                .ok_or(DshFailureKind::Protocol)?;
            connection.set_native_session(native.to_string()).await;
        }
        let child_exited = session
            .child
            .lock()
            .await
            .try_wait()
            .map(|status| status.is_some())
            .unwrap_or(true);
        if child_exited {
            return Err(DshFailureKind::Transport);
        }
        Ok(())
    }
}

async fn handshake_failure(session: &Arc<DshSession>, _error: &PortError) -> DshFailureKind {
    let child_exited = session
        .child
        .lock()
        .await
        .try_wait()
        .map(|status| status.is_some())
        .unwrap_or(true);
    if child_exited {
        DshFailureKind::Transport
    } else {
        DshFailureKind::Protocol
    }
}

struct DshSession {
    halo_session_id: String,
    task_id: Mutex<Option<String>>,
    connection: Arc<acp::AcpConnection>,
    child: Mutex<ProcessTreeChild>,
    running: AtomicBool,
    terminal: AtomicU8,
    terminated: AtomicBool,
    settled_epoch: AtomicU64,
    settled: Notify,
    events: broadcast::Sender<DshEvent>,
    _dsh_home: Option<tempfile::TempDir>,
    abort_grace_period: Duration,
    adapter_state: Weak<Mutex<AdapterState>>,
}

impl DshSession {
    async fn bind_task(&self, task_id: &str) -> PortResult<()> {
        let mut guard = self.task_id.lock().await;
        match guard.as_deref() {
            None => {
                *guard = Some(task_id.to_string());
                Ok(())
            }
            Some(bound) if bound == task_id => Ok(()),
            Some(_) => Err(PortError::new(
                PortErrorKind::PermissionDenied,
                "DSH session task scope did not match",
            )),
        }
    }

    fn claim_cancellation(&self) -> PortResult<()> {
        match self.terminal.compare_exchange(
            TERMINAL_OPEN,
            TERMINAL_CANCELLING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(TERMINAL_FAILED) => Err(PortError::new(
                PortErrorKind::Backend,
                "DSH session has already failed",
            )),
            Err(_) => Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "DSH session cancellation is already in progress",
            )),
        }
    }

    async fn is_closed_by_abort(&self) -> bool {
        self.terminal.load(Ordering::Acquire) == TERMINAL_CANCELLING
            || self.terminated.load(Ordering::Acquire)
    }

    async fn fail_closed(&self, reason: DshFailureKind) {
        if self
            .terminal
            .compare_exchange(
                TERMINAL_OPEN,
                TERMINAL_FAILED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return; // cancelling or already failed: the abort path owns it
        }
        self.running.store(false, Ordering::Release);
        self.settled.notify_waiters();
        if let Some(adapter_state) = self.adapter_state.upgrade() {
            adapter_state
                .lock()
                .await
                .sessions
                .remove(&self.halo_session_id);
        }
        let _ = self.events.send(DshEvent::SessionFailed {
            session_id: self.halo_session_id.clone(),
            reason,
        });
        self.terminate().await;
    }

    /// The reclaim ladder: close stdin → bounded cooperative exit window →
    /// force reclaim (the Windows Job Object terminates the whole tree).
    async fn terminate(&self) {
        if self.terminated.swap(true, Ordering::AcqRel) {
            return;
        }
        self.running.store(false, Ordering::Release);
        self.settled.notify_waiters();
        self.connection.close_stdin().await;
        let deadline = Instant::now() + self.abort_grace_period;
        loop {
            let exited = self
                .child
                .lock()
                .await
                .try_wait()
                .map(|status| status.is_some())
                .unwrap_or(true);
            if exited || Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        let mut child = self.child.lock().await;
        let exited = child
            .try_wait()
            .map(|status| status.is_some())
            .unwrap_or(false);
        if !exited {
            let _ = child.terminate(self.abort_grace_period).await;
        }
    }
}
