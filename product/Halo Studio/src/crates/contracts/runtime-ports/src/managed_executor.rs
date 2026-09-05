//! The unified managed-executor contract shared by every production managed
//! executor adapter (ADR-0078) plus the executor-neutral fact projection that
//! feeds the Runtime fact log (ADR-0080).
//!
//! The port is deliberately deep: a small, complete surface (prompt /
//! follow-up / abort / entry read / approval resolution / event subscription /
//! capability + sandbox facts) behind which adapter protocol details stay.
//! Native executor sessions, credentials, raw protocol records and raw log
//! payloads never cross this seam. Decision outcomes are the closed
//! fail-closed vocabulary from ADR-0012/0078.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{ManagedEventFactKind, PortError, PortErrorKind, PortResult};

/// Upper bound, in bytes, for any managed event fact summary. Enforced by
/// [`normalize_managed_event_summary`], the single redaction gate every fact
/// persisting path must pass through (ADR-0080).
pub const MAX_MANAGED_EVENT_SUMMARY_BYTES: usize = 512;

/// Fail-closed rejection reason for a summary that cannot be safely persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedEventSummaryError {
    UnsafePayload,
}

impl std::fmt::Display for ManagedEventSummaryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafePayload => {
                formatter.write_str("managed event fact payload is unsafe")
            }
        }
    }
}

impl std::error::Error for ManagedEventSummaryError {}

/// The single redaction gate for managed event fact summaries (ADR-0080).
///
/// Adapter protocol normalization happens in each adapter; before any summary
/// is persisted as a fact it passes through this one function so redaction,
/// size limiting and fail-closed rejection take effect in exactly one place.
/// Raw-protocol-like payloads (JSONL records, credential material, raw
/// prompt/response/tool JSON) are rejected instead of persisted.
pub fn normalize_managed_event_summary(
    value: &str,
) -> Result<String, ManagedEventSummaryError> {
    let lower = value.to_ascii_lowercase();
    if value.contains('\0')
        || lower.contains("jsonl")
        || lower.contains("api_key")
        || lower.contains("private_key")
        || lower.contains("credential")
        || lower.contains("\"prompt")
        || lower.contains("\"response")
        || lower.contains("\"tool")
    {
        return Err(ManagedEventSummaryError::UnsafePayload);
    }
    let mut normalized = String::new();
    for line in value.lines() {
        let lower = line.to_ascii_lowercase();
        if ["authorization", "cookie", "password", "secret", "token"]
            .iter()
            .any(|key| lower.contains(key))
        {
            normalized.push_str("[redacted]");
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
        if normalized.len() >= MAX_MANAGED_EVENT_SUMMARY_BYTES {
            break;
        }
    }
    normalized.truncate(normalized.floor_char_boundary(MAX_MANAGED_EVENT_SUMMARY_BYTES));
    Ok(normalized.trim_end().to_string())
}

/// Executor-neutral correlation for one managed session executing one task.
///
/// Both identifiers are Halo-local. Native executor session ids stay inside
/// the adapter (ADR-0078).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedExecutorTarget {
    pub task_id: String,
    pub session_id: String,
}

/// The closed set of real production managed executors a task may select
/// (ADR-0078 M3). The task-creation selector may only offer entries from
/// [`ManagedExecutorKind::production_executors`]; an adapter without a
/// production `ManagedExecutorPort` implementation is never listed, and a
/// session's executor is fixed at task creation with no in-session switch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorKind {
    #[default]
    PiRpc,
    Dsh,
}

impl ManagedExecutorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PiRpc => "pi_rpc",
            Self::Dsh => "dsh",
        }
    }

    /// The real production adapters only. The M5 task-creation selector
    /// renders exactly this list.
    pub const fn production_executors() -> &'static [Self] {
        &[Self::PiRpc, Self::Dsh]
    }
}

