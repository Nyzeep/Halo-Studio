//! Portable owner for the Halo Workbench Runtime public seam.
//!
//! The owner exposes Halo-local state and intent types. Pi RPC protocol and
//! process details remain behind [`PiRpcPort`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use bitfun_runtime_ports::{
    ClockPort, PiProviderReadinessPort, PiRpcAvailabilitySummary, PiRpcCapability, PiRpcCommand,
    PiRpcEvent, PiRpcFailureKind, PiRpcOperationDecision, PiRpcOperationKind, PiRpcPort,
    PiRpcReply, PiRpcSessionMode, PiRpcVersionEvidenceSource, PiRpcVersionSummary, PiRpcWorkspace,
    PortErrorKind, WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest,
    PI_RPC_ADAPTER_IDENTITY,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, watch, OnceCell};
use uuid::Uuid;

pub const HALO_WORKBENCH_SCHEMA_VERSION: u32 = 1;

const MAX_COMPLETED_REQUEST_RECORDS: usize = 256;
const MAX_COMPLETED_CLEANUP_RECORDS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchPhase {
    Disconnected,
    Probing,
    Starting,
    Ready,
    Failed,
    Stopping,
}

/// Halo-local capability names projected from the Pi P0 contract.
///
/// Contract owner: `HaloWorkbenchRuntime`; consumers are its Tauri/Web
/// readiness snapshot and contract tests. Versioning follows
/// `HALO_WORKBENCH_SCHEMA_VERSION`. Verification is covered by the runtime
/// contract and Web serialization tests. Delete this enum when the public
/// Workbench readiness projection has no current consumer and is replaced by
/// a versioned owner contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchCapability {
    UserInput,
    FollowUpInput,
    SessionAbort,
    SessionState,
    SessionEntries,
    SessionEntryCollection,
    SessionEntryCursor,
    SessionEntryIncremental,
    AssistantMessageStream,
    ToolExecutionStart,
    ToolExecutionUpdate,
    ToolExecutionEnd,
    AgentSettled,
    PermissionUiRequest,
    PermissionUiResponse,
}

impl HaloWorkbenchCapability {
    pub const fn required_p0() -> &'static [Self] {
        &[
            Self::UserInput,
            Self::FollowUpInput,
            Self::SessionAbort,
            Self::SessionState,
            Self::SessionEntries,
            Self::SessionEntryCollection,
            Self::SessionEntryCursor,
            Self::SessionEntryIncremental,
            Self::AssistantMessageStream,
            Self::ToolExecutionStart,
            Self::ToolExecutionUpdate,
            Self::ToolExecutionEnd,
            Self::AgentSettled,
            Self::PermissionUiRequest,
            Self::PermissionUiResponse,
        ]
    }

    pub const fn verified_by_readiness_handshake() -> &'static [Self] {
        &[
            Self::SessionAbort,
            Self::SessionState,
            Self::SessionEntries,
            Self::SessionEntryCollection,
            Self::SessionEntryCursor,
            Self::SessionEntryIncremental,
        ]
    }
}

impl From<PiRpcCapability> for HaloWorkbenchCapability {
    fn from(capability: PiRpcCapability) -> Self {
        match capability {
            PiRpcCapability::Prompt => Self::UserInput,
            PiRpcCapability::FollowUp => Self::FollowUpInput,
            PiRpcCapability::Abort => Self::SessionAbort,
            PiRpcCapability::GetState => Self::SessionState,
            PiRpcCapability::GetEntries => Self::SessionEntries,
            PiRpcCapability::GetEntriesEntries => Self::SessionEntryCollection,
            PiRpcCapability::GetEntriesLeafId => Self::SessionEntryCursor,
            PiRpcCapability::GetEntriesSince => Self::SessionEntryIncremental,
            PiRpcCapability::MessageUpdate => Self::AssistantMessageStream,
            PiRpcCapability::ToolExecutionStart => Self::ToolExecutionStart,
            PiRpcCapability::ToolExecutionUpdate => Self::ToolExecutionUpdate,
            PiRpcCapability::ToolExecutionEnd => Self::ToolExecutionEnd,
            PiRpcCapability::AgentSettled => Self::AgentSettled,
            PiRpcCapability::ExtensionUiRequest => Self::PermissionUiRequest,
            PiRpcCapability::ExtensionUiResponse => Self::PermissionUiResponse,
        }
    }
}

/// Required-versus-verified Halo-local capability facts.
///
/// Contract owner: `HaloWorkbenchRuntime`; consumers are the Tauri/Web
/// readiness view and its contract tests. Versioning follows
/// `HALO_WORKBENCH_SCHEMA_VERSION`. Verification is covered by Workbench
/// Runtime and Web store/selector tests. Delete this DTO when no current
/// public readiness consumer remains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchCapabilitySummary {
    pub required: Vec<HaloWorkbenchCapability>,
    pub verified: Vec<HaloWorkbenchCapability>,
}

/// Halo-local adapter version and readiness projection.
///
/// Contract owner: `HaloWorkbenchRuntime`; consumers are the Tauri/Web
/// snapshot, startup state, and readiness contract tests. Versioning follows
/// `HALO_WORKBENCH_SCHEMA_VERSION`; Pi protocol versions remain nested facts.
/// Delete this DTO when the Workbench adapter readiness view is replaced and
/// no current consumer depends on this projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchAdapterReadiness {
    pub version: PiRpcVersionSummary,
    pub capabilities: HaloWorkbenchCapabilitySummary,
}

impl From<&PiRpcAvailabilitySummary> for HaloWorkbenchAdapterReadiness {
    fn from(summary: &PiRpcAvailabilitySummary) -> Self {
        Self {
            version: summary.version.clone(),
            capabilities: HaloWorkbenchCapabilitySummary {
                required: summary
                    .capabilities
                    .required
                    .iter()
                    .copied()
                    .map(HaloWorkbenchCapability::from)
                    .collect(),
                verified: summary
                    .capabilities
                    .verified
                    .iter()
                    .copied()
                    .map(HaloWorkbenchCapability::from)
                    .collect(),
            },
        }
    }
}

