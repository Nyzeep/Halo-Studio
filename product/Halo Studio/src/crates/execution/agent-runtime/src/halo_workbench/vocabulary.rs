//! Halo Workbench Runtime public vocabulary: snapshots, intents, events, errors.

use std::fmt;
use std::path::PathBuf;


use halo_runtime_ports::{
    ManagedExecutorKind,
    PiRpcAvailabilitySummary,
    PiRpcCancellationMode, PiRpcCapability,
    PiRpcOperationDecision, PiRpcOperationKind, PiRpcOperationRiskLevel,
    PiRpcSessionMode, PiRpcVersionSummary, PortResult, WorkbenchDeliveryAttributionKind,
};
use serde::{Deserialize, Serialize};


pub const HALO_WORKBENCH_SCHEMA_VERSION: u32 = 1;

pub(super) const MAX_COMPLETED_REQUEST_RECORDS: usize = 256;
pub(super) const MAX_COMPLETED_CLEANUP_RECORDS: usize = 64;
pub(super) const MAX_SESSION_MESSAGES: usize = 64;
pub(super) const MAX_SESSION_ACTIVITIES: usize = 128;
pub(super) const MAX_BASELINE_CHANGED_FILES: usize = 4096;
pub(super) const BASELINE_FINGERPRINT_HEX_LENGTH: usize = 64;
pub(super) const MAX_PUBLIC_MESSAGE_BYTES: usize = 16 * 1024;
pub(super) const MAX_PUBLIC_LABEL_BYTES: usize = 128;
pub(super) const MAX_DELIVERY_DIFF_BYTES: usize = 64 * 1024;
pub(super) const MAX_DELIVERY_SUMMARY_BYTES: usize = 16 * 1024;

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
    /// The managed executor this task is bound to for its whole lifetime
    /// (ADR-0078 M3). Recorded in the baseline so every later fact and
    /// review attributes to the selected executor; there is no in-session
    /// switch.
    #[serde(default)]
    pub executor: ManagedExecutorKind,
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
    pub(super) fn is_terminal(self) -> bool {
        matches!(self, Self::Ended | Self::Failed)
    }

    pub(super) fn needs_interruption_checkpoint(self) -> bool {
        !self.is_terminal() && self != Self::Interrupted
    }

    pub(super) fn rejects_adapter_events(self) -> bool {
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
    /// The managed executor fixed at task creation (ADR-0078 M3). Standard
    /// sessions report the workspace default without dispatching through a
    /// managed executor port.
    #[serde(default)]
    pub executor: ManagedExecutorKind,
    pub cancellation_mode: Option<HaloWorkbenchCancellationMode>,
    pub baseline: Option<HaloWorkbenchTaskBaselineSnapshot>,
    pub messages: Vec<HaloWorkbenchMessageSnapshot>,
    pub activities: Vec<HaloWorkbenchActivitySnapshot>,
    pub error: Option<HaloWorkbenchError>,
    pub delivery_review: Option<HaloWorkbenchDeliveryReviewSnapshot>,
}