/// One executor turn input. The content is the developer-facing text; the
/// custom Debug keeps it out of accidental logs, matching the adapter
/// command DTOs.
#[derive(Clone, PartialEq, Eq)]
pub struct ManagedExecutorPromptRequest {
    pub target: ManagedExecutorTarget,
    pub content: String,
}

impl std::fmt::Debug for ManagedExecutorPromptRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedExecutorPromptRequest")
            .field("target", &self.target)
            .field("content", &"<redacted>")
            .finish()
    }
}

/// The kind of developer decision an executor asks for. P0 carries only the
/// first-party permission flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorApprovalKind {
    Permission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorRiskLevel {
    Standard,
    HighRisk,
}

/// Closed outcome vocabulary for one approval request (ADR-0012/0078).
///
/// The set is exhaustive and fail-closed: `Unavailable` covers timeout,
/// protocol error and a missing approval channel. Nothing outside this enum
/// may be recorded as a decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorApprovalOutcome {
    AllowedOnce,
    Rejected,
    Cancelled,
    /// The fail-closed default: no obtainable decision.
    #[default]
    Unavailable,
}

impl ManagedExecutorApprovalOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllowedOnce => "allowed_once",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Executor-neutral failure classification for a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorFailureKind {
    NotInstalled,
    UnsupportedVersion,
    CapabilityMismatch,
    Authentication,
    Transport,
    Protocol,
    Internal,
}

impl ManagedExecutorFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::UnsupportedVersion => "unsupported_version",
            Self::CapabilityMismatch => "capability_mismatch",
            Self::Authentication => "authentication",
            Self::Transport => "transport",
            Self::Protocol => "protocol",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorToolPhase {
    Started,
    Updated,
    Ended,
}

/// One developer decision crossing the port. `call_id` associates the
/// decision with its `approval/asked` audit event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedExecutorApprovalDecision {
    pub target: ManagedExecutorTarget,
    pub call_id: String,
    pub outcome: ManagedExecutorApprovalOutcome,
}

/// Synchronous abort result. `Cooperative` means the executor acknowledged the
/// abort and settled within the bounded grace period; `Reclaimed` means the
/// adapter closed the owned transport and reclaimed the child after that
/// grace period or an abort transport failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorAbortOutcome {
    Cooperative,
    Reclaimed,
}

/// Bounded, executor-neutral committed-entry facts returned by
/// [`ManagedExecutorPort::read_entries`].
///
/// Entry counts and a redacted ordering cursor cross the port; native entry
/// ids and entry payloads do not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedExecutorEntryPage {
    pub entry_count: u32,
    /// Redacted cursor of the newest committed entry, when one exists.
    pub leaf_cursor: Option<String>,
}

/// The executor-declared capability profile (ADR-0078).
///
/// Every flag is an honest declaration the UI may degrade against; a flag is
/// never asserted true because a caller would like it. `native_sandbox_modes`
/// is an example of an executor-specific flag that stays false until an
/// executor actually exposes sandbox modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedExecutorCapabilityProfile {
    pub adapter_identity: String,
    pub compatibility_profile: String,
    /// Steering messages into a running turn.
    pub steer: bool,
    /// Executor-native turn event queueing. Halo owns queueing in P0.
    pub queue_events: bool,
    /// A structured approval channel exists for the decision flow.
    pub approval_channel: bool,
    /// Committed entries can be read through the port.
    pub entry_read: bool,
    /// The executor natively enumerates sandbox modes.
    pub native_sandbox_modes: bool,
}

/// Sandbox contract facts, contract layer only (ADR-0078).
///
/// Executors report their sandbox reality; nothing here introduces an
/// execution backend. An executor without a native sandbox reports
/// `DangerFullAccess` and must not upgrade enforcement to `Full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedExecutorSandboxFacts {
    pub mode: ManagedExecutorSandboxMode,
    pub enforcement: ManagedExecutorSandboxEnforcement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorSandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedExecutorSandboxEnforcement {
    Full,
    Partial,
}