/// Public adapter snapshot owned and emitted by `HaloWorkbenchRuntime`.
///
/// Consumers are the Tauri/Web runtime snapshot and contract tests. Versioning
/// follows `HALO_WORKBENCH_SCHEMA_VERSION`; verification covers serialization,
/// readiness transitions, and cleanup. Delete the readiness field/DTO only
/// when that public snapshot is replaced by a versioned owner contract with no
/// remaining consumer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchAdapterSnapshot {
    pub identity: String,
    pub available: bool,
    pub readiness: Option<HaloWorkbenchAdapterReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchWorkspaceSnapshot {
    pub workspace_id: String,
    pub display_name: String,
    pub root_path: PathBuf,
    pub trusted: bool,
    pub git_repository: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchSessionMode {
    Standard,
    Managed,
}

impl From<HaloWorkbenchSessionMode> for PiRpcSessionMode {
    fn from(mode: HaloWorkbenchSessionMode) -> Self {
        match mode {
            HaloWorkbenchSessionMode::Standard => Self::Standard,
            HaloWorkbenchSessionMode::Managed => Self::Managed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchSessionPhase {
    Creating,
    Idle,
    Running,
    WaitingDeveloper,
    Interrupted,
    Stopping,
    Ended,
    Failed,
}

impl HaloWorkbenchSessionPhase {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Ended | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchSessionSnapshot {
    pub session_id: String,
    pub mode: HaloWorkbenchSessionMode,
    pub phase: HaloWorkbenchSessionPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchOperationKind {
    Permission,
}

impl From<PiRpcOperationKind> for HaloWorkbenchOperationKind {
    fn from(kind: PiRpcOperationKind) -> Self {
        match kind {
            PiRpcOperationKind::Permission => Self::Permission,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchPendingOperationPhase {
    AwaitingDecision,
    DecisionSubmitted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchPendingOperationSnapshot {
    pub operation_id: String,
    pub session_id: String,
    pub kind: HaloWorkbenchOperationKind,
    pub phase: HaloWorkbenchPendingOperationPhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {summary}")]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchError {
    pub code: String,
    pub summary: String,
    pub recovery_action: String,
}

impl HaloWorkbenchError {
    fn new(code: &str, summary: &str, recovery_action: &str) -> Self {
        Self {
            code: code.to_string(),
            summary: summary.to_string(),
            recovery_action: recovery_action.to_string(),
        }
    }

    fn request_id_conflict() -> Self {
        Self::new(
            "request_id_conflict",
            "The request identifier was already used for another intent",
            "create_new_request",
        )
    }

    fn invalid_request(summary: &str) -> Self {
        Self::new("invalid_request", summary, "correct_request")
    }

    fn runtime_not_ready() -> Self {
        Self::new(
            "runtime_not_ready",
            "The Workbench Runtime is not ready",
            "retry_after_runtime_ready",
        )
    }

    fn runtime_shutdown() -> Self {
        Self::new(
            "runtime_shutdown",
            "The Workbench Runtime has shut down",
            "restart_application",
        )
    }

    fn session_not_found() -> Self {
        Self::new(
            "session_not_found",
            "The requested Workbench session was not found",
            "refresh_runtime_snapshot",
        )
    }

    fn session_terminal() -> Self {
        Self::new(
            "session_terminal",
            "The requested Workbench session has ended",
            "create_new_session",
        )
    }

    fn operation_not_found() -> Self {
        Self::new(
            "operation_not_found",
            "The requested operation was not found",
            "refresh_runtime_snapshot",
        )
    }

    fn operation_decision_in_progress() -> Self {
        Self::new(
            "operation_decision_in_progress",
            "A decision for this Workbench operation is awaiting confirmation",
            "wait_for_operation_confirmation",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchSnapshot {
    pub schema_version: u32,
    pub phase: HaloWorkbenchPhase,
    pub adapter: HaloWorkbenchAdapterSnapshot,
    pub workspace: Option<HaloWorkbenchWorkspaceSnapshot>,
    pub sessions: Vec<HaloWorkbenchSessionSnapshot>,
    pub pending_operations: Vec<HaloWorkbenchPendingOperationSnapshot>,
    pub last_sequence: u64,
    pub state_version: u64,
    pub error: Option<HaloWorkbenchError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchEventKind {
    RuntimeStateChanged,
    WorkspaceChanged,
    SessionStateChanged,
    OperationRequested,
    OperationResolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchEvent {
    pub sequence: u64,
    pub state_version: u64,
    pub correlation_id: Option<String>,
    pub kind: HaloWorkbenchEventKind,
    pub summary: String,
    pub session_id: Option<String>,
    pub operation_id: Option<String>,
    pub occurred_at_ms: i64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchWorkspaceInput {
    pub workspace_id: String,
    pub display_name: String,
    pub root_path: PathBuf,
}

impl fmt::Debug for HaloWorkbenchWorkspaceInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HaloWorkbenchWorkspaceInput")
            .field("workspace_id", &self.workspace_id)
            .field("display_name", &self.display_name)
            .field("root_path", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HaloWorkbenchOperationDecision {
    AllowOnce,
    Deny,
}

impl fmt::Debug for HaloWorkbenchOperationDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllowOnce => formatter.write_str("AllowOnce"),
            Self::Deny => formatter.write_str("Deny"),
        }
    }
}

impl From<HaloWorkbenchOperationDecision> for PiRpcOperationDecision {
    fn from(decision: HaloWorkbenchOperationDecision) -> Self {
        match decision {
            HaloWorkbenchOperationDecision::AllowOnce => Self::AllowOnce,
            HaloWorkbenchOperationDecision::Deny => Self::Deny,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HaloWorkbenchIntent {
    OpenWorkspace {
        workspace: HaloWorkbenchWorkspaceInput,
    },
    CloseWorkspace,
    CreateSession {
        mode: HaloWorkbenchSessionMode,
    },
    SendUserInput {
        session_id: String,
        content: String,
    },
    StopSession {
        session_id: String,
    },
    EndSession {
        session_id: String,
    },
    ResolveOperation {
        operation_id: String,
        decision: HaloWorkbenchOperationDecision,
    },
}

impl fmt::Debug for HaloWorkbenchIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenWorkspace { workspace } => formatter
                .debug_struct("OpenWorkspace")
                .field("workspace", workspace)
                .finish(),
            Self::CloseWorkspace => formatter.write_str("CloseWorkspace"),
            Self::CreateSession { mode } => formatter
                .debug_struct("CreateSession")
                .field("mode", mode)
                .finish(),
            Self::SendUserInput { session_id, .. } => formatter
                .debug_struct("SendUserInput")
                .field("session_id", session_id)
                .field("content", &"<redacted>")
                .finish(),
            Self::StopSession { session_id } => formatter
                .debug_struct("StopSession")
                .field("session_id", session_id)
                .finish(),
            Self::EndSession { session_id } => formatter
                .debug_struct("EndSession")
                .field("session_id", session_id)
                .finish(),
            Self::ResolveOperation {
                operation_id,
                decision,
            } => formatter
                .debug_struct("ResolveOperation")
                .field("operation_id", operation_id)
                .field("decision", decision)
                .finish(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchIntentRequest {
    pub request_id: String,
    pub intent: HaloWorkbenchIntent,
}

impl fmt::Debug for HaloWorkbenchIntentRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HaloWorkbenchIntentRequest")
            .field("request_id", &self.request_id)
            .field("intent", &self.intent)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchIntentReceipt {
    pub request_id: String,
    pub state_version: u64,
    pub session_id: Option<String>,
}

type IntentResult = Result<HaloWorkbenchIntentReceipt, HaloWorkbenchError>;
type CleanupResult = Result<(), HaloWorkbenchError>;

enum RequestRecord {
    InFlight {
        fingerprint: [u8; 32],
        result: watch::Sender<Option<IntentResult>>,
    },
    Complete {
        fingerprint: [u8; 32],
        result: IntentResult,
    },
}

#[derive(Default)]
struct RequestLedger {
    records: HashMap<String, RequestRecord>,
}

impl RequestLedger {
    fn record_complete(&mut self, request_id: String, fingerprint: [u8; 32], result: IntentResult) {
        self.records.insert(
            request_id,
            RequestRecord::Complete {
                fingerprint,
                result,
            },
        );
        while self
            .records
            .values()
            .filter(|record| matches!(record, RequestRecord::Complete { .. }))
            .count()
            > MAX_COMPLETED_REQUEST_RECORDS
        {
            let Some(request_id) = self.records.iter().find_map(|(request_id, record)| {
                matches!(record, RequestRecord::Complete { .. }).then_some(request_id.clone())
            }) else {
                break;
            };
            self.records.remove(&request_id);
        }
    }
}

enum CleanupRecord {
    InFlight {
        result: watch::Sender<Option<CleanupResult>>,
    },
    Complete(CleanupResult),
}

struct RuntimeState {
    phase: HaloWorkbenchPhase,
    adapter_available: bool,
    adapter_readiness: Option<HaloWorkbenchAdapterReadiness>,
    workspace: Option<HaloWorkbenchWorkspaceSnapshot>,
    sessions: BTreeMap<String, HaloWorkbenchSessionSnapshot>,
    pending_operations: BTreeMap<String, HaloWorkbenchPendingOperationSnapshot>,
    error: Option<HaloWorkbenchError>,
    sequence: u64,
    state_version: u64,
    generation: u64,
    adapter_generation: Option<u64>,
    cleanup_started: HashSet<u64>,
    terminated: bool,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            phase: HaloWorkbenchPhase::Disconnected,
            adapter_available: false,
            adapter_readiness: None,
            workspace: None,
            sessions: BTreeMap::new(),
            pending_operations: BTreeMap::new(),
            error: None,
            sequence: 0,
            state_version: 0,
            generation: 0,
            adapter_generation: None,
            cleanup_started: HashSet::new(),
            terminated: false,
        }
    }
}

struct HaloWorkbenchRuntimeInner {
    adapter: Arc<dyn PiRpcPort>,
    workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
    provider_readiness: Arc<dyn PiProviderReadinessPort>,
    clock: Arc<dyn ClockPort>,
    state: Mutex<RuntimeState>,
    requests: tokio::sync::Mutex<RequestLedger>,
    cleanups: tokio::sync::Mutex<HashMap<u64, CleanupRecord>>,
    lifecycle_actions: tokio::sync::Mutex<()>,
    adapter_actions: tokio::sync::Mutex<()>,
    events: broadcast::Sender<HaloWorkbenchEvent>,
    adapter_events_started: AtomicBool,
    shutdown_result: OnceCell<Result<(), HaloWorkbenchError>>,
}

impl HaloWorkbenchRuntimeInner {
    fn snapshot(&self) -> HaloWorkbenchSnapshot {
        let state = self.state.lock().expect("Halo Workbench state lock");
        HaloWorkbenchSnapshot {
            schema_version: HALO_WORKBENCH_SCHEMA_VERSION,
            phase: state.phase,
            adapter: HaloWorkbenchAdapterSnapshot {
                identity: PI_RPC_ADAPTER_IDENTITY.to_string(),
                available: state.adapter_available,
                readiness: state.adapter_readiness.clone(),
            },
            workspace: state.workspace.clone(),
            sessions: state.sessions.values().cloned().collect(),
            pending_operations: state.pending_operations.values().cloned().collect(),
            last_sequence: state.sequence,
            state_version: state.state_version,
            error: state.error.clone(),
        }
    }

    fn receipt(&self, request_id: &str, session_id: Option<String>) -> HaloWorkbenchIntentReceipt {
        HaloWorkbenchIntentReceipt {
            request_id: request_id.to_string(),
            state_version: self
                .state
                .lock()
                .expect("Halo Workbench state lock")
                .state_version,
            session_id,
        }
    }

    fn publish_transition(
        &self,
        correlation_id: Option<&str>,
        kind: HaloWorkbenchEventKind,
        summary: &'static str,
        session_id: Option<String>,
        operation_id: Option<String>,
        mutate: impl FnOnce(&mut RuntimeState) -> bool,
    ) -> bool {
        let mut state = self.state.lock().expect("Halo Workbench state lock");
        if !mutate(&mut state) {
            return false;
        }
        state.sequence = state
            .sequence
            .checked_add(1)
            .expect("Halo Workbench event sequence exhausted");
        state.state_version = state
            .state_version
            .checked_add(1)
            .expect("Halo Workbench state version exhausted");
        let event = HaloWorkbenchEvent {
            sequence: state.sequence,
            state_version: state.state_version,
            correlation_id: correlation_id.map(str::to_string),
            kind,
            summary: summary.to_string(),
            session_id,
            operation_id,
            occurred_at_ms: self.clock.now_unix_millis(),
        };
        let _ = self.events.send(event);
        true
    }

    fn apply_adapter_event(&self, event: PiRpcEvent) {
        let generation = event.generation();
        match event {
            PiRpcEvent::Ready { .. } => {
                self.publish_transition(
                    None,
                    HaloWorkbenchEventKind::RuntimeStateChanged,
                    "Workbench Runtime is ready",
                    None,
                    None,
                    |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Starting
                            || state.terminated
                        {
                            return false;
                        }
                        state.phase = HaloWorkbenchPhase::Ready;
                        state.adapter_available = true;
                        state.error = None;
                        true
                    },
                );
            }
            PiRpcEvent::Failed { reason, .. } => {
                let error = adapter_failure(reason);
                self.fail_generation(generation, None, error);
            }
            PiRpcEvent::SessionCreated { session_id, .. }
            | PiRpcEvent::SessionIdle { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Idle,
                    "Workbench session is idle",
                );
            }
            PiRpcEvent::AgentSettled { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::WaitingDeveloper,
                    "Workbench session is waiting for developer",
                );
            }
            PiRpcEvent::SessionStopped { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Interrupted,
                    "Workbench session was interrupted",
                );
            }
            PiRpcEvent::SessionRunning { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Running,
                    "Workbench session is running",
                );
            }
            PiRpcEvent::SessionEnded { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Ended,
                    "Workbench session ended",
                );
            }
            PiRpcEvent::SessionFailed { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Failed,
                    "Workbench session failed",
                );
            }
            PiRpcEvent::MessageUpdated { session_id, .. } => {
                self.publish_session_observation(
                    generation,
                    &session_id,
                    "Workbench session message updated",
                );
            }
            PiRpcEvent::ToolExecutionStarted { session_id, .. } => {
                self.publish_session_observation(
                    generation,
                    &session_id,
                    "Workbench tool execution started",
                );
            }
            PiRpcEvent::ToolExecutionUpdated { session_id, .. } => {
                self.publish_session_observation(
                    generation,
                    &session_id,
                    "Workbench tool execution updated",
                );
            }
            PiRpcEvent::ToolExecutionEnded { session_id, .. } => {
                self.publish_session_observation(
                    generation,
                    &session_id,
                    "Workbench tool execution ended",
                );
            }
            PiRpcEvent::OperationRequested {
                session_id,
                operation_id,
                kind,
                ..
            } => {
                let event_session_id = session_id.clone();
                let event_operation_id = operation_id.clone();
                self.publish_transition(
                    None,
                    HaloWorkbenchEventKind::OperationRequested,
                    "A Workbench operation requires a decision",
                    Some(event_session_id),
                    Some(event_operation_id),
                    move |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Ready
                            || state
                                .sessions
                                .get(&session_id)
                                .is_none_or(|session| session.phase.is_terminal())
                            || state.pending_operations.contains_key(&operation_id)
                        {
                            return false;
                        }
                        state.pending_operations.insert(
                            operation_id.clone(),
                            HaloWorkbenchPendingOperationSnapshot {
                                operation_id,
                                session_id,
                                kind: kind.into(),
                                phase: HaloWorkbenchPendingOperationPhase::AwaitingDecision,
                            },
                        );
                        true
                    },
                );
            }
            PiRpcEvent::OperationResolved {
                session_id,
                operation_id,
                ..
            } => {
                let event_session_id = session_id.clone();
                let event_operation_id = operation_id.clone();
                self.publish_transition(
                    None,
                    HaloWorkbenchEventKind::OperationResolved,
                    "Workbench operation was resolved",
                    Some(event_session_id),
                    Some(event_operation_id),
                    move |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Ready
                        {
                            return false;
                        }
                        let belongs_to_session = state
                            .pending_operations
                            .get(&operation_id)
                            .is_some_and(|operation| operation.session_id == session_id);
                        belongs_to_session
                            && state.pending_operations.remove(&operation_id).is_some()
                    },
                );
            }
        }
    }

    fn publish_session_observation(
        &self,
        generation: u64,
        session_id: &str,
        summary: &'static str,
    ) {
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            summary,
            Some(owned_session_id.clone()),
            None,
            move |state| {
                state.generation == generation
                    && state.phase == HaloWorkbenchPhase::Ready
                    && state
                        .sessions
                        .get(&owned_session_id)
                        .is_some_and(|session| !session.phase.is_terminal())
            },
        );
    }

    fn set_session_phase(
        &self,
        generation: u64,
        session_id: &str,
        phase: HaloWorkbenchSessionPhase,
        summary: &'static str,
    ) {
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            summary,
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if session.phase == phase
                    || session.phase.is_terminal()
                    || !valid_session_transition(session.phase, phase)
                {
                    return false;
                }
                session.phase = phase;
                if phase.is_terminal() {
                    state
                        .pending_operations
                        .retain(|_, operation| operation.session_id != owned_session_id);
                }
                true
            },
        );
    }

    fn fail_generation(
        &self,
        generation: u64,
        correlation_id: Option<&str>,
        error: HaloWorkbenchError,
    ) -> bool {
        self.publish_transition(
            correlation_id,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime failed",
            None,
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Failed;
                state.adapter_available = false;
                state.error = Some(error);
                true
            },
        )
    }

    fn fail_adapter_event_gap(&self) {
        self.fail_active_adapter_stream(HaloWorkbenchError::new(
            "adapter_event_gap",
            "The Workbench execution event stream has a gap",
            "restart_runtime",
        ));
    }

    fn fail_adapter_event_stream_closed(&self) {
        self.fail_active_adapter_stream(HaloWorkbenchError::new(
            "adapter_event_stream_closed",
            "The Workbench execution event stream closed unexpectedly",
            "restart_runtime",
        ));
    }

    fn fail_active_adapter_stream(&self, error: HaloWorkbenchError) {
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime event stream failed",
            None,
            None,
            move |state| {
                if state.terminated
                    || !matches!(
                        state.phase,
                        HaloWorkbenchPhase::Probing
                            | HaloWorkbenchPhase::Starting
                            | HaloWorkbenchPhase::Ready
                    )
                {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Failed;
                state.adapter_available = false;
                state.error = Some(error);
                true
            },
        );
    }
}

impl Drop for HaloWorkbenchRuntimeInner {
    fn drop(&mut self) {
        let state = match self.state.get_mut() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(generation) = state.adapter_generation else {
            return;
        };
        if !state.cleanup_started.insert(generation) {
            return;
        }
        let adapter = self.adapter.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = adapter.execute(PiRpcCommand::Shutdown { generation }).await;
            });
        }
    }
}

