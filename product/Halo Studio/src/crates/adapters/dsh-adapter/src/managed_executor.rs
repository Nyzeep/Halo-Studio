//! The DSH adapter's `ManagedExecutorPort` implementation (ADR-0078/0080).
//!
//! Adapter protocol events are normalized here into the unified, fact-bearing
//! event vocabulary before anything crosses the port: committed message
//! chunks accumulate toward one reply summary per turn, tool-call lifecycles
//! project their committed transitions, permission requests project the
//! `approval/asked` / `approval/decided` audit pair, failed attempts are
//! counted independently, and interrupted turns land as `Interrupted` with the
//! delivered prefix preserved and no completion fact. The SDK canary channel
//! projects through the exact same vocabulary — its degraded capability
//! profile is declared honestly instead of being papered over.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use halo_runtime_ports::{
    project_managed_executor_event, normalize_managed_event_summary, ManagedExecutorAbortOutcome,
    ManagedExecutorApprovalDecision, ManagedExecutorEvent,
    ManagedExecutorFailureKind, ManagedExecutorPort, ManagedExecutorPromptRequest,
    ManagedExecutorSandboxEnforcement, ManagedExecutorSandboxFacts, ManagedExecutorSandboxMode,
    ManagedExecutorTarget, PortError, PortErrorKind, PortResult,
};
use tokio::sync::{broadcast, Mutex};

use crate::{DshAdapter, DshEvent, DshFailureKind, DSH_ADAPTER_IDENTITY};

/// Reply summaries are bounded like the pi adapter's accumulated replies;
/// token-level detail never becomes a fact.
const MAX_ACCUMULATED_REPLY_BYTES: usize = 8 * 1024;

const EVENT_CAPACITY: usize = 128;

pub fn managed_executor_failure_kind(kind: DshFailureKind) -> ManagedExecutorFailureKind {
    match kind {
        DshFailureKind::NotInstalled => ManagedExecutorFailureKind::NotInstalled,
        DshFailureKind::UnsupportedVersion => ManagedExecutorFailureKind::UnsupportedVersion,
        DshFailureKind::Protocol => ManagedExecutorFailureKind::Protocol,
        DshFailureKind::Transport => ManagedExecutorFailureKind::Transport,
        DshFailureKind::Authentication => ManagedExecutorFailureKind::Authentication,
        DshFailureKind::Internal => ManagedExecutorFailureKind::Internal,
    }
}

/// Per-session normalization state for the event forwarder.
#[derive(Default)]
pub(crate) struct DshEventNormalization {
    reply_buffers: HashMap<String, String>,
    attempt_counts: HashMap<String, u64>,
    interrupted_turns: HashSet<String>,
}

