//! The pi adapter's convergence onto the unified `ManagedExecutorPort`
//! (ADR-0078).
//!
//! [`PiRpcManagedExecutor`] is a thin wrapper over [`PiRpcPort`]: it maps the
//! port's common face onto the Pi RPC commands, translates Pi protocol events
//! into the unified fact-bearing event vocabulary (ADR-0080), and derives the
//! capability profile (including the adopted 0.85.0 steer/queue-event flags)
//! from the inner port's verified readiness facts. Native Pi sessions,
//! credentials, raw JSONL records and raw log payloads never cross this
//! wrapper.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use halo_runtime_ports::{
    normalize_managed_event_summary, ManagedExecutorAbortOutcome,
    ManagedExecutorApprovalDecision, ManagedExecutorApprovalKind, ManagedExecutorApprovalOutcome,
    ManagedExecutorCapabilityProfile, ManagedExecutorEntryPage, ManagedExecutorEvent,
    ManagedExecutorFailureKind, ManagedExecutorPort, ManagedExecutorPromptRequest,
    ManagedExecutorRiskLevel, ManagedExecutorSandboxEnforcement, ManagedExecutorSandboxFacts,
    ManagedExecutorSandboxMode, ManagedExecutorTarget, PiRpcCommand, PiRpcCompatibilityProfile,
    PiRpcEvent, PiRpcFailureKind,
    PiRpcOperationDecision, PiRpcOperationKind, PiRpcOperationRiskLevel, PiRpcPort, PiRpcReply,
    PortError, PortErrorKind, PortResult, PI_RPC_ADAPTER_IDENTITY,
};
use tokio::sync::{broadcast, Mutex};

const EVENT_CAPACITY: usize = 128;
/// Upper bound for one session's accumulated reply text. Assistant deltas are
/// already adapter-redacted; the accumulator only keeps them bounded.
const MAX_ACCUMULATED_REPLY_BYTES: usize = 8 * 1024;

/// Mutable normalization state shared by the event forwarder and the decision
/// path. Exposed for contract tests so translation stays a pure function.
#[derive(Default)]
pub struct PiEventNormalization {
    /// The adapter generation observed from the latest lifecycle event.
    pub generation: Option<u64>,
    /// session id -> accumulated, adapter-redacted reply text since the last
    /// settlement.
    pub accumulated_replies: HashMap<String, String>,
    /// session id -> failed-attempt counter; attempts stay independent facts.
    pub attempt_counts: HashMap<String, u64>,
    /// call id -> outcome the wrapper forwarded, resolved from the next
    /// matching executor resolution event.
    pub pending_outcomes: HashMap<String, ManagedExecutorApprovalOutcome>,
}

impl PiEventNormalization {
    /// Records the outcome of a decision the wrapper actually forwarded so
    /// the later `OperationResolved` event can carry the real outcome.
    pub(crate) fn record_forwarded_outcome(
        &mut self,
        call_id: &str,
        outcome: ManagedExecutorApprovalOutcome,
    ) {
        self.pending_outcomes
            .insert(call_id.to_string(), outcome);
    }
}

/// Maps a Pi failure reason onto the executor-neutral failure vocabulary.
pub const fn managed_executor_failure_kind(
    reason: PiRpcFailureKind,
) -> ManagedExecutorFailureKind {
    match reason {
        PiRpcFailureKind::NotInstalled => ManagedExecutorFailureKind::NotInstalled,
        PiRpcFailureKind::UnsupportedVersion => ManagedExecutorFailureKind::UnsupportedVersion,
        PiRpcFailureKind::CapabilityMismatch => ManagedExecutorFailureKind::CapabilityMismatch,
        PiRpcFailureKind::Authentication => ManagedExecutorFailureKind::Authentication,
        PiRpcFailureKind::Transport => ManagedExecutorFailureKind::Transport,
        PiRpcFailureKind::Protocol => ManagedExecutorFailureKind::Protocol,
        PiRpcFailureKind::Internal => ManagedExecutorFailureKind::Internal,
    }
}

const fn managed_executor_risk_level(
    risk_level: PiRpcOperationRiskLevel,
) -> ManagedExecutorRiskLevel {
    match risk_level {
        PiRpcOperationRiskLevel::Standard => ManagedExecutorRiskLevel::Standard,
        PiRpcOperationRiskLevel::HighRisk => ManagedExecutorRiskLevel::HighRisk,
    }
}