/// The unified, fact-bearing executor event vocabulary (ADR-0080).
///
/// Every event is committed-granularity and projects into the executor-neutral
/// fact kinds via [`project_managed_executor_event`]. Token-level streaming
/// frames and other live activity intentionally have no variant here: they
/// belong to the activity session record, never to the fact log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedExecutorEvent {
    /// The executor durably accepted one user message.
    UserMessageCommitted {
        session_id: String,
        /// Adapter-redacted display summary; seeds the fact identity only.
        summary: String,
    },
    /// One committed agent reply summary (reply completion, not streaming).
    AgentReplyCommitted {
        session_id: String,
        summary: String,
    },
    /// One committed tool activity transition.
    ToolActivityCommitted {
        session_id: String,
        call_id: String,
        phase: ManagedExecutorToolPhase,
        tool_name: String,
        is_error: bool,
    },
    /// `approval/asked` audit event. `call_id` is the executor-neutral
    /// correlation id for the later `approval/decided` event.
    ApprovalAsked {
        session_id: String,
        call_id: String,
        kind: ManagedExecutorApprovalKind,
        tool_name: String,
        redacted_arguments: String,
        risk_level: ManagedExecutorRiskLevel,
    },
    /// `approval/decided` audit event with the closed outcome.
    ApprovalDecided {
        session_id: String,
        call_id: String,
        outcome: ManagedExecutorApprovalOutcome,
    },
    /// One failed executor attempt, counted per attempt so attempts stay
    /// independent facts.
    AttemptFailed {
        session_id: String,
        attempt: u64,
        reason: ManagedExecutorFailureKind,
    },
    /// The task was cancelled: the delivered prefix stays recorded and no
    /// completion fact follows.
    Interrupted {
        session_id: String,
    },
}

/// One projected, normalized fact input. The Runtime derives the final fact id
/// from `identity` and appends it through the fact store port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedExecutorFactDraft {
    /// Executor-neutral identity seed; equal identities are idempotent
    /// re-projections, different identities are different facts.
    pub identity: String,
    pub kind: ManagedEventFactKind,
    /// Already passed through [`normalize_managed_event_summary`].
    pub redacted_summary: String,
}

const IDENTITY_SEPARATOR: char = '\u{1}';

/// Projects one unified executor event into fact drafts (ADR-0080).
///
/// Fact summaries are Halo-owned structural constants; executor display
/// summaries never enter the fact log. Every returned summary has passed the
/// single [`normalize_managed_event_summary`] gate. A summary the gate rejects
/// contributes no draft: nothing unsafe is ever persisted.
pub fn project_managed_executor_event(
    task_id: &str,
    event: &ManagedExecutorEvent,
) -> Vec<ManagedExecutorFactDraft> {
    let identity = |parts: &[&str]| -> String {
        let mut joined = String::from(task_id);
        for part in parts {
            joined.push(IDENTITY_SEPARATOR);
            joined.push_str(part);
        }
        joined
    };
    let draft = |parts: &[&str], kind, summary: String| -> Option<ManagedExecutorFactDraft> {
        let redacted_summary = normalize_managed_event_summary(&summary).ok()?;
        Some(ManagedExecutorFactDraft {
            identity: identity(parts),
            kind,
            redacted_summary,
        })
    };

    match event {
        ManagedExecutorEvent::UserMessageCommitted { session_id, summary } => draft(
            &[session_id, "user", summary],
            ManagedEventFactKind::UserMessageSummary,
            "Managed user message received".to_string(),
        )
        .into_iter()
        .collect(),
        ManagedExecutorEvent::AgentReplyCommitted { session_id, summary } => draft(
            &[session_id, "reply", summary],
            ManagedEventFactKind::AgentReplySummary,
            "Managed agent reply summary received".to_string(),
        )
        .into_iter()
        .collect(),
        ManagedExecutorEvent::ToolActivityCommitted {
            session_id,
            call_id,
            phase,
            ..
        } => draft(
            &[session_id, call_id, phase_str(*phase)],
            ManagedEventFactKind::ToolActivity,
            format!("Managed tool activity {}", phase_str(*phase)),
        )
        .into_iter()
        .collect(),
        ManagedExecutorEvent::ApprovalAsked {
            session_id, call_id, ..
        } => draft(
            &[session_id, call_id, "asked"],
            ManagedEventFactKind::AgentOperationRequest,
            format!("approval asked {call_id}"),
        )
        .into_iter()
        .collect(),
        ManagedExecutorEvent::ApprovalDecided {
            session_id,
            call_id,
            outcome,
        } => draft(
            &[session_id, call_id, "decided"],
            ManagedEventFactKind::AgentOperationDecision,
            format!("approval decided {call_id} {}", outcome.as_str()),
        )
        .into_iter()
        .collect(),
        ManagedExecutorEvent::AttemptFailed {
            session_id,
            attempt,
            reason,
        } => draft(
            &[session_id, "attempt", &attempt.to_string(), reason.as_str()],
            ManagedEventFactKind::AttemptFailed,
            format!("Managed attempt {attempt} failed: {}", reason.as_str()),
        )
        .into_iter()
        .collect(),
        ManagedExecutorEvent::Interrupted { session_id } => draft(
            &[session_id, "interrupted"],
            ManagedEventFactKind::TaskInterrupted,
            "Managed task interrupted; delivered prefix preserved".to_string(),
        )
        .into_iter()
        .collect(),
    }
}