#[derive(Clone)]
pub struct HaloWorkbenchRuntime {
    inner: Arc<HaloWorkbenchRuntimeInner>,
}

impl HaloWorkbenchRuntime {
    pub fn new(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(HaloWorkbenchRuntimeInner {
                adapter,
                workspace_facts,
                provider_readiness,
                clock,
                state: Mutex::new(RuntimeState::default()),
                requests: tokio::sync::Mutex::new(RequestLedger::default()),
                cleanups: tokio::sync::Mutex::new(HashMap::new()),
                lifecycle_actions: tokio::sync::Mutex::new(()),
                adapter_actions: tokio::sync::Mutex::new(()),
                events,
                adapter_events_started: AtomicBool::new(false),
                shutdown_result: OnceCell::new(),
            }),
        }
    }

    pub fn snapshot(&self) -> HaloWorkbenchSnapshot {
        self.inner.snapshot()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HaloWorkbenchEvent> {
        self.inner.events.subscribe()
    }

    pub async fn submit(&self, request: HaloWorkbenchIntentRequest) -> IntentResult {
        if request.request_id.trim().is_empty() {
            return Err(HaloWorkbenchError::invalid_request(
                "A non-empty request identifier is required",
            ));
        }
        if self
            .inner
            .state
            .lock()
            .expect("Halo Workbench state lock")
            .terminated
        {
            return Err(HaloWorkbenchError::runtime_shutdown());
        }
        self.ensure_adapter_event_loop();

        let fingerprint = request_fingerprint(&request.intent)?;
        let (owner_sender, mut waiter) = {
            let mut ledger = self.inner.requests.lock().await;
            match ledger.records.get(&request.request_id) {
                Some(RequestRecord::Complete {
                    fingerprint: existing,
                    result,
                }) => {
                    return if existing == &fingerprint {
                        result.clone()
                    } else {
                        Err(HaloWorkbenchError::request_id_conflict())
                    };
                }
                Some(RequestRecord::InFlight {
                    fingerprint: existing,
                    result,
                }) => {
                    if existing != &fingerprint {
                        return Err(HaloWorkbenchError::request_id_conflict());
                    }
                    (None, Some(result.subscribe()))
                }
                None => {
                    let (sender, receiver) = watch::channel(None);
                    ledger.records.insert(
                        request.request_id.clone(),
                        RequestRecord::InFlight {
                            fingerprint,
                            result: sender.clone(),
                        },
                    );
                    (Some(sender), Some(receiver))
                }
            }
        };

        if owner_sender.is_none() {
            let waiter = waiter.as_mut().expect("duplicate request waiter");
            loop {
                if let Some(result) = waiter.borrow().clone() {
                    return result;
                }
                if waiter.changed().await.is_err() {
                    return Err(HaloWorkbenchError::new(
                        "runtime_internal",
                        "The Workbench request owner stopped unexpectedly",
                        "retry",
                    ));
                }
            }
        }

        let sender = owner_sender.expect("request owner sender");
        let runtime = self.clone();
        let request_id = request.request_id;
        let intent = request.intent;
        tokio::spawn(async move {
            let execution_runtime = runtime.clone();
            let execution_request_id = request_id.clone();
            let execution = tokio::spawn(async move {
                execution_runtime
                    .execute_intent(&execution_request_id, intent)
                    .await
            });
            let result = match execution.await {
                Ok(result) => result,
                Err(_) => Err(HaloWorkbenchError::new(
                    "runtime_internal",
                    "The Workbench request execution stopped unexpectedly",
                    "retry",
                )),
            };
            sender.send_replace(Some(result.clone()));
            let mut ledger = runtime.inner.requests.lock().await;
            ledger.record_complete(request_id, fingerprint, result);
        });

        let waiter = waiter.as_mut().expect("request owner waiter");
        loop {
            if let Some(result) = waiter.borrow().clone() {
                return result;
            }
            if waiter.changed().await.is_err() {
                return Err(HaloWorkbenchError::new(
                    "runtime_internal",
                    "The Workbench request owner stopped unexpectedly",
                    "retry",
                ));
            }
        }
    }

    pub async fn shutdown(&self) -> Result<(), HaloWorkbenchError> {
        let runtime = self.clone();
        self.inner
            .shutdown_result
            .get_or_init(|| async move { runtime.shutdown_inner().await })
            .await
            .clone()
    }

    fn ensure_adapter_event_loop(&self) {
        if self
            .inner
            .adapter_events_started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let mut events = self.inner.adapter.subscribe();
        let inner: Weak<HaloWorkbenchRuntimeInner> = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.apply_adapter_event(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.fail_adapter_event_gap();
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.fail_adapter_event_stream_closed();
                        break;
                    }
                }
            }
        });
    }

    async fn execute_intent(&self, request_id: &str, intent: HaloWorkbenchIntent) -> IntentResult {
        match intent {
            HaloWorkbenchIntent::OpenWorkspace { workspace } => {
                self.open_workspace(request_id, workspace).await
            }
            HaloWorkbenchIntent::CloseWorkspace => {
                self.close_workspace(Some(request_id), false).await?;
                Ok(self.inner.receipt(request_id, None))
            }
            HaloWorkbenchIntent::CreateSession { mode } => {
                self.create_session(request_id, mode).await
            }
            HaloWorkbenchIntent::SendUserInput {
                session_id,
                content,
            } => {
                self.session_command(
                    request_id,
                    &session_id,
                    SessionIntent::SendUserInput(content),
                )
                .await
            }
            HaloWorkbenchIntent::StopSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::Stop)
                    .await
            }
            HaloWorkbenchIntent::EndSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::End)
                    .await
            }
            HaloWorkbenchIntent::ResolveOperation {
                operation_id,
                decision,
            } => {
                self.resolve_operation(request_id, &operation_id, decision)
                    .await
            }
        }
    }

    async fn open_workspace(
        &self,
        request_id: &str,
        workspace: HaloWorkbenchWorkspaceInput,
    ) -> IntentResult {
        validate_workspace_input(&workspace)?;
        let (cleanup_generation, generation) = {
            let _lifecycle = self.inner.lifecycle_actions.lock().await;
            let mut state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.terminated {
                return Err(HaloWorkbenchError::runtime_shutdown());
            }
            let cleanup_generation = state.adapter_generation;
            state.generation = state.generation.saturating_add(1);
            state.cleanup_started.clear();
            if cleanup_generation.is_some() || state.phase != HaloWorkbenchPhase::Disconnected {
                state.phase = HaloWorkbenchPhase::Stopping;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.error = None;
            }
            (cleanup_generation, state.generation)
        };

        if let Some(cleanup_generation) = cleanup_generation {
            self.cleanup_generation(cleanup_generation, generation, Some(request_id))
                .await?;
        }
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }

        let facts = self
            .inner
            .workspace_facts
            .inspect(WorkbenchWorkspaceFactsRequest {
                workspace_id: workspace.workspace_id.clone(),
                root: workspace.root_path.clone(),
            })
            .await;
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        let facts = match facts {
            Ok(facts) => facts,
            Err(_) => {
                let error = HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace facts could not be verified",
                    "retry",
                );
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
        };
        if facts.workspace_id != workspace.workspace_id {
            let error = HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "Workspace identity verification failed",
                "refresh_workspace",
            );
            self.inner
                .fail_generation(generation, Some(request_id), error.clone());
            return Err(error);
        }
        if !facts.trusted {
            let error = HaloWorkbenchError::new(
                "workspace_untrusted",
                "The selected workspace is not trusted",
                "choose_trusted_workspace",
            );
            self.inner
                .fail_generation(generation, Some(request_id), error.clone());
            return Err(error);
        }

        let adapter_workspace = PiRpcWorkspace {
            workspace_id: facts.workspace_id.clone(),
            canonical_root: facts.canonical_root.clone(),
        };
        let public_workspace = HaloWorkbenchWorkspaceSnapshot {
            workspace_id: facts.workspace_id,
            display_name: workspace.display_name,
            root_path: facts.canonical_root,
            trusted: facts.trusted,
            git_repository: facts.git_repository,
        };
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::WorkspaceChanged,
            "Workbench workspace is being probed",
            None,
            None,
            move |state| {
                if state.generation != generation || state.terminated {
                    return false;
                }
                state.workspace = Some(public_workspace);
                state.adapter_generation = Some(generation);
                state.sessions.clear();
                state.pending_operations.clear();
                state.phase = HaloWorkbenchPhase::Probing;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.error = None;
                true
            },
        );

        let probe = self
            .inner
            .adapter
            .execute(PiRpcCommand::Probe {
                generation,
                workspace: adapter_workspace.clone(),
            })
            .await
            .map_err(|error| port_failure(error.kind));
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        let adapter_readiness = match probe {
            Ok(PiRpcReply::Available { summary }) => {
                if !valid_adapter_profile_summary(&summary) {
                    let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                    self.inner
                        .fail_generation(generation, Some(request_id), error.clone());
                    return Err(error);
                }
                summary
            }
            Ok(PiRpcReply::Accepted) => {
                let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
            Ok(PiRpcReply::Ready { .. }) => {
                let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
            Ok(PiRpcReply::Unavailable { reason }) => {
                let error = adapter_failure(reason);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
            Err(error) => {
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
        };
        let public_profile_readiness = HaloWorkbenchAdapterReadiness::from(&adapter_readiness);
        let public_profile_readiness_for_event = public_profile_readiness.clone();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime adapter profile was verified",
            None,
            None,
            move |state| {
                if state.generation != generation
                    || state.phase != HaloWorkbenchPhase::Probing
                    || state.terminated
                {
                    return false;
                }
                state.adapter_readiness = Some(public_profile_readiness_for_event);
                true
            },
        );

        let provider_readiness = self.inner.provider_readiness.check().await;
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        let provider_readiness = match provider_readiness {
            Ok(provider_readiness) => provider_readiness,
            Err(_) => {
                let error = HaloWorkbenchError::new(
                    "provider_readiness_unavailable",
                    "Pi provider readiness could not be verified",
                    "retry",
                );
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
        };
        if !provider_readiness.available {
            let error = HaloWorkbenchError::new(
                "provider_unavailable",
                "Pi provider/model readiness is not available",
                "configure_provider",
            );
            self.inner
                .fail_generation(generation, Some(request_id), error.clone());
            return Err(error);
        }

        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime is starting",
            None,
            None,
            move |state| {
                if state.generation != generation || state.terminated {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Starting;
                state.adapter_available = true;
                state.adapter_readiness = Some(public_profile_readiness);
                state.error = None;
                true
            },
        );
        let start = {
            let _action = self.inner.adapter_actions.lock().await;
            if !self.is_current_generation(generation) {
                return Ok(self.inner.receipt(request_id, None));
            }
            self.inner
                .adapter
                .execute(PiRpcCommand::Start {
                    generation,
                    workspace: adapter_workspace,
                })
                .await
                .map_err(|error| port_failure(error.kind))
        };
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        match start {
            Ok(PiRpcReply::Ready { summary }) => {
                if !valid_adapter_ready_summary(&summary) {
                    let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                    self.inner
                        .fail_generation(generation, Some(request_id), error.clone());
                    return Err(error);
                }
                let public_adapter_readiness = HaloWorkbenchAdapterReadiness::from(&summary);
                self.inner.publish_transition(
                    Some(request_id),
                    HaloWorkbenchEventKind::RuntimeStateChanged,
                    "Workbench Runtime adapter readiness handshake was verified",
                    None,
                    None,
                    move |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Starting
                            || state.terminated
                        {
                            return false;
                        }
                        state.adapter_readiness = Some(public_adapter_readiness);
                        true
                    },
                );
                Ok(self.inner.receipt(request_id, None))
            }
            Ok(PiRpcReply::Accepted) | Ok(PiRpcReply::Available { .. }) => {
                let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                Err(error)
            }
            Ok(PiRpcReply::Unavailable { reason }) => {
                let error = adapter_failure(reason);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                Err(error)
            }
            Err(error) => {
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                Err(error)
            }
        }
    }

    async fn close_workspace(
        &self,
        correlation_id: Option<&str>,
        terminate: bool,
    ) -> Result<(), HaloWorkbenchError> {
        let (cleanup_generation, generation) = {
            let _lifecycle = self.inner.lifecycle_actions.lock().await;
            let mut state = self.inner.state.lock().expect("Halo Workbench state lock");
            if terminate {
                state.terminated = true;
            }
            let cleanup_generation = state.adapter_generation;
            state.generation = state.generation.saturating_add(1);
            state.cleanup_started.clear();
            if cleanup_generation.is_some() || state.phase != HaloWorkbenchPhase::Disconnected {
                state.phase = HaloWorkbenchPhase::Stopping;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.error = None;
            }
            (cleanup_generation, state.generation)
        };
        if let Some(cleanup_generation) = cleanup_generation {
            self.cleanup_generation(cleanup_generation, generation, correlation_id)
                .await?;
        } else {
            self.inner.publish_transition(
                correlation_id,
                HaloWorkbenchEventKind::WorkspaceChanged,
                "Workbench workspace was closed",
                None,
                None,
                |state| {
                    if state.generation != generation
                        || (state.phase == HaloWorkbenchPhase::Disconnected
                            && state.workspace.is_none()
                            && state.sessions.is_empty()
                            && state.pending_operations.is_empty()
                            && state.error.is_none())
                    {
                        return false;
                    }
                    state.phase = HaloWorkbenchPhase::Disconnected;
                    state.adapter_available = false;
                    state.adapter_readiness = None;
                    state.workspace = None;
                    state.sessions.clear();
                    state.pending_operations.clear();
                    state.error = None;
                    true
                },
            );
        }
        Ok(())
    }

    async fn cleanup_generation(
        &self,
        cleanup_generation: u64,
        fence_generation: u64,
        correlation_id: Option<&str>,
    ) -> Result<(), HaloWorkbenchError> {
        self.inner.publish_transition(
            correlation_id,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime is stopping",
            None,
            None,
            |state| {
                if state.generation != fence_generation {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Stopping;
                true
            },
        );
        let result = self.execute_cleanup_once(cleanup_generation).await;
        if !self.is_current_generation(fence_generation) {
            return Ok(());
        }
        if result.is_err() {
            let error = HaloWorkbenchError::new(
                "cleanup_failed",
                "Workbench Runtime cleanup did not complete",
                "restart_application",
            );
            self.inner
                .fail_generation(fence_generation, correlation_id, error.clone());
            return Err(error);
        }
        self.inner.publish_transition(
            correlation_id,
            HaloWorkbenchEventKind::WorkspaceChanged,
            "Workbench workspace was closed",
            None,
            None,
            |state| {
                if state.generation != fence_generation {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Disconnected;
                state.adapter_available = false;
                state.adapter_readiness = None;
                if state.adapter_generation == Some(cleanup_generation) {
                    state.adapter_generation = None;
                }
                state.workspace = None;
                state.sessions.clear();
                state.pending_operations.clear();
                state.error = None;
                true
            },
        );
        Ok(())
    }

    async fn execute_cleanup_once(&self, generation: u64) -> CleanupResult {
        let mut result = {
            let mut cleanups = self.inner.cleanups.lock().await;
            match cleanups.get(&generation) {
                Some(CleanupRecord::Complete(result)) => return result.clone(),
                Some(CleanupRecord::InFlight { result }) => result.subscribe(),
                None => {
                    let (sender, receiver) = watch::channel(None);
                    cleanups.insert(
                        generation,
                        CleanupRecord::InFlight {
                            result: sender.clone(),
                        },
                    );
                    self.inner
                        .state
                        .lock()
                        .expect("Halo Workbench state lock")
                        .cleanup_started
                        .insert(generation);
                    let inner = self.inner.clone();
                    tokio::spawn(async move {
                        let cleanup_result = match {
                            let _actions = inner.adapter_actions.lock().await;
                            inner
                                .adapter
                                .execute(PiRpcCommand::Shutdown { generation })
                                .await
                        } {
                            Ok(PiRpcReply::Accepted)
                            | Ok(PiRpcReply::Available { .. })
                            | Ok(PiRpcReply::Ready { .. }) => Ok(()),
                            Ok(PiRpcReply::Unavailable { .. }) => Err(HaloWorkbenchError::new(
                                "cleanup_failed",
                                "Workbench Runtime cleanup did not complete",
                                "restart_application",
                            )),
                            Err(error) => Err(port_failure(error.kind)),
                        };
                        sender.send_replace(Some(cleanup_result.clone()));
                        let mut cleanups = inner.cleanups.lock().await;
                        cleanups.insert(generation, CleanupRecord::Complete(cleanup_result));
                        while cleanups
                            .values()
                            .filter(|record| matches!(record, CleanupRecord::Complete(_)))
                            .count()
                            > MAX_COMPLETED_CLEANUP_RECORDS
                        {
                            let Some(generation) =
                                cleanups.iter().find_map(|(generation, record)| {
                                    matches!(record, CleanupRecord::Complete(_))
                                        .then_some(*generation)
                                })
                            else {
                                break;
                            };
                            cleanups.remove(&generation);
                        }
                    });
                    receiver
                }
            }
        };

        loop {
            if let Some(cleanup_result) = result.borrow().clone() {
                return cleanup_result;
            }
            if result.changed().await.is_err() {
                return Err(HaloWorkbenchError::new(
                    "cleanup_failed",
                    "Workbench Runtime cleanup did not complete",
                    "restart_application",
                ));
            }
        }
    }

    async fn create_session(
        &self,
        request_id: &str,
        mode: HaloWorkbenchSessionMode,
    ) -> IntentResult {
        let session_id = Uuid::new_v4().to_string();
        let generation = self.ready_generation()?;
        let event_session_id = session_id.clone();
        let state_session_id = session_id.clone();
        if !self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is being created",
            Some(event_session_id),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                state.sessions.insert(
                    state_session_id.clone(),
                    HaloWorkbenchSessionSnapshot {
                        session_id: state_session_id,
                        mode,
                        phase: HaloWorkbenchSessionPhase::Creating,
                    },
                );
                true
            },
        ) {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        let result = self
            .execute_session_adapter_action(
                generation,
                &session_id,
                PiRpcCommand::CreateSession {
                    generation,
                    task_id: session_id.clone(),
                    session_id: session_id.clone(),
                    mode: mode.into(),
                },
            )
            .await;
        self.finish_session_command(
            generation,
            request_id,
            &session_id,
            result,
            HaloWorkbenchSessionPhase::Failed,
        )?;
        Ok(self.inner.receipt(request_id, Some(session_id)))
    }

    async fn session_command(
        &self,
        request_id: &str,
        session_id: &str,
        intent: SessionIntent,
    ) -> IntentResult {
        if let SessionIntent::SendUserInput(content) = &intent {
            validate_user_input(content)?;
        }
        let generation = self.ready_generation()?;
        self.ensure_session_action_allowed(generation, session_id)?;
        let command = match intent {
            SessionIntent::SendUserInput(content) => PiRpcCommand::SendUserInput {
                generation,
                session_id: session_id.to_string(),
                content,
            },
            SessionIntent::Stop => {
                self.mark_session_stopping(generation, request_id, session_id);
                PiRpcCommand::StopSession {
                    generation,
                    session_id: session_id.to_string(),
                }
            }
            SessionIntent::End => {
                self.mark_session_stopping(generation, request_id, session_id);
                PiRpcCommand::EndSession {
                    generation,
                    session_id: session_id.to_string(),
                }
            }
        };
        let result = self
            .execute_session_adapter_action(generation, session_id, command)
            .await;
        self.finish_session_command(
            generation,
            request_id,
            session_id,
            result,
            HaloWorkbenchSessionPhase::Failed,
        )?;
        Ok(self.inner.receipt(request_id, Some(session_id.to_string())))
    }

    async fn execute_session_adapter_action(
        &self,
        generation: u64,
        session_id: &str,
        command: PiRpcCommand,
    ) -> Result<PiRpcReply, HaloWorkbenchError> {
        self.ensure_workspace_trusted(generation).await?;
        self.ensure_session_action_allowed(generation, session_id)?;
        let result = {
            let _action = self.inner.adapter_actions.lock().await;
            self.ensure_session_action_allowed(generation, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        };
        self.ensure_workspace_trusted(generation).await?;
        self.ensure_session_action_allowed(generation, session_id)?;
        result
    }

    async fn ensure_workspace_trusted(&self, generation: u64) -> Result<(), HaloWorkbenchError> {
        let request = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return Err(HaloWorkbenchError::runtime_not_ready());
            }
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            WorkbenchWorkspaceFactsRequest {
                workspace_id: workspace.workspace_id.clone(),
                root: workspace.root_path.clone(),
            }
        };

        let facts = self
            .inner
            .workspace_facts
            .inspect(request.clone())
            .await
            .map_err(|_| {
                HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace trust could not be revalidated",
                    "retry",
                )
            })?;
        let still_trusted = facts.workspace_id == request.workspace_id
            && facts.canonical_root == request.root
            && facts.trusted;
        if still_trusted {
            return Ok(());
        }

        let error = HaloWorkbenchError::new(
            "workspace_untrusted",
            "Workspace trust was revoked while the task was active",
            "choose_trusted_workspace",
        );
        let _ = self.close_workspace(None, false).await;
        Err(error)
    }

    fn ensure_session_action_allowed(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.terminated
            || state.generation != generation
            || state.phase != HaloWorkbenchPhase::Ready
        {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(HaloWorkbenchError::session_not_found)?;
        if session.phase.is_terminal() {
            return Err(HaloWorkbenchError::session_terminal());
        }
        Ok(())
    }

    fn mark_session_stopping(&self, generation: u64, request_id: &str, session_id: &str) {
        let session_id = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is stopping",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase.is_terminal() {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Stopping;
                true
            },
        );
    }

    fn finish_session_command(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        result: Result<PiRpcReply, HaloWorkbenchError>,
        failure_phase: HaloWorkbenchSessionPhase,
    ) -> Result<(), HaloWorkbenchError> {
        let error = match result {
            Ok(PiRpcReply::Accepted)
            | Ok(PiRpcReply::Available { .. })
            | Ok(PiRpcReply::Ready { .. }) => return Ok(()),
            Ok(PiRpcReply::Unavailable { reason }) => adapter_failure(reason),
            Err(error) => error,
        };
        let session_id = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session command failed",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase.is_terminal() {
                    return false;
                }
                session.phase = failure_phase;
                true
            },
        );
        Err(error)
    }

    async fn resolve_operation(
        &self,
        request_id: &str,
        operation_id: &str,
        decision: HaloWorkbenchOperationDecision,
    ) -> IntentResult {
        let generation = self.ready_generation()?;
        let session_id = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state
                .pending_operations
                .get(operation_id)
                .map(|operation| operation.session_id.clone())
                .ok_or_else(HaloWorkbenchError::operation_not_found)?
        };
        self.ensure_workspace_trusted(generation).await?;
        self.ensure_session_action_allowed(generation, &session_id)?;
        validate_operation_decision(&decision)?;
        let owned_operation_id = operation_id.to_string();
        let claimed = self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::OperationRequested,
            "Workbench operation decision was submitted",
            Some(session_id.clone()),
            Some(owned_operation_id.clone()),
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(operation) = state.pending_operations.get_mut(&owned_operation_id) else {
                    return false;
                };
                if operation.phase != HaloWorkbenchPendingOperationPhase::AwaitingDecision {
                    return false;
                }
                operation.phase = HaloWorkbenchPendingOperationPhase::DecisionSubmitted;
                true
            },
        );
        if !claimed {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return Err(HaloWorkbenchError::runtime_not_ready());
            }
            return if state.pending_operations.contains_key(operation_id) {
                Err(HaloWorkbenchError::operation_decision_in_progress())
            } else {
                Err(HaloWorkbenchError::operation_not_found())
            };
        }
        let result = {
            let _action = self.inner.adapter_actions.lock().await;
            self.ensure_session_action_allowed(generation, &session_id)?;
            let operation_is_claimed = self
                .inner
                .state
                .lock()
                .expect("Halo Workbench state lock")
                .pending_operations
                .get(operation_id)
                .is_some_and(|operation| {
                    operation.session_id == session_id
                        && operation.phase == HaloWorkbenchPendingOperationPhase::DecisionSubmitted
                });
            if !operation_is_claimed {
                return Err(HaloWorkbenchError::operation_not_found());
            }
            let result = self
                .inner
                .adapter
                .execute(PiRpcCommand::ResolveOperation {
                    generation,
                    task_id: session_id.clone(),
                    session_id: session_id.clone(),
                    operation_id: operation_id.to_string(),
                    decision: decision.into(),
                })
                .await
                .map_err(|error| port_failure(error.kind));
            result
        };
        self.ensure_workspace_trusted(generation).await?;
        self.ensure_session_action_allowed(generation, &session_id)?;
        match result {
            Ok(PiRpcReply::Accepted)
            | Ok(PiRpcReply::Available { .. })
            | Ok(PiRpcReply::Ready { .. }) => Ok(self.inner.receipt(request_id, Some(session_id))),
            Ok(PiRpcReply::Unavailable { reason }) => {
                let error = adapter_failure(reason);
                self.restore_operation(generation, request_id, operation_id, &session_id);
                Err(error)
            }
            Err(error) => {
                self.restore_operation(generation, request_id, operation_id, &session_id);
                Err(error)
            }
        }
    }

    fn restore_operation(
        &self,
        generation: u64,
        request_id: &str,
        operation_id: &str,
        session_id: &str,
    ) {
        let operation_id = operation_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::OperationRequested,
            "Workbench operation decision was not accepted",
            Some(session_id.to_string()),
            Some(operation_id.clone()),
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(operation) = state.pending_operations.get_mut(&operation_id) else {
                    return false;
                };
                operation.phase = HaloWorkbenchPendingOperationPhase::AwaitingDecision;
                true
            },
        );
    }

    fn ready_generation(&self) -> Result<u64, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.terminated {
            return Err(HaloWorkbenchError::runtime_shutdown());
        }
        if state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        Ok(state.generation)
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        self.inner
            .state
            .lock()
            .expect("Halo Workbench state lock")
            .generation
            == generation
    }

    async fn shutdown_inner(&self) -> Result<(), HaloWorkbenchError> {
        self.close_workspace(None, true).await
    }
}

