//! Portable owner for the Halo Workbench Runtime public seam.
//!
//! The owner exposes Halo-local state and intent types. Pi RPC protocol and
//! process details remain behind [`PiRpcPort`].

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use crate::managed_event_facts::{
    normalize_summary, HaloFactId, HaloTaskId, ManagedEventFactInput, ManagedEventFactKind,
    ManagedEventFacts, ManagedEventFactsPortAdapter,
};

use halo_runtime_ports::{
    ClockPort, ManagedEventFactStorePort, PiProviderReadinessPort, PiRpcAvailabilitySummary,
    PiRpcCancellationMode, PiRpcCapability, PiRpcCommand, PiRpcEvent, PiRpcFailureKind,
    PiRpcOperationDecision, PiRpcOperationKind, PiRpcOperationRiskLevel, PiRpcPort, PiRpcReply,
    PiRpcSessionMode, PiRpcVersionEvidenceSource, PiRpcVersionSummary, PiRpcWorkspace,
    PortErrorKind, PortResult, WorkbenchDeliveryAttributionKind, WorkbenchDeliveryEvidence,
    WorkbenchDeliveryEvidencePort, WorkbenchDeliveryEvidenceRequest, WorkbenchDeliveryFingerprint,
    WorkbenchDeliveryFingerprintRequest, WorkbenchTaskBaseline, WorkbenchTaskBaselinePort,
    WorkbenchTaskBaselineRequest, WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest,
    WorkbenchWorkspaceTrustRequest, PI_RPC_ADAPTER_IDENTITY,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, watch, OnceCell};
use uuid::Uuid;

pub const HALO_WORKBENCH_SCHEMA_VERSION: u32 = 1;