impl HaloWorkbenchSessionSnapshot {
    pub(super) fn accepts_terminal_adapter_event(&self) -> bool {
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
    pub(super) fn new(code: &str, summary: &str, recovery_action: &str) -> Self {
        Self {
            code: code.to_string(),
            summary: summary.to_string(),
            recovery_action: recovery_action.to_string(),
        }
    }

    pub(super) fn request_id_conflict() -> Self {
        Self::new(
            "request_id_conflict",
            "The request identifier was already used for another intent",
            "create_new_request",
        )
    }

    pub(super) fn invalid_request(summary: &str) -> Self {
        Self::new("invalid_request", summary, "correct_request")
    }

    pub(super) fn runtime_not_ready() -> Self {
        Self::new(
            "runtime_not_ready",
            "The Workbench Runtime is not ready",
            "retry_after_runtime_ready",
        )
    }

    pub(super) fn runtime_shutdown() -> Self {
        Self::new(
            "runtime_shutdown",
            "The Workbench Runtime has shut down",
            "restart_application",
        )
    }

    pub(super) fn managed_event_facts_unavailable() -> Self {
        Self::new(
            "managed_event_facts_unavailable",
            "Managed event facts could not be recorded",
            "retry",
        )
    }

    pub(super) fn interruption_history_unavailable() -> Self {
        Self::new(
            "interruption_history_unavailable",
            "The Workbench interruption history could not be restored",
            "restart_application",
        )
    }

    pub(super) fn workspace_closed() -> Self {
        Self::new(
            "workspace_closed",
            "The Workbench workspace was closed before the task finished",
            "start_new_run_or_review_interruption",
        )
    }

    pub(super) fn application_interrupted() -> Self {
        Self::new(
            "application_interrupted",
            "The Workbench application stopped before the task finished",
            "start_new_run_or_review_interruption",
        )
    }

    pub(super) fn session_not_found() -> Self {
        Self::new(
            "session_not_found",
            "The requested Workbench session was not found",
            "refresh_runtime_snapshot",
        )
    }

    pub(super) fn session_terminal() -> Self {
        Self::new(
            "session_terminal",
            "The requested Workbench session has ended",
            "create_new_session",
        )
    }

    pub(super) fn session_busy() -> Self {
        Self::new(
            "session_busy",
            "The requested Workbench session is already processing a lifecycle action",
            "wait_for_session_state",
        )
    }

    pub(super) fn session_not_ready() -> Self {
        Self::new(
            "session_not_ready",
            "The requested Workbench session is not in the required state for this action",
            "wait_for_agent_settled",
        )
    }

    pub(super) fn task_already_active() -> Self {
        Self::new(
            "task_already_active",
            "The requested Halo task already owns an active session in this workspace",
            "reuse_or_end_existing_session",
        )
    }

    pub(super) fn managed_workspace_confirmation_required() -> Self {
        Self::new(
            "managed_workspace_confirmation_required",
            "Managed execution requires an explicit workspace confirmation",
            "confirm_managed_workspace",
        )
    }

    pub(super) fn managed_workspace_not_git() -> Self {
        Self::new(
            "managed_workspace_not_git",
            "Managed execution requires a Git workspace",
            "choose_git_workspace",
        )
    }

    pub(super) fn task_baseline_unavailable() -> Self {
        Self::new(
            "task_baseline_unavailable",
            "The managed task Git baseline could not be captured",
            "retry",
        )
    }

    pub(super) fn operation_not_found() -> Self {
        Self::new(
            "operation_not_found",
            "The requested operation was not found",
            "refresh_runtime_snapshot",
        )
    }

    pub(super) fn operation_decision_in_progress() -> Self {
        Self::new(
            "operation_decision_in_progress",
            "A decision for this Workbench operation is awaiting confirmation",
            "wait_for_operation_confirmation",
        )
    }

    pub(super) fn delivery_review_not_ready() -> Self {
        Self::new(
            "delivery_review_not_ready",
            "The Workbench session is not ready for delivery review",
            "wait_for_agent_settled",
        )
    }

    pub(super) fn delivery_evidence_unavailable() -> Self {
        Self::new(
            "delivery_evidence_unavailable",
            "The managed delivery evidence could not be captured",
            "retry",
        )
    }

    pub(super) fn delivery_decision_not_ready() -> Self {
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
        /// Optional task-creation executor override (ADR-0078 M3). `None`
        /// selects the workspace default executor. There is no in-session
        /// switch: the resolved executor is fixed on the session and its
        /// baseline for the whole task lifetime.
        executor: Option<ManagedExecutorKind>,
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
            Self::CreateSession {
                task_id,
                mode,
                executor,
            } => formatter
                .debug_struct("CreateSession")
                .field("task_id", task_id)
                .field("mode", mode)
                .field("executor", executor)
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