const fn phase_str(phase: ManagedExecutorToolPhase) -> &'static str {
    match phase {
        ManagedExecutorToolPhase::Started => "started",
        ManagedExecutorToolPhase::Updated => "updated",
        ManagedExecutorToolPhase::Ended => "ended",
    }
}

/// The common execution face every production managed executor adapter
/// implements (ADR-0078): prompt / follow-up / abort / entry read / the
/// one-shot approval decision flow / the unified event projection plus the
/// honest capability and sandbox facts.
///
/// Implementations must keep native executor sessions, credentials and raw
/// protocol records behind the port and must answer `resolve_approval` only
/// with the closed [`ManagedExecutorApprovalOutcome`] vocabulary.
#[async_trait]
pub trait ManagedExecutorPort: Send + Sync {
    /// The executor's honest capability profile for UI degradation.
    fn capability_profile(&self) -> ManagedExecutorCapabilityProfile;

    /// The executor's sandbox reality, contract layer only.
    fn sandbox_facts(&self) -> ManagedExecutorSandboxFacts;

    /// Starts a managed turn with developer input.
    async fn prompt(&self, request: ManagedExecutorPromptRequest) -> PortResult<()>;

    /// Continues after the executor settled and is waiting for the developer.
    async fn follow_up(&self, request: ManagedExecutorPromptRequest) -> PortResult<()>;

    /// Steers a currently running turn with developer input. Only executors
    /// whose capability profile declares `steer` implement this; the default
    /// implementation fails closed without touching the executor.
    async fn steer(&self, _request: ManagedExecutorPromptRequest) -> PortResult<()> {
        Err(PortError::new(
            PortErrorKind::InvalidRequest,
            "executor has not adopted steering for a running turn",
        ))
    }

    /// Aborts the running turn and reclaims the executor session.
    async fn abort(&self, target: ManagedExecutorTarget)
        -> PortResult<ManagedExecutorAbortOutcome>;

    /// Reads bounded, redacted committed-entry facts.
    async fn read_entries(
        &self,
        target: ManagedExecutorTarget,
    ) -> PortResult<ManagedExecutorEntryPage>;

    /// Resolves one approval request exactly once. Outcomes the adapter
    /// cannot express fail closed without touching the executor.
    async fn resolve_approval(
        &self,
        decision: ManagedExecutorApprovalDecision,
    ) -> PortResult<()>;

    /// Subscribes to the unified, fact-bearing event vocabulary.
    fn subscribe(&self) -> broadcast::Receiver<ManagedExecutorEvent>;
}