const MAX_COMPLETED_REQUEST_RECORDS: usize = 256;
const MAX_COMPLETED_CLEANUP_RECORDS: usize = 64;
const MAX_SESSION_MESSAGES: usize = 64;
const MAX_SESSION_ACTIVITIES: usize = 128;
const MAX_BASELINE_CHANGED_FILES: usize = 4096;
const BASELINE_FINGERPRINT_HEX_LENGTH: usize = 64;
const MAX_PUBLIC_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_PUBLIC_LABEL_BYTES: usize = 128;
const MAX_DELIVERY_DIFF_BYTES: usize = 64 * 1024;
const MAX_DELIVERY_SUMMARY_BYTES: usize = 16 * 1024;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchTaskBaselineSnapshot {
    pub head: String,
    pub canonical_root: PathBuf,
    pub existing_changed_files: Vec<String>,
    pub working_tree_fingerprint: String,
    pub captured_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchMessageSnapshot {
    pub role: HaloWorkbenchMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchActivityKind {
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchActivityStatus {
    Started,
    Updated,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchActivitySnapshot {
    /// A Halo/adapter-redacted correlation value. This is never a raw Pi
    /// toolCallId or an entry/session identifier.
    pub activity_id: String,
    pub kind: HaloWorkbenchActivityKind,
    pub label: String,
    pub status: HaloWorkbenchActivityStatus,
    pub is_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchSessionPhase {
    Creating,
    Idle,
    Running,
    WaitingDeveloper,
    Reviewing,
    Interrupted,
    Stopping,
    Ended,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchCancellationMode {
    Native,
    Forced,
}

impl From<PiRpcCancellationMode> for HaloWorkbenchCancellationMode {
    fn from(mode: PiRpcCancellationMode) -> Self {
        match mode {
            PiRpcCancellationMode::Native => Self::Native,
            PiRpcCancellationMode::Forced => Self::Forced,
        }
    }
}

impl HaloWorkbenchSessionPhase {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Ended | Self::Failed)
    }

    fn needs_interruption_checkpoint(self) -> bool {
        !self.is_terminal() && self != Self::Interrupted
    }

    fn rejects_adapter_events(self) -> bool {
        self.is_terminal() || matches!(self, Self::Interrupted | Self::Reviewing)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchDeliveryDecision {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchDeliveryAttributionKind {
    ExistingUserModification,
    TaskModification,
    ManualIntervention,
}

impl From<WorkbenchDeliveryAttributionKind> for HaloWorkbenchDeliveryAttributionKind {
    fn from(kind: WorkbenchDeliveryAttributionKind) -> Self {
        match kind {
            WorkbenchDeliveryAttributionKind::ExistingUserModification => {
                Self::ExistingUserModification
            }
            WorkbenchDeliveryAttributionKind::TaskModification => Self::TaskModification,
            WorkbenchDeliveryAttributionKind::ManualIntervention => Self::ManualIntervention,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchDeliveryAttributionSnapshot {
    pub path: String,
    pub kind: HaloWorkbenchDeliveryAttributionKind,
}

/// Read-only, bounded, redacted delivery evidence exposed to the Workbench UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchDeliveryEvidenceSnapshot {
    pub captured_at_ms: i64,
    pub head: String,
    pub working_tree_fingerprint: String,
    pub changed_files: Vec<String>,
    pub diff_preview: String,
    pub attribution: Vec<HaloWorkbenchDeliveryAttributionSnapshot>,
}

/// Frozen delivery review state shown to the developer after an explicit
/// "finish and review". Contains no raw Pi identifiers, tool logs, credentials
/// or full conversation content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchDeliveryReviewSnapshot {
    pub evidence: HaloWorkbenchDeliveryEvidenceSnapshot,
    pub summary: String,
    pub verification_results: String,
    pub run_conclusion: String,
    pub decision: Option<HaloWorkbenchDeliveryDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchSessionSnapshot {
    /// The active Halo workspace that owns this session. This is a Halo-local
    /// binding, not a Pi session identifier.
    pub workspace_id: String,
    /// Stable Halo task identity. Standard sessions use it to select their
    /// adapter-owned persistent session directory.
    pub task_id: String,
    pub session_id: String,
    pub mode: HaloWorkbenchSessionMode,
    pub phase: HaloWorkbenchSessionPhase,
    pub cancellation_mode: Option<HaloWorkbenchCancellationMode>,
    pub baseline: Option<HaloWorkbenchTaskBaselineSnapshot>,
    pub messages: Vec<HaloWorkbenchMessageSnapshot>,
    pub activities: Vec<HaloWorkbenchActivitySnapshot>,
    pub error: Option<HaloWorkbenchError>,
    pub delivery_review: Option<HaloWorkbenchDeliveryReviewSnapshot>,
}

impl HaloWorkbenchSessionSnapshot {
    fn accepts_terminal_adapter_event(&self) -> bool {
        if !self.phase.rejects_adapter_events() {
            return true;
        }
        // A settled session remains exposed to a transport failure until its
        // review evidence is frozen. Reviews entered from an interruption
        // retain an error or cancellation fact and keep fencing late Pi events.
        self.phase == HaloWorkbenchSessionPhase::Reviewing
            && self.delivery_review.is_none()
            && self.error.is_none()
            && self.cancellation_mode.is_none()
    }
}

/// Durable boundary for the bounded, redacted facts that must be projected as
/// interrupted after an unexpected application loss. Implementations must
/// never persist Pi transport state, credentials, raw RPC identifiers, or
/// pending operations.
pub trait HaloWorkbenchInterruptionHistoryPort: Send + Sync {
    fn load_interrupted_sessions(&self) -> PortResult<Vec<HaloWorkbenchSessionSnapshot>>;

    fn replace_interrupted_sessions(
        &self,
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> PortResult<()>;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HaloWorkbenchOperationRiskLevel {
    Standard,
    HighRisk,
}

impl From<PiRpcOperationRiskLevel> for HaloWorkbenchOperationRiskLevel {
    fn from(level: PiRpcOperationRiskLevel) -> Self {
        match level {
            PiRpcOperationRiskLevel::Standard => Self::Standard,
            PiRpcOperationRiskLevel::HighRisk => Self::HighRisk,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchPendingOperationSnapshot {
    pub operation_id: String,
    pub task_id: String,
    pub session_id: String,
    pub kind: HaloWorkbenchOperationKind,
    pub phase: HaloWorkbenchPendingOperationPhase,
    pub tool_name: String,
    pub arguments: String,
    pub risk_level: HaloWorkbenchOperationRiskLevel,
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

    fn managed_event_facts_unavailable() -> Self {
        Self::new(
            "managed_event_facts_unavailable",
            "Managed event facts could not be recorded",
            "retry",
        )
    }

    fn interruption_history_unavailable() -> Self {
        Self::new(
            "interruption_history_unavailable",
            "The Workbench interruption history could not be restored",
            "restart_application",
        )
    }

    fn workspace_closed() -> Self {
        Self::new(
            "workspace_closed",
            "The Workbench workspace was closed before the task finished",
            "start_new_run_or_review_interruption",
        )
    }

    fn application_interrupted() -> Self {
        Self::new(
            "application_interrupted",
            "The Workbench application stopped before the task finished",
            "start_new_run_or_review_interruption",
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

    fn session_busy() -> Self {
        Self::new(
            "session_busy",
            "The requested Workbench session is already processing a lifecycle action",
            "wait_for_session_state",
        )
    }

    fn session_not_ready() -> Self {
        Self::new(
            "session_not_ready",
            "The requested Workbench session is not in the required state for this action",
            "wait_for_agent_settled",
        )
    }

    fn task_already_active() -> Self {
        Self::new(
            "task_already_active",
            "The requested Halo task already owns an active session in this workspace",
            "reuse_or_end_existing_session",
        )
    }

    fn managed_workspace_confirmation_required() -> Self {
        Self::new(
            "managed_workspace_confirmation_required",
            "Managed execution requires an explicit workspace confirmation",
            "confirm_managed_workspace",
        )
    }

    fn managed_workspace_not_git() -> Self {
        Self::new(
            "managed_workspace_not_git",
            "Managed execution requires a Git workspace",
            "choose_git_workspace",
        )
    }

    fn task_baseline_unavailable() -> Self {
        Self::new(
            "task_baseline_unavailable",
            "The managed task Git baseline could not be captured",
            "retry",
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

    fn delivery_review_not_ready() -> Self {
        Self::new(
            "delivery_review_not_ready",
            "The Workbench session is not ready for delivery review",
            "wait_for_agent_settled",
        )
    }

    fn delivery_evidence_unavailable() -> Self {
        Self::new(
            "delivery_evidence_unavailable",
            "The managed delivery evidence could not be captured",
            "retry",
        )
    }

    fn delivery_decision_not_ready() -> Self {
        Self::new(
            "delivery_decision_not_ready",
            "The delivery decision is not available for this Workbench session",
            "refresh_runtime_snapshot",
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
    SessionMessageUpdated,
    SessionActivityUpdated,
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
    ConfirmManagedWorkspace {
        workspace_id: String,
        root_path: PathBuf,
    },
    CreateSession {
        task_id: String,
        mode: HaloWorkbenchSessionMode,
    },
    SendUserInput {
        session_id: String,
        content: String,
    },
    FollowUp {
        session_id: String,
        content: String,
    },
    StopSession {
        session_id: String,
    },
    AbortSession {
        session_id: String,
    },
    EndSession {
        session_id: String,
    },
    ResolveOperation {
        operation_id: String,
        decision: HaloWorkbenchOperationDecision,
    },
    FinishAndReview {
        session_id: String,
    },
    AcceptDelivery {
        session_id: String,
    },
    RejectDelivery {
        session_id: String,
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
            Self::ConfirmManagedWorkspace {
                workspace_id,
                root_path: _,
            } => formatter
                .debug_struct("ConfirmManagedWorkspace")
                .field("workspace_id", workspace_id)
                .field("root_path", &"<redacted>")
                .finish(),
            Self::CreateSession { task_id, mode } => formatter
                .debug_struct("CreateSession")
                .field("task_id", task_id)
                .field("mode", mode)
                .finish(),
            Self::SendUserInput { session_id, .. } => formatter
                .debug_struct("SendUserInput")
                .field("session_id", session_id)
                .field("content", &"<redacted>")
                .finish(),
            Self::FollowUp { session_id, .. } => formatter
                .debug_struct("FollowUp")
                .field("session_id", session_id)
                .field("content", &"<redacted>")
                .finish(),
            Self::StopSession { session_id } => formatter
                .debug_struct("StopSession")
                .field("session_id", session_id)
                .finish(),
            Self::AbortSession { session_id } => formatter
                .debug_struct("AbortSession")
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
            Self::FinishAndReview { session_id } => formatter
                .debug_struct("FinishAndReview")
                .field("session_id", session_id)
                .finish(),
            Self::AcceptDelivery { session_id } => formatter
                .debug_struct("AcceptDelivery")
                .field("session_id", session_id)
                .finish(),
            Self::RejectDelivery { session_id } => formatter
                .debug_struct("RejectDelivery")
                .field("session_id", session_id)
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
    settled_fingerprints: BTreeMap<String, watch::Receiver<Option<WorkbenchDeliveryFingerprint>>>,
    error: Option<HaloWorkbenchError>,
    sequence: u64,
    state_version: u64,
    generation: u64,
    adapter_generation: Option<u64>,
    managed_workspace_confirmation: Option<ManagedWorkspaceConfirmation>,
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
            settled_fingerprints: BTreeMap::new(),
            error: None,
            sequence: 0,
            state_version: 0,
            generation: 0,
            adapter_generation: None,
            managed_workspace_confirmation: None,
            cleanup_started: HashSet::new(),
            terminated: false,
        }
    }
}

impl RuntimeState {
    fn from_interruption_history(
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> Result<Self, HaloWorkbenchError> {
        let mut state = Self::default();
        for session in sessions {
            if session.workspace_id.is_empty()
                || session.task_id.is_empty()
                || session.session_id.is_empty()
                || session.mode != HaloWorkbenchSessionMode::Managed
                || session.phase != HaloWorkbenchSessionPhase::Interrupted
                || !session.messages.is_empty()
                || !session.activities.is_empty()
            {
                return Err(HaloWorkbenchError::interruption_history_unavailable());
            }
            if state
                .sessions
                .insert(session.session_id.clone(), session)
                .is_some()
            {
                return Err(HaloWorkbenchError::interruption_history_unavailable());
            }
        }
        Ok(state)
    }
}

struct InterruptionHistoryState {
    persisted_sessions: Vec<HaloWorkbenchSessionSnapshot>,
    // State is snapshotted before persistence so adapters cannot block the
    // runtime lock. This high-water mark prevents a delayed old snapshot from
    // overwriting a later interruption fact.
    last_observed_state_version: u64,
}

impl InterruptionHistoryState {
    fn new(persisted_sessions: Vec<HaloWorkbenchSessionSnapshot>) -> Self {
        Self {
            persisted_sessions,
            last_observed_state_version: 0,
        }
    }

    fn should_persist(
        &mut self,
        state_version: u64,
        sessions: &[HaloWorkbenchSessionSnapshot],
    ) -> bool {
        if state_version < self.last_observed_state_version {
            return false;
        }
        self.last_observed_state_version = state_version;
        self.persisted_sessions.as_slice() != sessions
    }

    fn mark_persisted(&mut self, sessions: Vec<HaloWorkbenchSessionSnapshot>) {
        self.persisted_sessions = sessions;
    }
}

struct HaloWorkbenchRuntimeInner {
    adapter: Arc<dyn PiRpcPort>,
    workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
    task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
    delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
    interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
    provider_readiness: Arc<dyn PiProviderReadinessPort>,
    clock: Arc<dyn ClockPort>,
    managed_event_facts: Mutex<Option<Arc<dyn ManagedEventFacts>>>,
    state: Mutex<RuntimeState>,
    interruption_history_state: Mutex<InterruptionHistoryState>,
    requests: tokio::sync::Mutex<RequestLedger>,
    cleanups: tokio::sync::Mutex<HashMap<u64, CleanupRecord>>,
    lifecycle_actions: tokio::sync::Mutex<()>,
    adapter_actions: tokio::sync::Mutex<()>,
    prompt_actions: tokio::sync::Mutex<()>,
    events: broadcast::Sender<HaloWorkbenchEvent>,
    adapter_events_started: AtomicBool,
    shutdown_result: OnceCell<Result<(), HaloWorkbenchError>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedWorkspaceConfirmation {
    generation: u64,
    workspace_id: String,
    canonical_root: PathBuf,
}

struct UnavailableTaskBaselinePort;

#[async_trait::async_trait]
impl WorkbenchTaskBaselinePort for UnavailableTaskBaselinePort {
    async fn capture(
        &self,
        _request: WorkbenchTaskBaselineRequest,
    ) -> halo_runtime_ports::PortResult<WorkbenchTaskBaseline> {
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "managed task baseline provider is unavailable",
        ))
    }
}

struct UnavailableDeliveryEvidencePort;

#[async_trait::async_trait]
impl WorkbenchDeliveryEvidencePort for UnavailableDeliveryEvidencePort {
    async fn capture(
        &self,
        _request: WorkbenchDeliveryEvidenceRequest,
    ) -> halo_runtime_ports::PortResult<WorkbenchDeliveryEvidence> {
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "managed delivery evidence provider is unavailable",
        ))
    }

    async fn capture_fingerprint(
        &self,
        _request: WorkbenchDeliveryFingerprintRequest,
    ) -> halo_runtime_ports::PortResult<WorkbenchDeliveryFingerprint> {
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "managed delivery evidence provider is unavailable",
        ))
    }
}

struct EmptyInterruptionHistoryPort;

impl HaloWorkbenchInterruptionHistoryPort for EmptyInterruptionHistoryPort {
    fn load_interrupted_sessions(&self) -> PortResult<Vec<HaloWorkbenchSessionSnapshot>> {
        Ok(Vec::new())
    }

    fn replace_interrupted_sessions(
        &self,
        _sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> PortResult<()> {
        Ok(())
    }
}

impl HaloWorkbenchRuntimeInner {
    fn persist_interruption_history(
        &self,
        state_version: u64,
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) {
        let mut history = self
            .interruption_history_state
            .lock()
            .expect("Halo Workbench interruption history lock");
        if !history.should_persist(state_version, &sessions) {
            return;
        }
        if let Err(error) = self
            .interruption_history
            .replace_interrupted_sessions(sessions.clone())
        {
            log::warn!(
                "Halo Workbench interruption history persistence failed: operation=replace_interrupted_sessions session_count={} error={error}",
                sessions.len()
            );
            return;
        }
        history.mark_persisted(sessions);
    }

    fn install_managed_event_fact_store(&self, port: Arc<dyn ManagedEventFactStorePort>) {
        let mut store = self
            .managed_event_facts
            .lock()
            .expect("Halo Workbench managed event facts lock");
        *store = Some(Arc::new(ManagedEventFactsPortAdapter::new(port)));
    }

    fn append_managed_task_fact(
        &self,
        task_id: &str,
        kind: ManagedEventFactKind,
        summary: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let store = self
            .managed_event_facts
            .lock()
            .expect("Halo Workbench managed event facts lock")
            .clone();
        let Some(store) = store else {
            return Ok(());
        };
        store
            .append(ManagedEventFactInput {
                fact_id: HaloFactId::from_runtime(Uuid::new_v4().to_string()),
                task_id: HaloTaskId::from_runtime(task_id.to_string()),
                recorded_at_ms: self.clock.now_unix_millis(),
                schema_version: crate::managed_event_facts::MANAGED_EVENT_FACT_SCHEMA_VERSION,
                kind,
                redacted_summary: normalize_summary(summary)
                    .map_err(|_| HaloWorkbenchError::managed_event_facts_unavailable())?,
            })
            .map(|_| ())
            .map_err(|_| HaloWorkbenchError::managed_event_facts_unavailable())
    }

    fn expose_error(&self, error: HaloWorkbenchError) {
        self.state.lock().expect("Halo Workbench state lock").error = Some(error);
    }

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
        let (event, interrupted_sessions) = {
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
            let interrupted_sessions = interruption_history_snapshots(&state);
            (event, interrupted_sessions)
        };
        self.persist_interruption_history(event.state_version, interrupted_sessions);
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
                self.capture_settled_fingerprint(generation, &session_id);
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::WaitingDeveloper,
                    "Workbench session is waiting for developer",
                );
            }
            PiRpcEvent::SessionStopped {
                session_id,
                cancellation_mode,
                ..
            } => {
                self.set_session_interrupted(generation, &session_id, cancellation_mode.into());
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
                self.set_adapter_session_ended(generation, &session_id);
            }
            PiRpcEvent::SessionFailed {
                session_id, reason, ..
            } => {
                let phase = self
                    .state
                    .lock()
                    .expect("Halo Workbench state lock")
                    .sessions
                    .get(&session_id)
                    .filter(|session| session.mode == HaloWorkbenchSessionMode::Managed)
                    .map(|_| HaloWorkbenchSessionPhase::Interrupted)
                    .unwrap_or(HaloWorkbenchSessionPhase::Failed);
                self.set_session_failure(generation, &session_id, adapter_failure(reason), phase);
            }
            PiRpcEvent::MessageUpdated {
                session_id, text, ..
            } => {
                self.append_assistant_message(generation, &session_id, text);
            }
            PiRpcEvent::ToolExecutionStarted {
                session_id,
                redacted_tool_call_id,
                tool_name,
                ..
            } => {
                self.update_tool_activity(
                    generation,
                    &session_id,
                    redacted_tool_call_id,
                    tool_name,
                    HaloWorkbenchActivityStatus::Started,
                    false,
                );
            }
            PiRpcEvent::ToolExecutionUpdated {
                session_id,
                redacted_tool_call_id,
                tool_name,
                ..
            } => {
                self.update_tool_activity(
                    generation,
                    &session_id,
                    redacted_tool_call_id,
                    tool_name,
                    HaloWorkbenchActivityStatus::Updated,
                    false,
                );
            }
            PiRpcEvent::ToolExecutionEnded {
                session_id,
                redacted_tool_call_id,
                tool_name,
                is_error,
                ..
            } => {
                self.update_tool_activity(
                    generation,
                    &session_id,
                    redacted_tool_call_id,
                    tool_name,
                    if is_error {
                        HaloWorkbenchActivityStatus::Failed
                    } else {
                        HaloWorkbenchActivityStatus::Completed
                    },
                    is_error,
                );
            }
            PiRpcEvent::OperationRequested {
                session_id,
                operation_id,
                kind,
                summary,
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
                                .is_none_or(|session| session.phase.rejects_adapter_events())
                            || state.pending_operations.contains_key(&operation_id)
                        {
                            return false;
                        }
                        let Some(session) = state.sessions.get(&session_id) else {
                            return false;
                        };
                        state.pending_operations.insert(
                            operation_id.clone(),
                            HaloWorkbenchPendingOperationSnapshot {
                                operation_id,
                                task_id: session.task_id.clone(),
                                session_id,
                                kind: kind.into(),
                                phase: HaloWorkbenchPendingOperationPhase::AwaitingDecision,
                                tool_name: summary.tool_name,
                                arguments: summary.arguments,
                                risk_level: summary.risk_level.into(),
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

    fn append_assistant_message(&self, generation: u64, session_id: &str, text: String) {
        let text = redact_halo_text(&text, MAX_PUBLIC_MESSAGE_BYTES);
        if text.is_empty() {
            return;
        }
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionMessageUpdated,
            "Workbench assistant message was updated",
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Running {
                    return false;
                }
                append_message(
                    &mut session.messages,
                    HaloWorkbenchMessageRole::Assistant,
                    text,
                );
                true
            },
        );
    }

    fn update_tool_activity(
        &self,
        generation: u64,
        session_id: &str,
        activity_id: String,
        label: String,
        status: HaloWorkbenchActivityStatus,
        is_error: bool,
    ) {
        let Some(activity_id) = opaque_public_activity_id(&activity_id) else {
            return;
        };
        let label = redact_halo_text(&label, MAX_PUBLIC_LABEL_BYTES);
        let Some(label) = bounded_public_label(&label, MAX_PUBLIC_LABEL_BYTES) else {
            return;
        };
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionActivityUpdated,
            "Workbench tool activity was updated",
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Running {
                    return false;
                }
                if let Some(activity) = session
                    .activities
                    .iter_mut()
                    .find(|activity| activity.activity_id == activity_id)
                {
                    activity.label = label;
                    activity.status = status;
                    activity.is_error = is_error;
                    return true;
                }
                if session.activities.len() >= MAX_SESSION_ACTIVITIES {
                    session.activities.remove(0);
                }
                session.activities.push(HaloWorkbenchActivitySnapshot {
                    activity_id,
                    kind: HaloWorkbenchActivityKind::Tool,
                    label,
                    status,
                    is_error,
                });
                true
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
                session.error = None;
                if phase != HaloWorkbenchSessionPhase::Interrupted {
                    session.cancellation_mode = None;
                }
                if phase.is_terminal() {
                    state
                        .pending_operations
                        .retain(|_, operation| operation.session_id != owned_session_id);
                }
                true
            },
        );
    }

    fn set_session_interrupted(
        &self,
        generation: u64,
        session_id: &str,
        cancellation_mode: HaloWorkbenchCancellationMode,
    ) {
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session was interrupted",
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if !session.accepts_terminal_adapter_event()
                    || !valid_session_transition(
                        session.phase,
                        HaloWorkbenchSessionPhase::Interrupted,
                    )
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Interrupted;
                session.error = None;
                session.cancellation_mode = Some(cancellation_mode);
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != owned_session_id);
                true
            },
        );
    }

    fn set_session_failure(
        &self,
        generation: u64,
        session_id: &str,
        error: HaloWorkbenchError,
        phase: HaloWorkbenchSessionPhase,
    ) {
        let owned_session_id = session_id.to_string();
        let summary = match phase {
            HaloWorkbenchSessionPhase::Interrupted => "Workbench session was interrupted",
            _ => "Workbench session failed",
        };
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
                if !session.accepts_terminal_adapter_event()
                    || !valid_session_transition(session.phase, phase)
                {
                    return false;
                }
                session.phase = phase;
                session.error = Some(error);
                session.cancellation_mode = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != owned_session_id);
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
                interrupt_managed_sessions(state, &error);
                state.phase = HaloWorkbenchPhase::Failed;
                state.adapter_available = false;
                state.error = Some(error);
                true
            },
        )
    }

    async fn fail_adapter_event_gap(self: &Arc<Self>) {
        self.fail_active_runtime(
            HaloWorkbenchError::new(
                "adapter_event_gap",
                "The Workbench execution event stream has a gap",
                "restart_runtime",
            ),
            "Workbench Runtime event stream failed",
        )
        .await;
    }

    async fn fail_adapter_event_stream_closed(self: &Arc<Self>) {
        self.fail_active_runtime(
            HaloWorkbenchError::new(
                "adapter_event_stream_closed",
                "The Workbench execution event stream closed unexpectedly",
                "restart_runtime",
            ),
            "Workbench Runtime event stream failed",
        )
        .await;
    }

    async fn fail_active_runtime(
        self: &Arc<Self>,
        error: HaloWorkbenchError,
        summary: &'static str,
    ) {
        let transitioned = self.publish_transition(
            None,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            summary,
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
                interrupt_managed_sessions(state, &error);
                state.phase = HaloWorkbenchPhase::Failed;
                state.adapter_available = false;
                state.error = Some(error);
                true
            },
        );
        let cleanup_generation = transitioned.then(|| {
            self.state
                .lock()
                .expect("Halo Workbench state lock")
                .adapter_generation
        });
        if let Some(generation) = cleanup_generation.flatten() {
            if let Err(error) = self.execute_cleanup_once(generation).await {
                log::warn!(
                    "Halo Workbench Runtime cleanup failed: operation=shutdown generation={generation} error={error}"
                );
            }
        }
    }

    fn capture_settled_fingerprint(&self, generation: u64, session_id: &str) {
        let session_id_owned = session_id.to_string();
        let request = {
            let state = self.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return;
            }
            let Some(session) = state.sessions.get(&session_id_owned) else {
                return;
            };
            if session.mode != HaloWorkbenchSessionMode::Managed {
                return;
            }
            let Some(workspace) = state.workspace.as_ref() else {
                return;
            };
            WorkbenchDeliveryFingerprintRequest {
                workspace_id: workspace.workspace_id.clone(),
                canonical_root: workspace.root_path.clone(),
            }
        };
        let (sender, receiver) = watch::channel(None);
        {
            let mut state = self.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation {
                return;
            }
            state
                .settled_fingerprints
                .insert(session_id_owned, receiver);
        }
        let port = self.delivery_evidence.clone();
        tokio::spawn(async move {
            let fingerprint = port.capture_fingerprint(request).await.ok();
            sender.send_replace(fingerprint);
        });
    }

    fn set_adapter_session_ended(&self, generation: u64, session_id: &str) {
        let session_id_owned = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session ended",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                // A finished managed task remains in read-only delivery review
                // until the developer accepts or rejects the result.
                if session.phase == HaloWorkbenchSessionPhase::Reviewing
                    || session.phase == HaloWorkbenchSessionPhase::Interrupted
                    || session.phase.is_terminal()
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Ended;
                session.error = None;
                session.cancellation_mode = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != session_id_owned);
                true
            },
        );
    }

    async fn execute_cleanup_once(self: &Arc<Self>, generation: u64) -> CleanupResult {
        let mut result = {
            let mut cleanups = self.cleanups.lock().await;
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
                    self.state
                        .lock()
                        .expect("Halo Workbench state lock")
                        .cleanup_started
                        .insert(generation);
                    let inner = Arc::clone(self);
                    tokio::spawn(async move {
                        let cleanup_result = {
                            let _action = inner.adapter_actions.lock().await;
                            match inner
                                .adapter
                                .execute(PiRpcCommand::Shutdown { generation })
                                .await
                            {
                                Ok(PiRpcReply::Accepted)
                                | Ok(PiRpcReply::Available { .. })
                                | Ok(PiRpcReply::Ready { .. }) => Ok(()),
                                Ok(PiRpcReply::Unavailable { .. }) => Err(HaloWorkbenchError::new(
                                    "cleanup_failed",
                                    "Workbench Runtime cleanup did not complete",
                                    "restart_application",
                                )),
                                Err(error) => Err(port_failure(error.kind)),
                            }
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
        Self::new_with_task_baseline(
            adapter,
            workspace_facts,
            provider_readiness,
            Arc::new(UnavailableTaskBaselinePort),
            clock,
        )
    }

    /// Constructs the runtime with the read-only Git baseline provider used
    /// by managed task creation. The compatibility `new` constructor remains
    /// available for standard-only callers and contract fakes.
    pub fn new_with_task_baseline(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self::new_with_delivery_evidence(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            Arc::new(UnavailableDeliveryEvidencePort),
            clock,
        )
    }

    /// Constructs the runtime with both the Git baseline provider and the
    /// read-only delivery evidence provider used by managed tasks.
    pub fn new_with_delivery_evidence(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self::try_new_with_delivery_evidence_and_interruption_history(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            delivery_evidence,
            Arc::new(EmptyInterruptionHistoryPort),
            clock,
        )
        .expect("the empty interruption history port is infallible")
    }

    /// Constructs the runtime with an injected durable managed-facts store.
    pub fn new_with_delivery_evidence_and_fact_store(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        fact_store: Arc<dyn ManagedEventFactStorePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        let runtime = Self::new_with_delivery_evidence(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            delivery_evidence,
            clock,
        );
        runtime.inner.install_managed_event_fact_store(fact_store);
        runtime
    }

    /// Restores the safe Halo snapshot while attaching the durable facts store.
    /// Facts are read for schema/record validation only; they are never replayed
    /// into Pi or treated as executable operations during recovery.
    pub fn try_new_with_delivery_evidence_and_fact_store_and_interruption_history(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        fact_store: Arc<dyn ManagedEventFactStorePort>,
        interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, HaloWorkbenchError> {
        let restored_sessions = interruption_history
            .load_interrupted_sessions()
            .map_err(|_| HaloWorkbenchError::interruption_history_unavailable())?;
        for session in &restored_sessions {
            fact_store
                .read_task(&session.task_id)
                .map_err(|_| HaloWorkbenchError::managed_event_facts_unavailable())?;
        }
        let runtime = Self::try_new_with_delivery_evidence_and_interruption_history(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            delivery_evidence,
            interruption_history,
            clock,
        )?;
        runtime.inner.install_managed_event_fact_store(fact_store);
        Ok(runtime)
    }

    /// Constructs the runtime with the durable, redacted interruption facts
    /// that are safe to surface after an application restart. This boundary
    /// deliberately excludes native Pi session state and pending operations.
    pub fn try_new_with_delivery_evidence_and_interruption_history(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, HaloWorkbenchError> {
        let restored_interruption_history = interruption_history
            .load_interrupted_sessions()
            .map_err(|_| HaloWorkbenchError::interruption_history_unavailable())?;
        let state = RuntimeState::from_interruption_history(restored_interruption_history.clone())?;
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            inner: Arc::new(HaloWorkbenchRuntimeInner {
                adapter,
                workspace_facts,
                task_baseline,
                delivery_evidence,
                interruption_history,
                provider_readiness,
                clock,
                managed_event_facts: Mutex::new(None),
                state: Mutex::new(state),
                interruption_history_state: Mutex::new(InterruptionHistoryState::new(
                    restored_interruption_history,
                )),
                requests: tokio::sync::Mutex::new(RequestLedger::default()),
                cleanups: tokio::sync::Mutex::new(HashMap::new()),
                lifecycle_actions: tokio::sync::Mutex::new(()),
                adapter_actions: tokio::sync::Mutex::new(()),
                prompt_actions: tokio::sync::Mutex::new(()),
                events,
                adapter_events_started: AtomicBool::new(false),
                shutdown_result: OnceCell::new(),
            }),
        })
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
                Err(_) => {
                    let error = HaloWorkbenchError::new(
                        "runtime_internal",
                        "The Workbench request execution stopped unexpectedly",
                        "restart_application",
                    );
                    runtime
                        .inner
                        .fail_active_runtime(
                            error.clone(),
                            "Workbench Runtime request execution stopped unexpectedly",
                        )
                        .await;
                    Err(error)
                }
            };
            if let Err(error) = &result {
                runtime.inner.expose_error(error.clone());
            }
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
                        inner.fail_adapter_event_gap().await;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.fail_adapter_event_stream_closed().await;
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
            HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id,
                root_path,
            } => {
                self.confirm_managed_workspace(request_id, workspace_id, root_path)
                    .await
            }
            HaloWorkbenchIntent::CreateSession { task_id, mode } => {
                self.create_session(request_id, task_id, mode).await
            }
            HaloWorkbenchIntent::SendUserInput {
                session_id,
                content,
            } => {
                self.session_command(request_id, &session_id, SessionIntent::Prompt(content))
                    .await
            }
            HaloWorkbenchIntent::FollowUp {
                session_id,
                content,
            } => {
                self.session_command(request_id, &session_id, SessionIntent::FollowUp(content))
                    .await
            }
            HaloWorkbenchIntent::StopSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::Abort)
                    .await
            }
            HaloWorkbenchIntent::AbortSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::Abort)
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
            HaloWorkbenchIntent::FinishAndReview { session_id } => {
                self.finish_and_review(request_id, &session_id).await
            }
            HaloWorkbenchIntent::AcceptDelivery { session_id } => {
                self.resolve_delivery(
                    request_id,
                    &session_id,
                    HaloWorkbenchDeliveryDecision::Accepted,
                )
                .await
            }
            HaloWorkbenchIntent::RejectDelivery { session_id } => {
                self.resolve_delivery(
                    request_id,
                    &session_id,
                    HaloWorkbenchDeliveryDecision::Rejected,
                )
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
            interrupt_managed_sessions(&mut state, &HaloWorkbenchError::workspace_closed());
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
                state.managed_workspace_confirmation = None;
                retain_managed_interruption_facts(state);
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
            let close_error = if terminate {
                HaloWorkbenchError::runtime_shutdown()
            } else {
                HaloWorkbenchError::workspace_closed()
            };
            interrupt_managed_sessions(&mut state, &close_error);
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
                            && state.pending_operations.is_empty()
                            && state.error.is_none())
                    {
                        return false;
                    }
                    state.phase = HaloWorkbenchPhase::Disconnected;
                    state.adapter_available = false;
                    state.adapter_readiness = None;
                    state.managed_workspace_confirmation = None;
                    state.workspace = None;
                    retain_managed_interruption_facts(state);
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
        let result = self.inner.execute_cleanup_once(cleanup_generation).await;
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
                state.managed_workspace_confirmation = None;
                if state.adapter_generation == Some(cleanup_generation) {
                    state.adapter_generation = None;
                }
                state.workspace = None;
                retain_managed_interruption_facts(state);
                state.pending_operations.clear();
                state.error = None;
                true
            },
        );
        Ok(())
    }

    async fn confirm_managed_workspace(
        &self,
        request_id: &str,
        workspace_id: String,
        root_path: PathBuf,
    ) -> IntentResult {
        validate_workspace_confirmation(&workspace_id, &root_path)?;
        let generation = self.ready_generation()?;
        let expected_root = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            if workspace.workspace_id != workspace_id || workspace.root_path != root_path {
                return Err(HaloWorkbenchError::new(
                    "workspace_identity_mismatch",
                    "The confirmed workspace does not match the active canonical workspace",
                    "refresh_workspace",
                ));
            }
            workspace.root_path.clone()
        };

        let facts = self
            .inner
            .workspace_facts
            .confirm_managed_trust(WorkbenchWorkspaceTrustRequest {
                workspace_id: workspace_id.clone(),
                root: root_path,
            })
            .await
            .map_err(|_| {
                HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace trust could not be confirmed",
                    "retry",
                )
            })?;
        if facts.workspace_id != workspace_id || facts.canonical_root != expected_root {
            return Err(HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "Workspace identity verification failed",
                "refresh_workspace",
            ));
        }
        if !facts.git_repository {
            return Err(HaloWorkbenchError::managed_workspace_not_git());
        }
        if !facts.trusted {
            return Err(HaloWorkbenchError::new(
                "workspace_untrusted",
                "The workspace owner did not confirm managed execution",
                "confirm_managed_workspace",
            ));
        }

        let confirmation = ManagedWorkspaceConfirmation {
            generation,
            workspace_id: workspace_id.clone(),
            canonical_root: expected_root,
        };
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::WorkspaceChanged,
            "Workspace trust was explicitly confirmed for managed execution",
            None,
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(workspace) = state.workspace.as_mut() else {
                    return false;
                };
                if workspace.workspace_id != workspace_id
                    || workspace.root_path != confirmation.canonical_root
                {
                    return false;
                }
                workspace.trusted = true;
                state.managed_workspace_confirmation = Some(confirmation);
                true
            },
        );
        Ok(self.inner.receipt(request_id, None))
    }

    async fn create_session(
        &self,
        request_id: &str,
        task_id: String,
        mode: HaloWorkbenchSessionMode,
    ) -> IntentResult {
        validate_task_id(&task_id)?;
        let session_id = Uuid::new_v4().to_string();
        let generation = self.ready_generation()?;
        let workspace_id = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state
                .workspace
                .as_ref()
                .map(|workspace| workspace.workspace_id.clone())
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?
        };
        if mode == HaloWorkbenchSessionMode::Managed {
            self.ensure_managed_workspace_confirmed(generation).await?;
        }
        let event_session_id = session_id.clone();
        let state_session_id = session_id.clone();
        let state_task_id = task_id.clone();
        let state_workspace_id = workspace_id.clone();
        if mode == HaloWorkbenchSessionMode::Managed {
            self.inner.append_managed_task_fact(
                &task_id,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task session is being created",
            )?;
        }
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
                if state.sessions.values().any(|session| {
                    session.workspace_id == state_workspace_id
                        && session.task_id == state_task_id
                        && !session.phase.is_terminal()
                }) {
                    return false;
                }
                state.sessions.insert(
                    state_session_id.clone(),
                    HaloWorkbenchSessionSnapshot {
                        workspace_id: state_workspace_id,
                        task_id: state_task_id,
                        session_id: state_session_id,
                        mode,
                        phase: HaloWorkbenchSessionPhase::Creating,
                        cancellation_mode: None,
                        baseline: None,
                        messages: Vec::new(),
                        activities: Vec::new(),
                        error: None,
                        delivery_review: None,
                    },
                );
                true
            },
        ) {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation == generation
                && state.sessions.values().any(|session| {
                    session.workspace_id == workspace_id
                        && session.task_id == task_id
                        && !session.phase.is_terminal()
                })
            {
                return Err(HaloWorkbenchError::task_already_active());
            }
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        if mode == HaloWorkbenchSessionMode::Managed {
            let baseline = match self.capture_managed_task_baseline(generation).await {
                Ok(baseline) => baseline,
                Err(error) => {
                    self.fail_session_before_adapter(
                        generation,
                        request_id,
                        &session_id,
                        error.clone(),
                    );
                    return Err(error);
                }
            };
            if !self.attach_session_baseline(generation, &session_id, baseline) {
                return Err(HaloWorkbenchError::session_not_found());
            }
        }
        let result = self
            .execute_session_adapter_action(
                generation,
                &task_id,
                &session_id,
                PiRpcCommand::CreateSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                    mode: mode.into(),
                },
                false,
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

    async fn ensure_managed_workspace_confirmed(
        &self,
        generation: u64,
    ) -> Result<(), HaloWorkbenchError> {
        let confirmation = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state.managed_workspace_confirmation.clone()
        };
        let Some(confirmation) = confirmation else {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        };
        if confirmation.generation != generation {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        }
        let request = WorkbenchWorkspaceFactsRequest {
            workspace_id: confirmation.workspace_id.clone(),
            root: confirmation.canonical_root.clone(),
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
        if facts.workspace_id != request.workspace_id
            || facts.canonical_root != request.root
            || !facts.git_repository
        {
            return Err(HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "The managed workspace changed after confirmation",
                "refresh_workspace",
            ));
        }
        if !facts.trusted {
            return Err(HaloWorkbenchError::new(
                "workspace_untrusted",
                "Managed workspace trust is no longer active",
                "confirm_managed_workspace",
            ));
        }
        Ok(())
    }

    async fn capture_managed_task_baseline(
        &self,
        generation: u64,
    ) -> Result<HaloWorkbenchTaskBaselineSnapshot, HaloWorkbenchError> {
        self.ensure_managed_workspace_confirmed(generation).await?;
        let request = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            WorkbenchTaskBaselineRequest {
                workspace_id: workspace.workspace_id.clone(),
                canonical_root: workspace.root_path.clone(),
            }
        };
        let baseline = self
            .inner
            .task_baseline
            .capture(request.clone())
            .await
            .map_err(|_| HaloWorkbenchError::task_baseline_unavailable())?;
        validate_task_baseline(&baseline)
            .map_err(|_| HaloWorkbenchError::task_baseline_unavailable())?;
        if baseline.canonical_root != request.canonical_root {
            return Err(HaloWorkbenchError::task_baseline_unavailable());
        }
        Ok(HaloWorkbenchTaskBaselineSnapshot {
            head: baseline.head,
            canonical_root: baseline.canonical_root,
            existing_changed_files: baseline.existing_changed_files,
            working_tree_fingerprint: baseline.working_tree_fingerprint,
            captured_at_ms: baseline.captured_at_ms,
        })
    }

    fn attach_session_baseline(
        &self,
        generation: u64,
        session_id: &str,
        baseline: HaloWorkbenchTaskBaselineSnapshot,
    ) -> bool {
        let session_id = session_id.to_string();
        self.inner.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            "Managed task Git baseline was captured",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.mode != HaloWorkbenchSessionMode::Managed
                    || session.phase != HaloWorkbenchSessionPhase::Creating
                {
                    return false;
                }
                session.baseline = Some(baseline);
                true
            },
        )
    }

    fn fail_session_before_adapter(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        error: HaloWorkbenchError,
    ) {
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
                session.phase = HaloWorkbenchSessionPhase::Failed;
                session.error = Some(error);
                true
            },
        );
    }

    async fn session_command(
        &self,
        request_id: &str,
        session_id: &str,
        intent: SessionIntent,
    ) -> IntentResult {
        if let SessionIntent::Prompt(content) | SessionIntent::FollowUp(content) = &intent {
            validate_user_input(content)?;
        }
        let generation = self.ready_generation()?;
        self.ensure_session_action_allowed(generation, session_id, &intent)?;
        let task_id = self.session_task_id(generation, session_id)?;
        let facts_managed = self.session_requires_managed_trust(generation, session_id)?;
        let allow_session_removal = matches!(&intent, SessionIntent::End);
        let command = match intent {
            SessionIntent::Prompt(content) => {
                if facts_managed {
                    self.inner.append_managed_task_fact(
                        &task_id,
                        ManagedEventFactKind::UserMessageSummary,
                        "Managed user message received",
                    )?;
                }
                self.append_user_message(generation, session_id, &content)?;
                self.mark_session_running(
                    generation,
                    request_id,
                    session_id,
                    HaloWorkbenchSessionPhase::Idle,
                )?;
                PiRpcCommand::SendUserInput {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                    content,
                }
            }
            SessionIntent::FollowUp(content) => {
                if facts_managed {
                    self.inner.append_managed_task_fact(
                        &task_id,
                        ManagedEventFactKind::UserMessageSummary,
                        "Managed follow-up message received",
                    )?;
                }
                self.append_user_message(generation, session_id, &content)?;
                self.mark_session_running(
                    generation,
                    request_id,
                    session_id,
                    HaloWorkbenchSessionPhase::WaitingDeveloper,
                )?;
                PiRpcCommand::FollowUp {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                    content,
                }
            }
            SessionIntent::Abort => {
                self.mark_session_stopping(
                    generation,
                    request_id,
                    session_id,
                    SessionIntent::Abort,
                )?;
                PiRpcCommand::AbortSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                }
            }
            SessionIntent::End => {
                self.mark_session_stopping(generation, request_id, session_id, SessionIntent::End)?;
                PiRpcCommand::EndSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                }
            }
        };
        let result = self
            .execute_session_adapter_action(
                generation,
                &task_id,
                session_id,
                command,
                allow_session_removal,
            )
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

    fn append_user_message(
        &self,
        generation: u64,
        session_id: &str,
        content: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let session_id = session_id.to_string();
        let content = redact_halo_text(content, MAX_PUBLIC_MESSAGE_BYTES);
        if content.trim().is_empty() {
            return Err(HaloWorkbenchError::invalid_request(
                "Non-empty user input is required",
            ));
        }
        if !self.inner.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionMessageUpdated,
            "Workbench user message was recorded",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase.is_terminal() {
                    return false;
                }
                append_message(
                    &mut session.messages,
                    HaloWorkbenchMessageRole::User,
                    content,
                );
                true
            },
        ) {
            return Err(HaloWorkbenchError::session_not_found());
        }
        Ok(())
    }

    async fn execute_session_adapter_action(
        &self,
        generation: u64,
        task_id: &str,
        session_id: &str,
        command: PiRpcCommand,
        allow_session_removal: bool,
    ) -> Result<PiRpcReply, HaloWorkbenchError> {
        self.ensure_workspace_available(generation).await?;
        let managed = self.session_requires_managed_trust(generation, session_id)?;
        if managed {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        self.ensure_session_transport_allowed(generation, task_id, session_id)?;
        let result = if matches!(&command, PiRpcCommand::AbortSession { .. }) {
            // A running prompt can legitimately wait for a Pi response. Abort
            // must still reach that session before the bounded response wait
            // completes; PiRpcAdapter serializes JSONL writes itself.
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        } else if matches!(
            &command,
            PiRpcCommand::SendUserInput { .. } | PiRpcCommand::FollowUp { .. }
        ) {
            // Prompts retain their existing cross-session serialization without
            // blocking a shutdown from fencing a running decision action.
            let _prompt = self.inner.prompt_actions.lock().await;
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        } else {
            let _action = self.inner.adapter_actions.lock().await;
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        };
        self.ensure_workspace_available(generation).await?;
        if managed {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        if !allow_session_removal {
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
        }
        result
    }

    fn session_task_id(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Result<String, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        state
            .sessions
            .get(session_id)
            .map(|session| session.task_id.clone())
            .ok_or_else(HaloWorkbenchError::session_not_found)
    }

    async fn ensure_workspace_available(&self, generation: u64) -> Result<(), HaloWorkbenchError> {
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
                    "Workspace facts could not be revalidated",
                    "retry",
                )
            })?;
        if facts.workspace_id == request.workspace_id && facts.canonical_root == request.root {
            return Ok(());
        }

        let error = HaloWorkbenchError::new(
            "workspace_identity_mismatch",
            "The active workspace changed while the session was running",
            "refresh_workspace",
        );
        let _ = self.close_workspace(None, false).await;
        Err(error)
    }

    async fn ensure_managed_workspace_trusted(
        &self,
        generation: u64,
    ) -> Result<(), HaloWorkbenchError> {
        let confirmation = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state.managed_workspace_confirmation.clone()
        };
        let Some(confirmation) = confirmation else {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        };
        if confirmation.generation != generation {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        }
        let request = WorkbenchWorkspaceFactsRequest {
            workspace_id: confirmation.workspace_id.clone(),
            root: confirmation.canonical_root.clone(),
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
        if facts.workspace_id != request.workspace_id || facts.canonical_root != request.root {
            let error = HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "The managed workspace changed while the task was active",
                "refresh_workspace",
            );
            let _ = self.close_workspace(None, false).await;
            return Err(error);
        }
        if facts.git_repository && facts.trusted {
            return Ok(());
        }
        let error = if facts.git_repository {
            HaloWorkbenchError::new(
                "workspace_untrusted",
                "Workspace trust was revoked while the managed task was active",
                "confirm_managed_workspace",
            )
        } else {
            HaloWorkbenchError::managed_workspace_not_git()
        };
        let _ = self.close_workspace(None, false).await;
        Err(error)
    }

    fn session_requires_managed_trust(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Result<bool, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        state
            .sessions
            .get(session_id)
            .map(|session| session.mode == HaloWorkbenchSessionMode::Managed)
            .ok_or_else(HaloWorkbenchError::session_not_found)
    }

    fn ensure_session_action_allowed(
        &self,
        generation: u64,
        session_id: &str,
        intent: &SessionIntent,
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
        let allowed = match intent {
            SessionIntent::Prompt(_) => matches!(session.phase, HaloWorkbenchSessionPhase::Idle),
            SessionIntent::FollowUp(_) => {
                matches!(session.phase, HaloWorkbenchSessionPhase::WaitingDeveloper)
            }
            SessionIntent::Abort => matches!(session.phase, HaloWorkbenchSessionPhase::Running),
            SessionIntent::End => matches!(
                session.phase,
                HaloWorkbenchSessionPhase::Idle
                    | HaloWorkbenchSessionPhase::Running
                    | HaloWorkbenchSessionPhase::WaitingDeveloper
                    | HaloWorkbenchSessionPhase::Interrupted
            ),
        };
        if allowed {
            return Ok(());
        }
        if session.phase == HaloWorkbenchSessionPhase::Stopping
            || (session.phase == HaloWorkbenchSessionPhase::Running
                && matches!(
                    intent,
                    SessionIntent::Prompt(_) | SessionIntent::FollowUp(_)
                ))
        {
            Err(HaloWorkbenchError::session_busy())
        } else {
            Err(HaloWorkbenchError::session_not_ready())
        }
    }

    fn ensure_session_transport_allowed(
        &self,
        generation: u64,
        task_id: &str,
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
        if session.task_id != task_id {
            return Err(HaloWorkbenchError::session_not_found());
        }
        if session.phase.is_terminal() {
            return Err(HaloWorkbenchError::session_terminal());
        }
        Ok(())
    }

    fn mark_session_running(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        expected_phase: HaloWorkbenchSessionPhase,
    ) -> Result<(), HaloWorkbenchError> {
        let session_id = session_id.to_string();
        if self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is running",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase != expected_phase {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Running;
                true
            },
        ) {
            Ok(())
        } else {
            Err(HaloWorkbenchError::session_busy())
        }
    }

    fn mark_session_stopping(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        intent: SessionIntent,
    ) -> Result<(), HaloWorkbenchError> {
        let session_id = session_id.to_string();
        if self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is stopping",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                let allowed = match intent {
                    SessionIntent::Abort => session.phase == HaloWorkbenchSessionPhase::Running,
                    SessionIntent::End => matches!(
                        session.phase,
                        HaloWorkbenchSessionPhase::Idle
                            | HaloWorkbenchSessionPhase::Running
                            | HaloWorkbenchSessionPhase::WaitingDeveloper
                            | HaloWorkbenchSessionPhase::Interrupted
                    ),
                    SessionIntent::Prompt(_) | SessionIntent::FollowUp(_) => false,
                };
                if !allowed {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Stopping;
                true
            },
        ) {
            Ok(())
        } else {
            Err(HaloWorkbenchError::session_busy())
        }
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
        let session_error = error.clone();
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
                if session.phase.rejects_adapter_events() {
                    return false;
                }
                let projected_phase = if session.mode == HaloWorkbenchSessionMode::Managed
                    && failure_phase == HaloWorkbenchSessionPhase::Failed
                {
                    HaloWorkbenchSessionPhase::Interrupted
                } else {
                    failure_phase
                };
                if !valid_session_transition(session.phase, projected_phase) {
                    return false;
                }
                session.phase = projected_phase;
                session.error = Some(session_error);
                session.cancellation_mode = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != session_id);
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
        let (task_id, session_id) = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state
                .pending_operations
                .get(operation_id)
                .map(|operation| (operation.task_id.clone(), operation.session_id.clone()))
                .ok_or_else(HaloWorkbenchError::operation_not_found)?
        };
        self.ensure_workspace_available(generation).await?;
        if self.session_requires_managed_trust(generation, &session_id)? {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        self.ensure_session_transport_allowed(generation, &task_id, &session_id)?;
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
            self.ensure_session_transport_allowed(generation, &task_id, &session_id)?;
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
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                    operation_id: operation_id.to_string(),
                    decision: decision.into(),
                })
                .await
                .map_err(|error| port_failure(error.kind));
            result
        };
        self.ensure_workspace_available(generation).await?;
        if self.session_requires_managed_trust(generation, &session_id)? {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        self.ensure_session_transport_allowed(generation, &task_id, &session_id)?;
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

    /// Explicitly closes the logical session for delivery review. A settled
    /// session releases its adapter session after freezing bounded/redacted
    /// evidence. An interrupted session is already transport-isolated, so its
    /// explicit review path must not contact Pi again.
    async fn finish_and_review(&self, request_id: &str, session_id: &str) -> IntentResult {
        let generation = self.ready_generation()?;
        let Some(entry) = self.enter_delivery_review(generation, request_id, session_id) else {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            return if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                Err(HaloWorkbenchError::runtime_not_ready())
            } else if !state.sessions.contains_key(session_id) {
                Err(HaloWorkbenchError::session_not_found())
            } else {
                Err(HaloWorkbenchError::delivery_review_not_ready())
            };
        };

        let settled = match entry {
            DeliveryReviewEntry::Settled => {
                self.await_settled_fingerprint(generation, session_id).await
            }
            DeliveryReviewEntry::Interrupted => None,
        };
        let evidence = match self
            .capture_delivery_evidence(generation, session_id, settled)
            .await
        {
            Ok(evidence) => evidence,
            Err(error) => {
                self.handle_delivery_review_failure(
                    entry,
                    generation,
                    request_id,
                    session_id,
                    error.clone(),
                );
                return Err(error);
            }
        };
        let review = match self.build_delivery_review(generation, session_id, evidence) {
            Ok(review) => review,
            Err(error) => {
                self.handle_delivery_review_failure(
                    entry,
                    generation,
                    request_id,
                    session_id,
                    error.clone(),
                );
                return Err(error);
            }
        };
        if !self.attach_delivery_review(generation, request_id, session_id, review) {
            let error = HaloWorkbenchError::session_not_found();
            self.handle_delivery_review_failure(
                entry,
                generation,
                request_id,
                session_id,
                error.clone(),
            );
            return Err(error);
        }

        if entry == DeliveryReviewEntry::Settled {
            self.release_adapter_session(generation, request_id, session_id)
                .await?;
        }
        Ok(self.inner.receipt(request_id, Some(session_id.to_string())))
    }

    /// Records the developer's accept/reject conclusion. No Git write, commit,
    /// push, rollback, file deletion, branch creation or history rewrite is
    /// performed here.
    async fn resolve_delivery(
        &self,
        request_id: &str,
        session_id: &str,
        decision: HaloWorkbenchDeliveryDecision,
    ) -> IntentResult {
        let session_id_owned = session_id.to_string();
        if self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench delivery was resolved",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.terminated {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                let active_review = state.phase == HaloWorkbenchPhase::Ready
                    && session.phase == HaloWorkbenchSessionPhase::Reviewing;
                let interrupted_history = state.phase != HaloWorkbenchPhase::Stopping
                    && session.phase == HaloWorkbenchSessionPhase::Interrupted
                    && session.delivery_review.is_some();
                if session.mode != HaloWorkbenchSessionMode::Managed
                    || (!active_review && !interrupted_history)
                {
                    return false;
                }
                let Some(review) = session.delivery_review.as_mut() else {
                    return false;
                };
                if review.decision.is_some() {
                    return false;
                }
                review.decision = Some(decision);
                session.phase = HaloWorkbenchSessionPhase::Ended;
                session.error = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != session_id_owned);
                true
            },
        ) {
            Ok(self.inner.receipt(request_id, Some(session_id.to_string())))
        } else {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.terminated {
                Err(HaloWorkbenchError::runtime_shutdown())
            } else if !state.sessions.contains_key(session_id) {
                Err(HaloWorkbenchError::session_not_found())
            } else if state.phase != HaloWorkbenchPhase::Ready
                && state
                    .sessions
                    .get(session_id)
                    .is_some_and(|session| session.phase != HaloWorkbenchSessionPhase::Interrupted)
            {
                Err(HaloWorkbenchError::runtime_not_ready())
            } else {
                Err(HaloWorkbenchError::delivery_decision_not_ready())
            }
        }
    }

    fn enter_delivery_review(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
    ) -> Option<DeliveryReviewEntry> {
        let session_id_owned = session_id.to_string();
        let mut entry = None;
        let transitioned = self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is in delivery review",
            Some(session_id_owned.clone()),
            None,
            |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let active_workspace_id = state
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.workspace_id.clone());
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.mode != HaloWorkbenchSessionMode::Managed
                    || active_workspace_id.as_deref() != Some(session.workspace_id.as_str())
                {
                    return false;
                }
                let review_entry = match session.phase {
                    HaloWorkbenchSessionPhase::WaitingDeveloper
                        if session.delivery_review.is_none() =>
                    {
                        DeliveryReviewEntry::Settled
                    }
                    HaloWorkbenchSessionPhase::Interrupted if session.delivery_review.is_none() => {
                        DeliveryReviewEntry::Interrupted
                    }
                    _ => return false,
                };
                session.phase = HaloWorkbenchSessionPhase::Reviewing;
                entry = Some(review_entry);
                true
            },
        );
        transitioned.then_some(entry).flatten()
    }

    fn attach_delivery_review(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        review: HaloWorkbenchDeliveryReviewSnapshot,
    ) -> bool {
        let session_id_owned = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench delivery evidence was frozen",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Reviewing {
                    return false;
                }
                session.delivery_review = Some(review);
                true
            },
        )
    }

    fn fail_session_phase(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        error: HaloWorkbenchError,
    ) {
        let session_id_owned = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session command failed",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.phase.is_terminal()
                    || session.phase == HaloWorkbenchSessionPhase::Interrupted
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Failed;
                session.error = Some(error);
                true
            },
        );
    }

    fn handle_delivery_review_failure(
        &self,
        entry: DeliveryReviewEntry,
        generation: u64,
        request_id: &str,
        session_id: &str,
        error: HaloWorkbenchError,
    ) {
        match entry {
            DeliveryReviewEntry::Settled => {
                self.fail_session_phase(generation, request_id, session_id, error);
            }
            DeliveryReviewEntry::Interrupted => {
                self.restore_interrupted_delivery_review(generation, request_id, session_id);
            }
        }
    }

    fn restore_interrupted_delivery_review(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
    ) {
        let session_id_owned = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Interrupted delivery review remains available",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Reviewing
                    || session.delivery_review.is_some()
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Interrupted;
                true
            },
        );
    }

    async fn await_settled_fingerprint(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Option<WorkbenchDeliveryFingerprint> {
        let mut receiver = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation {
                return None;
            }
            state.settled_fingerprints.get(session_id).cloned()?
        };
        let current = receiver.borrow().clone();
        if current.is_some() {
            return current;
        }
        if tokio::time::timeout(Duration::from_secs(5), receiver.changed())
            .await
            .is_err()
        {
            return None;
        }
        let result = receiver.borrow().clone();
        result
    }

    async fn capture_delivery_evidence(
        &self,
        generation: u64,
        session_id: &str,
        settled: Option<WorkbenchDeliveryFingerprint>,
    ) -> Result<WorkbenchDeliveryEvidence, HaloWorkbenchError> {
        let request = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return Err(HaloWorkbenchError::runtime_not_ready());
            }
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(HaloWorkbenchError::session_not_found)?;
            let baseline = session
                .baseline
                .as_ref()
                .ok_or_else(HaloWorkbenchError::task_baseline_unavailable)?;
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            WorkbenchDeliveryEvidenceRequest {
                workspace_id: workspace.workspace_id.clone(),
                canonical_root: workspace.root_path.clone(),
                baseline: WorkbenchTaskBaseline {
                    head: baseline.head.clone(),
                    canonical_root: baseline.canonical_root.clone(),
                    existing_changed_files: baseline.existing_changed_files.clone(),
                    working_tree_fingerprint: baseline.working_tree_fingerprint.clone(),
                    captured_at_ms: baseline.captured_at_ms,
                },
                settled,
            }
        };
        self.inner
            .delivery_evidence
            .capture(request)
            .await
            .map_err(|_| HaloWorkbenchError::delivery_evidence_unavailable())
    }

    fn build_delivery_review(
        &self,
        generation: u64,
        session_id: &str,
        evidence: WorkbenchDeliveryEvidence,
    ) -> Result<HaloWorkbenchDeliveryReviewSnapshot, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(HaloWorkbenchError::session_not_found)?;
        Ok(HaloWorkbenchDeliveryReviewSnapshot {
            evidence: HaloWorkbenchDeliveryEvidenceSnapshot {
                captured_at_ms: evidence.captured_at_ms,
                head: evidence.head,
                working_tree_fingerprint: evidence.working_tree_fingerprint,
                changed_files: evidence.changed_files,
                diff_preview: redact_halo_text(&evidence.diff_preview, MAX_DELIVERY_DIFF_BYTES),
                attribution: evidence
                    .attribution
                    .into_iter()
                    .map(|item| HaloWorkbenchDeliveryAttributionSnapshot {
                        path: item.path,
                        kind: item.kind.into(),
                    })
                    .collect(),
            },
            summary: summarize_delivery_messages(&session.messages),
            verification_results: summarize_delivery_activities(&session.activities),
            run_conclusion: session
                .messages
                .iter()
                .rev()
                .find(|message| message.role == HaloWorkbenchMessageRole::Assistant)
                .map(|message| redact_halo_text(&message.content, MAX_DELIVERY_SUMMARY_BYTES))
                .unwrap_or_default(),
            decision: None,
        })
    }

    async fn release_adapter_session(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let task_id = self.session_task_id(generation, session_id)?;
        let result = self
            .execute_session_adapter_action(
                generation,
                &task_id,
                session_id,
                PiRpcCommand::EndSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                },
                true,
            )
            .await;
        self.finish_session_command(
            generation,
            request_id,
            session_id,
            result,
            HaloWorkbenchSessionPhase::Failed,
        )?;
        Ok(())
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
    Prompt(String),
    FollowUp(String),
    Abort,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryReviewEntry {
    Settled,
    Interrupted,
}