enum SessionIntent {
    SendUserInput(String),
    Stop,
    End,
}

fn valid_session_transition(
    from: HaloWorkbenchSessionPhase,
    to: HaloWorkbenchSessionPhase,
) -> bool {
    use HaloWorkbenchSessionPhase::*;

    matches!(
        (from, to),
        (Creating, Idle | Running | Stopping | Ended | Failed)
            | (Idle, Running | WaitingDeveloper | Stopping | Ended | Failed)
            | (
                Running,
                Idle | WaitingDeveloper | Interrupted | Stopping | Ended | Failed
            )
            | (
                WaitingDeveloper,
                Running | Interrupted | Stopping | Ended | Failed
            )
            | (
                Interrupted,
                Running | WaitingDeveloper | Stopping | Ended | Failed
            )
            | (Stopping, WaitingDeveloper | Interrupted | Ended | Failed)
    )
}

fn validate_workspace_input(
    workspace: &HaloWorkbenchWorkspaceInput,
) -> Result<(), HaloWorkbenchError> {
    if workspace.workspace_id.trim().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace identifier is required",
        ));
    }
    if workspace.display_name.trim().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace display name is required",
        ));
    }
    if workspace.root_path.as_os_str().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace root is required",
        ));
    }
    Ok(())
}