/// Normalizes one Pi RPC protocol event into zero or more unified,
/// fact-bearing events (ADR-0080).
///
/// Committed granularity: token-level `MessageUpdated` frames only accumulate
/// as live activity; the committed reply summary event is emitted once, at the
/// reliable `agent_settled` boundary. Cancellation lands as `Interrupted`;
/// failed attempts land as independently counted `AttemptFailed` events.
pub fn normalize_pi_rpc_event(
    event: &PiRpcEvent,
    state: &mut PiEventNormalization,
) -> Vec<ManagedExecutorEvent> {
    match event {
        PiRpcEvent::Ready { generation } => {
            state.generation = Some(*generation);
            Vec::new()
        }
        PiRpcEvent::Failed { .. }
        | PiRpcEvent::SessionCreated { .. }
        | PiRpcEvent::SessionRunning { .. }
        | PiRpcEvent::SessionIdle { .. }
        | PiRpcEvent::SessionEnded { .. }
        // Live queue activity stays in the adapter; Halo owns queueing and
        // committed facts never derive from queue bookkeeping.
        | PiRpcEvent::QueueUpdated { .. } => Vec::new(),
        PiRpcEvent::AgentSettled { session_id, .. } => {
            let summary = state
                .accumulated_replies
                .remove(session_id)
                .unwrap_or_default();
            if summary.is_empty() {
                return Vec::new();
            }
            vec![ManagedExecutorEvent::AgentReplyCommitted {
                session_id: session_id.clone(),
                summary,
            }]
        }
        PiRpcEvent::SessionStopped { session_id, .. } => {
            vec![ManagedExecutorEvent::Interrupted {
                session_id: session_id.clone(),
            }]
        }
        PiRpcEvent::SessionFailed {
            session_id, reason, ..
        } => {
            let attempt = state
                .attempt_counts
                .entry(session_id.clone())
                .and_modify(|count| *count += 1)
                .or_insert(1);
            vec![ManagedExecutorEvent::AttemptFailed {
                session_id: session_id.clone(),
                attempt: *attempt,
                reason: managed_executor_failure_kind(*reason),
            }]
        }
        PiRpcEvent::ToolExecutionStarted {
            session_id,
            redacted_tool_call_id,
            tool_name,
            ..
        } => {
            vec![ManagedExecutorEvent::ToolActivityCommitted {
                session_id: session_id.clone(),
                call_id: redacted_tool_call_id.clone(),
                phase: halo_runtime_ports::ManagedExecutorToolPhase::Started,
                tool_name: tool_name.clone(),
                is_error: false,
            }]
        }
        PiRpcEvent::ToolExecutionUpdated {
            session_id,
            redacted_tool_call_id,
            tool_name,
            ..
        } => {
            vec![ManagedExecutorEvent::ToolActivityCommitted {
                session_id: session_id.clone(),
                call_id: redacted_tool_call_id.clone(),
                phase: halo_runtime_ports::ManagedExecutorToolPhase::Updated,
                tool_name: tool_name.clone(),
                is_error: false,
            }]
        }
        PiRpcEvent::ToolExecutionEnded {
            session_id,
            redacted_tool_call_id,
            tool_name,
            is_error,
            ..
        } => {
            vec![ManagedExecutorEvent::ToolActivityCommitted {
                session_id: session_id.clone(),
                call_id: redacted_tool_call_id.clone(),
                phase: halo_runtime_ports::ManagedExecutorToolPhase::Ended,
                tool_name: tool_name.clone(),
                is_error: *is_error,
            }]
        }
        PiRpcEvent::MessageUpdated {
            session_id, text, ..
        } => {
            let accumulated = state
                .accumulated_replies
                .entry(session_id.clone())
                .or_default();
            let remaining = MAX_ACCUMULATED_REPLY_BYTES.saturating_sub(accumulated.len());
            if remaining > 0 {
                let take = text.len().min(remaining);
                let take = text.floor_char_boundary(take);
                accumulated.push_str(&text[..take]);
            }
            Vec::new()
        }
        PiRpcEvent::OperationRequested {
            session_id,
            operation_id,
            kind,
            summary,
            ..
        } => {
            if !matches!(kind, PiRpcOperationKind::Permission) {
                return Vec::new();
            }
            vec![ManagedExecutorEvent::ApprovalAsked {
                session_id: session_id.clone(),
                call_id: operation_id.clone(),
                kind: ManagedExecutorApprovalKind::Permission,
                tool_name: summary.tool_name.clone(),
                redacted_arguments: summary.arguments.clone(),
                risk_level: managed_executor_risk_level(summary.risk_level),
            }]
        }
        PiRpcEvent::OperationResolved {
            session_id,
            operation_id,
            ..
        } => {
            // A resolution the wrapper forwarded carries its real outcome;
            // one resolved without an observed decision is honestly
            // `Unavailable`, never an invented allow or deny.
            let outcome = state
                .pending_outcomes
                .remove(operation_id)
                .unwrap_or(ManagedExecutorApprovalOutcome::Unavailable);
            vec![ManagedExecutorEvent::ApprovalDecided {
                session_id: session_id.clone(),
                call_id: operation_id.clone(),
                outcome,
            }]
        }
    }
}