fn interrupt_managed_sessions(state: &mut RuntimeState, error: &HaloWorkbenchError) {
    let mut interrupted_session_ids = HashSet::new();
    for session in state.sessions.values_mut() {
        if session.mode != HaloWorkbenchSessionMode::Managed
            || session.phase.is_terminal()
            || !valid_session_transition(session.phase, HaloWorkbenchSessionPhase::Interrupted)
        {
            continue;
        }
        session.phase = HaloWorkbenchSessionPhase::Interrupted;
        session.cancellation_mode = None;
        session.error = Some(error.clone());
        interrupted_session_ids.insert(session.session_id.clone());
    }
    state
        .pending_operations
        .retain(|_, operation| !interrupted_session_ids.contains(&operation.session_id));
}

fn retain_managed_interruption_facts(state: &mut RuntimeState) {
    state.sessions.retain(|_, session| {
        session.mode == HaloWorkbenchSessionMode::Managed
            && session.phase == HaloWorkbenchSessionPhase::Interrupted
    });
}

fn interruption_history_snapshots(state: &RuntimeState) -> Vec<HaloWorkbenchSessionSnapshot> {
    state
        .sessions
        .values()
        .filter_map(|session| {
            if session.mode != HaloWorkbenchSessionMode::Managed || session.phase.is_terminal() {
                return None;
            }
            // The durable record is deliberately a post-crash projection, not
            // a resumable transport checkpoint. It can only return as an
            // explicit Interrupted disposition after process loss. Frozen
            // delivery evidence and the task baseline remain reviewable, but
            // active session content never crosses this persistence boundary.
            let mut checkpoint = session.clone();
            if checkpoint.phase.needs_interruption_checkpoint() {
                checkpoint.phase = HaloWorkbenchSessionPhase::Interrupted;
                checkpoint.cancellation_mode = None;
                checkpoint.error = Some(HaloWorkbenchError::application_interrupted());
            }
            checkpoint.messages.clear();
            checkpoint.activities.clear();
            Some(checkpoint)
        })
        .collect()
}