fn validate_operation_decision(
    decision: &HaloWorkbenchOperationDecision,
) -> Result<(), HaloWorkbenchError> {
    match decision {
        HaloWorkbenchOperationDecision::AllowOnce | HaloWorkbenchOperationDecision::Deny => Ok(()),
    }
}

fn validate_user_input(content: &str) -> Result<(), HaloWorkbenchError> {
    if content.trim().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "Non-empty user input is required",
        ));
    }
    Ok(())
}

fn request_fingerprint(intent: &HaloWorkbenchIntent) -> Result<[u8; 32], HaloWorkbenchError> {
    let encoded = serde_json::to_vec(intent).map_err(|_| {
        HaloWorkbenchError::new(
            "runtime_internal",
            "The Workbench intent could not be fingerprinted",
            "retry",
        )
    })?;
    Ok(Sha256::digest(encoded).into())
}

fn valid_adapter_profile_summary(summary: &PiRpcAvailabilitySummary) -> bool {
    summary.version.profile == summary.version.version.compatibility_profile()
        && summary.version.evidence_source == PiRpcVersionEvidenceSource::LocalVersionProbe
        && summary.capabilities.required.as_slice() == PiRpcCapability::required_p0()
        && summary.capabilities.verified.is_empty()
}

fn valid_adapter_ready_summary(summary: &PiRpcAvailabilitySummary) -> bool {
    summary.version.profile == summary.version.version.compatibility_profile()
        && summary.version.evidence_source == PiRpcVersionEvidenceSource::LocalVersionProbe
        && summary.capabilities.required.as_slice() == PiRpcCapability::required_p0()
        && summary.capabilities.verified.as_slice()
            == PiRpcCapability::verified_by_readiness_handshake()
}