/// The pi adapter's `ManagedExecutorPort` implementation, thin-wrapped over
/// any `PiRpcPort` (the production `PiRpcAdapter` or a contract-test fake).
#[derive(Clone)]
pub struct PiRpcManagedExecutor {
    inner: Arc<dyn PiRpcPort>,
    events: broadcast::Sender<ManagedExecutorEvent>,
    state: Arc<Mutex<PiEventNormalization>>,
}

impl PiRpcManagedExecutor {
    /// Wraps a Pi RPC port. The capability profile is derived from the
    /// inner port's verified readiness facts; the wrapper never invents one.
    /// Must be called within a Tokio runtime: the event forwarder task is
    /// spawned here, after a synchronous subscription so no event is missed.
    pub fn new(inner: Arc<dyn PiRpcPort>) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let executor = Self {
            inner,
            events,
            state: Arc::new(Mutex::new(PiEventNormalization::default())),
        };
        let mut receiver = executor.inner.subscribe();
        let forward_events = executor.events.clone();
        let forward_state = executor.state.clone();
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        let mut state = forward_state.lock().await;
                        for unified in normalize_pi_rpc_event(&event, &mut state) {
                            let _ = forward_events.send(unified);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        executor
    }

    /// The currently observed adapter generation, if any. A diagnostic
    /// accessor for consumers and contract tests; command methods fail
    /// closed until a generation has been observed.
    pub async fn current_generation(&self) -> Option<u64> {
        self.state.lock().await.generation
    }

    async fn observed_generation(&self) -> PortResult<u64> {
        let state = self.state.lock().await;
        state.generation.ok_or_else(|| {
            PortError::new(
                PortErrorKind::NotAvailable,
                "pi managed executor has not observed an adapter generation",
            )
        })
    }

    /// Emits the committed user-message event for an accepted prompt or
    /// follow-up. The summary passes the single redaction gate; a rejected
    /// summary emits nothing (fail-closed, nothing fabricated).
    fn emit_user_message_committed(&self, session_id: &str, content: &str) {
        if let Ok(summary) = normalize_managed_event_summary(content) {
            let _ = self.events.send(ManagedExecutorEvent::UserMessageCommitted {
                session_id: session_id.to_string(),
                summary,
            });
        }
    }
}

#[async_trait]
impl ManagedExecutorPort for PiRpcManagedExecutor {
    fn capability_profile(&self) -> ManagedExecutorCapabilityProfile {
        match self.inner.readiness() {
            Some(summary) => {
                let profile = summary.version.profile;
                let adopted_0850 = profile == PiRpcCompatibilityProfile::PiRpc0850P0;
                ManagedExecutorCapabilityProfile {
                    adapter_identity: PI_RPC_ADAPTER_IDENTITY.to_string(),
                    compatibility_profile: profile.as_str().to_string(),
                    // The 0.85.0 profile adopts steering and native queue
                    // events (M3); older profiles honestly report false and
                    // pi has no native sandbox mode enumeration.
                    steer: adopted_0850,
                    queue_events: adopted_0850,
                    approval_channel: true,
                    entry_read: true,
                    native_sandbox_modes: false,
                }
            }
            None => ManagedExecutorCapabilityProfile {
                adapter_identity: PI_RPC_ADAPTER_IDENTITY.to_string(),
                // No verified readiness facts yet: nothing is claimed.
                compatibility_profile: "unprobed".to_string(),
                steer: false,
                queue_events: false,
                approval_channel: false,
                entry_read: false,
                native_sandbox_modes: false,
            },
        }
    }