fn valid_session_transition(
    from: HaloWorkbenchSessionPhase,
    to: HaloWorkbenchSessionPhase,
) -> bool {
    use HaloWorkbenchSessionPhase::*;

    matches!(
        (from, to),
        (
            Creating,
            Idle | Running | Interrupted | Stopping | Ended | Failed
        ) | (Idle, Running | Interrupted | Stopping | Ended | Failed)
            | (
                Running,
                WaitingDeveloper | Interrupted | Stopping | Ended | Failed
            )
            | (
                WaitingDeveloper,
                Reviewing | Interrupted | Stopping | Ended | Failed
            )
            | (Reviewing, Interrupted | Failed)
            | (Interrupted, Reviewing | Ended | Failed)
            | (Stopping, Interrupted | Ended | Failed)
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

fn validate_workspace_confirmation(
    workspace_id: &str,
    root_path: &PathBuf,
) -> Result<(), HaloWorkbenchError> {
    if workspace_id.trim().is_empty() || root_path.as_os_str().is_empty() {
        return Err(HaloWorkbenchError::invalid_request(
            "A workspace identity and canonical root are required for managed confirmation",
        ));
    }
    if workspace_id.chars().any(char::is_control)
        || root_path.to_string_lossy().chars().any(char::is_control)
    {
        return Err(HaloWorkbenchError::invalid_request(
            "The managed workspace confirmation contains invalid characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod interruption_history_tests {
    use super::*;

    fn interrupted_session(session_id: &str) -> HaloWorkbenchSessionSnapshot {
        HaloWorkbenchSessionSnapshot {
            workspace_id: "workspace-1".to_string(),
            task_id: "task-1".to_string(),
            session_id: session_id.to_string(),
            mode: HaloWorkbenchSessionMode::Managed,
            phase: HaloWorkbenchSessionPhase::Interrupted,
            cancellation_mode: None,
            baseline: None,
            messages: Vec::new(),
            activities: Vec::new(),
            error: None,
            delivery_review: None,
        }
    }

    #[test]
    fn an_older_checkpoint_cannot_replace_newer_interruption_history() {
        let newer = vec![interrupted_session("newer-session")];
        let older = vec![interrupted_session("older-session")];
        let mut history = InterruptionHistoryState::new(Vec::new());

        assert!(history.should_persist(2, &newer));
        history.mark_persisted(newer.clone());

        assert!(!history.should_persist(1, &older));
        assert_eq!(history.persisted_sessions, newer);
    }
}

fn validate_task_baseline(baseline: &WorkbenchTaskBaseline) -> Result<(), ()> {
    if baseline.head.trim().is_empty()
        || baseline.canonical_root.as_os_str().is_empty()
        || baseline.captured_at_ms < 0
        || baseline.existing_changed_files.len() > MAX_BASELINE_CHANGED_FILES
        || baseline.working_tree_fingerprint.len() != BASELINE_FINGERPRINT_HEX_LENGTH
        || !baseline
            .working_tree_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || baseline.existing_changed_files.iter().any(|path| {
            path.trim().is_empty()
                || path.len() > MAX_PUBLIC_LABEL_BYTES * 8
                || path.chars().any(char::is_control)
        })
    {
        return Err(());
    }
    Ok(())
}

fn summarize_delivery_messages(messages: &[HaloWorkbenchMessageSnapshot]) -> String {
    let joined = messages
        .iter()
        .filter(|message| message.role == HaloWorkbenchMessageRole::Assistant)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    redact_halo_text(&joined, MAX_DELIVERY_SUMMARY_BYTES)
}

fn summarize_delivery_activities(activities: &[HaloWorkbenchActivitySnapshot]) -> String {
    let joined = activities
        .iter()
        .map(|activity| activity.label.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    redact_halo_text(&joined, MAX_DELIVERY_SUMMARY_BYTES)
}

fn append_message(
    messages: &mut Vec<HaloWorkbenchMessageSnapshot>,
    role: HaloWorkbenchMessageRole,
    content: String,
) {
    if content.is_empty() {
        return;
    }
    if role == HaloWorkbenchMessageRole::Assistant
        && messages
            .last()
            .is_some_and(|message| message.role == HaloWorkbenchMessageRole::Assistant)
    {
        if let Some(message) = messages.last_mut() {
            let remaining = MAX_PUBLIC_MESSAGE_BYTES.saturating_sub(message.content.len());
            if remaining > 0 {
                message
                    .content
                    .push_str(&truncate_utf8(&content, remaining));
            }
        }
        return;
    }
    if messages.len() >= MAX_SESSION_MESSAGES {
        messages.remove(0);
    }
    messages.push(HaloWorkbenchMessageSnapshot {
        role,
        content: truncate_utf8(&content, MAX_PUBLIC_MESSAGE_BYTES),
    });
}

fn bounded_public_label(value: &str, max_bytes: usize) -> Option<String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(truncate_utf8(value, max_bytes))
}

/// Tool-call identifiers stay adapter-private even if a malformed adapter
/// event reaches the Runtime. The public snapshot uses an opaque local key.
fn opaque_public_activity_id(value: &str) -> Option<String> {
    bounded_public_label(value, MAX_PUBLIC_LABEL_BYTES)?;
    let digest = Sha256::digest(value.as_bytes());
    Some(format!("activity-{}", hex::encode(&digest[..8])))
}

fn redact_halo_text(value: &str, max_bytes: usize) -> String {
    let mut redacted = value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect::<String>();
    for header in ["authorization", "cookie"] {
        redacted = redact_halo_header_values(&redacted, header);
    }
    for prefix in ["sk-", "sk_", "ghp_", "github_pat_", "xoxb-", "AIza"] {
        redacted = redact_prefixed_halo_token(&redacted, prefix);
    }
    redacted = redact_halo_literal_value(&redacted, "bearer ");
    for name in [
        "api-key",
        "api_key",
        "secret",
        "token",
        "password",
        "sessionid",
        "entryid",
        "toolcallid",
        "session_id",
        "entry_id",
        "tool_call_id",
    ] {
        redacted = redact_halo_named_values(&redacted, name);
    }
    truncate_utf8(&redacted, max_bytes)
}

fn redact_halo_header_values(value: &str, header: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let Some(start) = find_halo_named_marker(&redacted, header, cursor) else {
            break;
        };
        let mut delimiter = start + header.len();
        if redacted[delimiter..].starts_with('"') || redacted[delimiter..].starts_with('\'') {
            delimiter += 1;
        }
        delimiter = skip_halo_horizontal_whitespace(&redacted, delimiter);
        if !redacted[delimiter..].starts_with(':') && !redacted[delimiter..].starts_with('=') {
            cursor = delimiter;
            continue;
        }
        let value_start = skip_halo_horizontal_whitespace(&redacted, delimiter + 1);
        let value_end = halo_header_value_end(&redacted, value_start);
        if value_start == value_end {
            cursor = value_start;
            continue;
        }
        redacted.replace_range(value_start..value_end, "[redacted]");
        cursor = value_start + "[redacted]".len();
    }
    redacted
}

fn redact_halo_named_values(value: &str, name: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let Some(start) = find_halo_named_marker(&redacted, name, cursor) else {
            break;
        };
        let mut delimiter = start + name.len();
        if redacted[delimiter..].starts_with('"') || redacted[delimiter..].starts_with('\'') {
            delimiter += 1;
        }
        delimiter = skip_halo_horizontal_whitespace(&redacted, delimiter);
        if !redacted[delimiter..].starts_with(':') && !redacted[delimiter..].starts_with('=') {
            cursor = delimiter;
            continue;
        }
        let mut value_start = skip_halo_horizontal_whitespace(&redacted, delimiter + 1);
        let quote = redacted[value_start..]
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\'' | '`'));
        if let Some(quote) = quote {
            value_start += quote.len_utf8();
            let value_end = halo_quoted_value_end(&redacted, value_start, quote);
            if value_start != value_end {
                redacted.replace_range(value_start..value_end, "[redacted]");
                cursor = value_start + "[redacted]".len();
                continue;
            }
        } else {
            let value_end = halo_token_value_end(&redacted, value_start);
            if value_start != value_end {
                redacted.replace_range(value_start..value_end, "[redacted]");
                cursor = value_start + "[redacted]".len();
                continue;
            }
        }
        cursor = value_start;
    }
    redacted
}

fn redact_halo_literal_value(value: &str, marker: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let lower = redacted[cursor..].to_ascii_lowercase();
        let Some(relative) = lower.find(marker) else {
            break;
        };
        let value_start = cursor + relative + marker.len();
        let value_end = halo_token_value_end(&redacted, value_start);
        if value_start == value_end {
            cursor = value_start;
            continue;
        }
        redacted.replace_range(value_start..value_end, "[redacted]");
        cursor = value_start + "[redacted]".len();
    }
    redacted
}