/// Normalizes one adapter event into zero or more unified fact-bearing events
/// (ADR-0080).
pub(crate) fn normalize_dsh_event(
    event: &DshEvent,
    state: &mut DshEventNormalization,
) -> Vec<ManagedExecutorEvent> {
    match event {
        DshEvent::MessageChunk { session_id, text } => {
            let accumulated = state
                .reply_buffers
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
        DshEvent::ToolCallStarted {
            session_id,
            call_id,
            tool_name,
        } => vec![ManagedExecutorEvent::ToolActivityCommitted {
            session_id: session_id.clone(),
            call_id: call_id.clone(),
            phase: halo_runtime_ports::ManagedExecutorToolPhase::Started,
            tool_name: tool_name.clone(),
            is_error: false,
        }],
        DshEvent::ToolCallEnded {
            session_id,
            call_id,
            is_error,
        } => vec![ManagedExecutorEvent::ToolActivityCommitted {
            session_id: session_id.clone(),
            call_id: call_id.clone(),
            phase: halo_runtime_ports::ManagedExecutorToolPhase::Ended,
            tool_name: String::new(),
            is_error: *is_error,
        }],
        DshEvent::PermissionRequested {
            session_id,
            operation_id,
            tool_name,
            redacted_arguments,
        } => vec![ManagedExecutorEvent::ApprovalAsked {
            session_id: session_id.clone(),
            call_id: operation_id.clone(),
            kind: halo_runtime_ports::ManagedExecutorApprovalKind::Permission,
            tool_name: tool_name.clone(),
            redacted_arguments: redacted_arguments.clone(),
            risk_level: halo_runtime_ports::ManagedExecutorRiskLevel::Standard,
        }],
        DshEvent::PermissionResolved {
            session_id,
            operation_id,
            outcome,
        } => {
            // A resolution without an observed decision is honestly
            // `Unavailable`, never an invented allow or deny.
            vec![ManagedExecutorEvent::ApprovalDecided {
                session_id: session_id.clone(),
                call_id: operation_id.clone(),
                outcome: outcome.unwrap_or(halo_runtime_ports::ManagedExecutorApprovalOutcome::Unavailable),
            }]
        }
        DshEvent::PromptSettled {
            session_id,
            stop_reason,
        } => {
            if *stop_reason == "cancelled" {
                // The delivered prefix stays recorded; no completion fact
                // follows an interrupted turn.
                state.reply_buffers.remove(session_id);
                if state.interrupted_turns.insert(session_id.clone()) {
                    vec![ManagedExecutorEvent::Interrupted {
                        session_id: session_id.clone(),
                    }]
                } else {
                    Vec::new()
                }
            } else {
                state.interrupted_turns.remove(session_id);
                let summary = state
                    .reply_buffers
                    .remove(session_id)
                    .unwrap_or_default();
                if summary.is_empty() {
                    Vec::new()
                } else {
                    vec![ManagedExecutorEvent::AgentReplyCommitted {
                        session_id: session_id.clone(),
                        summary,
                    }]
                }
            }
        }
        DshEvent::TurnAborted { session_id } => {
            // An abort that was never wire-confirmed still lands as
            // `Interrupted`; one that settled as `cancelled` already did.
            if state.interrupted_turns.insert(session_id.clone()) {
                vec![ManagedExecutorEvent::Interrupted {
                    session_id: session_id.clone(),
                }]
            } else {
                Vec::new()
            }
        }
        DshEvent::TransportEnded => Vec::new(),
        DshEvent::SessionFailed {
            session_id,
            reason,
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
    }
}

/// The DSH adapter's `ManagedExecutorPort` implementation, thin-wrapped over
/// the production adapter or a contract-test stand-in.
#[derive(Clone)]
pub struct DshManagedExecutor {
    inner: Arc<DshAdapter>,
    events: broadcast::Sender<ManagedExecutorEvent>,
    state: Arc<Mutex<DshEventNormalization>>,
}

impl DshManagedExecutor {
    /// Wraps the DSH adapter. Must be called within a Tokio runtime: the
    /// event forwarder task is spawned here, after a synchronous subscription
    /// so no event is missed.
    pub fn new(inner: Arc<DshAdapter>) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let executor = Self {
            inner,
            events,
            state: Arc::new(Mutex::new(DshEventNormalization::default())),
        };
        let mut receiver = executor.inner.subscribe_internal();
        let forward_events = executor.events.clone();
        let forward_state = executor.state.clone();
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        let mut state = forward_state.lock().await;
                        for unified in normalize_dsh_event(&event, &mut state) {
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

    /// Projects collected unified events through the M1 fact projection —
    /// exposed for contract tests; production consumers call the port-level
    /// projection themselves.
    pub fn project_to_facts(
        task_id: &str,
        events: &[ManagedExecutorEvent],
    ) -> Vec<halo_runtime_ports::ManagedExecutorFactDraft> {
        events
            .iter()
            .flat_map(|event| project_managed_executor_event(task_id, event))
            .collect()
    }

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
impl ManagedExecutorPort for DshManagedExecutor {
    fn capability_profile(&self) -> halo_runtime_ports::ManagedExecutorCapabilityProfile {
        let degraded = self.inner.channel().is_degraded_canary();
        halo_runtime_ports::ManagedExecutorCapabilityProfile {
            adapter_identity: DSH_ADAPTER_IDENTITY.to_string(),
            // The SDK canary marks its degradation in the profile string; the
            // fact vocabulary itself never degrades.
            compatibility_profile: if degraded {
                format!("{}+sdk-canary", self.inner.declared_version())
            } else {
                self.inner.declared_version().to_string()
            },
            // DSH 0.1.3-alpha.1 honest profile: no steering into a running
            // turn, Halo owns turn queueing, no native entry read on the ACP
            // wire in P0, and no native sandbox-mode enumeration.
            steer: false,
            queue_events: false,
            approval_channel: !degraded,
            entry_read: false,
            native_sandbox_modes: false,
        }
    }

    fn sandbox_facts(&self) -> ManagedExecutorSandboxFacts {
        // dsh-base defaults its sandbox policy to workspace-write; on Windows
        // the restricted-token ACL self-reports partial enforcement (research
        // section 4.2). Reported as-is, never upgraded to full.
        ManagedExecutorSandboxFacts {
            mode: ManagedExecutorSandboxMode::WorkspaceWrite,
            enforcement: ManagedExecutorSandboxEnforcement::Partial,
        }
    }

    async fn prompt(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        self.inner.prompt_turn(&request, false).await?;
        self.emit_user_message_committed(&request.target.session_id, &request.content);
        Ok(())
    }

    async fn follow_up(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        self.inner.prompt_turn(&request, true).await?;
        self.emit_user_message_committed(&request.target.session_id, &request.content);
        Ok(())
    }

    async fn abort(&self, target: ManagedExecutorTarget) -> PortResult<ManagedExecutorAbortOutcome> {
        self.inner.abort_turn(&target).await
    }

    async fn read_entries(
        &self,
        _target: ManagedExecutorTarget,
    ) -> PortResult<halo_runtime_ports::ManagedExecutorEntryPage> {
        // Honest capability gap: the anchored ACP wire exposes no committed
        // entry read. The capability profile declares entry_read=false so the
        // UI degrades instead of this path being invented.
        Err(PortError::new(
            PortErrorKind::NotAvailable,
            "the DSH channel has no Halo-owned committed-entry read in P0",
        ))
    }

    async fn resolve_approval(&self, decision: ManagedExecutorApprovalDecision) -> PortResult<()> {
        if self.inner.channel().is_degraded_canary() {
            // The degraded channel has no approval wire; a decision is never
            // fabricated to keep the fact chain moving.
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "the degraded DSH sdk canary channel has no approval wire; the decision was not forwarded",
            ));
        }
        match decision.outcome {
            halo_runtime_ports::ManagedExecutorApprovalOutcome::AllowedOnce
            | halo_runtime_ports::ManagedExecutorApprovalOutcome::Rejected
            | halo_runtime_ports::ManagedExecutorApprovalOutcome::Cancelled => {
                self.inner
                    .send_approval_decision(&decision.target, &decision.call_id, decision.outcome)
                    .await
            }
            halo_runtime_ports::ManagedExecutorApprovalOutcome::Unavailable => Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "dsh cannot express this approval outcome; the decision was not forwarded",
            )),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<ManagedExecutorEvent> {
        self.events.subscribe()
    }
}