    fn sandbox_facts(&self) -> ManagedExecutorSandboxFacts {
        // Pi executes with the developer's full privileges and has no native
        // sandbox; the first-party approval gate only partially constrains
        // tool calls. Reported as-is, never upgraded to full.
        ManagedExecutorSandboxFacts {
            mode: ManagedExecutorSandboxMode::DangerFullAccess,
            enforcement: ManagedExecutorSandboxEnforcement::Partial,
        }
    }

    async fn prompt(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        let generation = self.observed_generation().await?;
        self.inner
            .execute(PiRpcCommand::SendUserInput {
                generation,
                task_id: request.target.task_id,
                session_id: request.target.session_id.clone(),
                content: request.content.clone(),
            })
            .await?;
        self.emit_user_message_committed(&request.target.session_id, &request.content);
        Ok(())
    }

    async fn follow_up(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        let generation = self.observed_generation().await?;
        self.inner
            .execute(PiRpcCommand::FollowUp {
                generation,
                task_id: request.target.task_id,
                session_id: request.target.session_id.clone(),
                content: request.content.clone(),
            })
            .await?;
        self.emit_user_message_committed(&request.target.session_id, &request.content);
        Ok(())
    }

    /// Steers the running turn. Only the adopted 0.85.0 profile forwards a
    /// Pi `steer`; every other readiness state fails closed before Pi stdin.
    async fn steer(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        let adopted = self.inner.readiness().is_some_and(|summary| {
            summary.version.profile == PiRpcCompatibilityProfile::PiRpc0850P0
        });
        if !adopted {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "pi executor has not adopted steering for this profile",
            ));
        }
        let generation = self.observed_generation().await?;
        self.inner
            .execute(PiRpcCommand::Steer {
                generation,
                task_id: request.target.task_id,
                session_id: request.target.session_id,
                content: request.content,
            })
            .await?;
        Ok(())
    }

    async fn abort(&self, target: ManagedExecutorTarget) -> PortResult<ManagedExecutorAbortOutcome> {
        let generation = self.observed_generation().await?;
        self.inner
            .execute(PiRpcCommand::AbortSession {
                generation,
                task_id: target.task_id,
                session_id: target.session_id,
            })
            .await?;
        // An accepted abort means the executor acknowledged the abort and
        // settled within the bounded grace period; a forced reclaim surfaces
        // as an error or a transport failure, never as this outcome.
        Ok(ManagedExecutorAbortOutcome::Cooperative)
    }

    async fn read_entries(&self, target: ManagedExecutorTarget) -> PortResult<ManagedExecutorEntryPage> {
        let generation = self.observed_generation().await?;
        match self
            .inner
            .execute(PiRpcCommand::GetEntries {
                generation,
                task_id: target.task_id,
                session_id: target.session_id,
            })
            .await?
        {
            PiRpcReply::Entries {
                entry_count,
                leaf_cursor,
            } => Ok(ManagedExecutorEntryPage {
                entry_count,
                leaf_cursor,
            }),
            _ => Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "pi managed executor received an unexpected reply for an entry read",
            )),
        }
    }

    async fn resolve_approval(
        &self,
        decision: ManagedExecutorApprovalDecision,
    ) -> PortResult<()> {
        let generation = self.observed_generation().await?;
        // The closed outcome vocabulary is fail-closed: outcomes this
        // executor cannot express never reach Pi and never get rewritten
        // into an allow or deny.
        let forwarded = match decision.outcome {
            ManagedExecutorApprovalOutcome::AllowedOnce => PiRpcOperationDecision::AllowOnce,
            ManagedExecutorApprovalOutcome::Rejected => PiRpcOperationDecision::Deny,
            ManagedExecutorApprovalOutcome::Cancelled | ManagedExecutorApprovalOutcome::Unavailable => {
                return Err(PortError::new(
                    PortErrorKind::InvalidRequest,
                    "pi cannot express this approval outcome; the decision was not forwarded",
                ));
            }
        };
        self.inner
            .execute(PiRpcCommand::ResolveOperation {
                generation,
                task_id: decision.target.task_id,
                session_id: decision.target.session_id,
                operation_id: decision.call_id.clone(),
                decision: forwarded,
            })
            .await?;
        self.state
            .lock()
            .await
            .record_forwarded_outcome(&decision.call_id, decision.outcome);
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<ManagedExecutorEvent> {
        self.events.subscribe()
    }
}