fn find_halo_named_marker(value: &str, name: &str, mut cursor: usize) -> Option<usize> {
    while cursor < value.len() {
        let lower = value[cursor..].to_ascii_lowercase();
        let relative = lower.find(name)?;
        let start = cursor + relative;
        let end = start + name.len();
        if halo_identifier_boundary(value, start, end) {
            return Some(start);
        }
        cursor = end;
    }
    None
}

fn halo_identifier_boundary(value: &str, start: usize, end: usize) -> bool {
    let before = value[..start].chars().next_back();
    let after = value[end..].chars().next();
    !before.is_some_and(is_halo_identifier_character)
        && !after.is_some_and(is_halo_identifier_character)
}

fn is_halo_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn skip_halo_horizontal_whitespace(value: &str, mut cursor: usize) -> usize {
    while let Some(character) = value[cursor..].chars().next() {
        if !matches!(character, ' ' | '\t') {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

fn halo_header_value_end(value: &str, value_start: usize) -> usize {
    for (offset, character) in value[value_start..].char_indices() {
        let cursor = value_start + offset;
        if matches!(character, '\n' | '\r') {
            return cursor;
        }
        if cursor > value_start
            && value[..cursor]
                .chars()
                .next_back()
                .is_some_and(|previous| matches!(previous, ' ' | '\t'))
            && is_inline_halo_sensitive_key(value, cursor)
        {
            let mut boundary = cursor;
            while boundary > value_start
                && value[..boundary]
                    .chars()
                    .next_back()
                    .is_some_and(|previous| matches!(previous, ' ' | '\t'))
            {
                boundary -= value[..boundary]
                    .chars()
                    .next_back()
                    .expect("boundary has a preceding character")
                    .len_utf8();
            }
            return boundary;
        }
    }
    value.len()
}

fn is_inline_halo_sensitive_key(value: &str, cursor: usize) -> bool {
    [
        "authorization",
        "cookie",
        "api-key",
        "api_key",
        "secret",
        "token",
        "password",
        "sessionid",
        "entryid",
        "toolcallid",
        "session_id",
        "entry_id",
        "tool_call_id",
    ]
    .into_iter()
    .any(|name| {
        find_halo_named_marker(value, name, cursor) == Some(cursor)
            && halo_named_marker_has_value_delimiter(value, cursor, name)
    })
}

fn halo_named_marker_has_value_delimiter(value: &str, start: usize, name: &str) -> bool {
    let mut cursor = start + name.len();
    if value[cursor..].starts_with('"') || value[cursor..].starts_with('\'') {
        cursor += 1;
    }
    cursor = skip_halo_horizontal_whitespace(value, cursor);
    value[cursor..].starts_with(':') || value[cursor..].starts_with('=')
}

fn halo_quoted_value_end(value: &str, value_start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (offset, character) in value[value_start..].char_indices() {
        if character == quote && !escaped {
            return value_start + offset;
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    value.len()
}

fn halo_token_value_end(value: &str, value_start: usize) -> usize {
    value[value_start..]
        .char_indices()
        .find(|(_, character)| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '`' | ',' | ';' | '}' | ']')
        })
        .map(|(offset, _)| value_start + offset)
        .unwrap_or(value.len())
}

fn redact_prefixed_halo_token(value: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative) = value[cursor..].find(prefix) {
        let start = cursor + relative;
        result.push_str(&value[cursor..start]);
        let end = value[start..]
            .char_indices()
            .find(|(_, character)| {
                character.is_whitespace() || matches!(character, '"' | '\'' | '`' | ',' | ';')
            })
            .map(|(offset, _)| start + offset)
            .unwrap_or(value.len());
        result.push_str("[redacted]");
        cursor = end;
        if cursor >= value.len() {
            break;
        }
    }
    result.push_str(&value[cursor..]);
    result
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
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

fn validate_task_id(task_id: &str) -> Result<(), HaloWorkbenchError> {
    if task_id.trim().is_empty()
        || task_id.len() > 256
        || task_id
            .chars()
            .any(|character| character.is_control() || character == '\\')
    {
        return Err(HaloWorkbenchError::invalid_request(
            "A safe, non-empty task identifier is required",
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