fn port_failure(kind: PortErrorKind) -> HaloWorkbenchError {
    match kind {
        PortErrorKind::Cancelled => HaloWorkbenchError::new(
            "adapter_cancelled",
            "The Workbench execution request was cancelled",
            "retry",
        ),
        PortErrorKind::Timeout => HaloWorkbenchError::new(
            "adapter_timeout",
            "The Workbench execution adapter timed out",
            "retry",
        ),
        PortErrorKind::PermissionDenied => HaloWorkbenchError::new(
            "adapter_access_denied",
            "The Workbench execution adapter was denied access",
            "review_system_permissions",
        ),
        _ => HaloWorkbenchError::new(
            "adapter_unavailable",
            "The Workbench execution adapter is unavailable",
            "retry",
        ),
    }
}

fn adapter_failure(reason: PiRpcFailureKind) -> HaloWorkbenchError {
    match reason {
        PiRpcFailureKind::NotInstalled => HaloWorkbenchError::new(
            "pi_not_installed",
            "Pi is not installed or cannot be resolved on PATH",
            "install_pi",
        ),
        PiRpcFailureKind::UnsupportedVersion => HaloWorkbenchError::new(
            "pi_version_unsupported",
            "The installed Pi version is not supported",
            "upgrade_pi",
        ),
        PiRpcFailureKind::CapabilityMismatch => HaloWorkbenchError::new(
            "pi_capability_mismatch",
            "The installed Pi RPC process lacks required capabilities",
            "upgrade_pi",
        ),
        PiRpcFailureKind::Authentication => HaloWorkbenchError::new(
            "pi_authentication_failed",
            "Pi provider authentication is unavailable",
            "configure_provider",
        ),
        PiRpcFailureKind::Transport => HaloWorkbenchError::new(
            "pi_transport_unavailable",
            "The Pi RPC child process is unavailable",
            "restart_runtime",
        ),
        PiRpcFailureKind::Protocol => HaloWorkbenchError::new(
            "pi_protocol_error",
            "The Pi RPC protocol is incompatible or malformed",
            "upgrade_pi",
        ),
        PiRpcFailureKind::Internal => HaloWorkbenchError::new(
            "pi_internal_error",
            "The Pi RPC adapter reported an internal failure",
            "restart_runtime",
        ),
    }
}
