//! Contract tests for the pi adapter's convergence onto the unified
//! `ManagedExecutorPort` (ADR-0078): command mapping, the closed approval
//! vocabulary, honest capability/sandbox facts, and the executor-neutral
//! fact projection shared with the future DSH adapter (ADR-0080).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use halo_pi_rpc_adapter::{normalize_pi_rpc_event, PiEventNormalization, PiRpcManagedExecutor};
use halo_runtime_ports::{
    ManagedExecutorApprovalDecision, ManagedExecutorApprovalKind, ManagedExecutorApprovalOutcome,
    ManagedExecutorCapabilityProfile, ManagedExecutorEvent, ManagedExecutorKind,
    ManagedExecutorPort, ManagedExecutorPromptRequest, ManagedExecutorSandboxEnforcement,
    ManagedExecutorSandboxFacts, ManagedExecutorSandboxMode, ManagedExecutorTarget,
    ManagedExecutorToolPhase, ManagedEventFactKind, PiRpcAvailabilitySummary, PiRpcCommand,
    PiRpcCompatibilityProfile, PiRpcEvent, PiRpcFailureKind, PiRpcOperationDecision,
    PiRpcOperationKind, PiRpcOperationRiskLevel, PiRpcOperationSummary, PiRpcPort, PiRpcReply,
    PiRpcVersion, PiRpcVersionEvidenceSource, PortErrorKind, PortResult,
    project_managed_executor_event,
};
use tokio::sync::broadcast;

#[derive(Default)]
struct FakePiRpc {
    commands: Mutex<Vec<PiRpcCommand>>,
    replies: Mutex<VecDeque<PortResult<PiRpcReply>>>,
    events: Mutex<Option<broadcast::Sender<PiRpcEvent>>>,
    readiness: Mutex<Option<PiRpcAvailabilitySummary>>,
}

/// Anchors a fake readiness fact on the probed compatibility profile so the
/// wrapper derives its capability profile from real facts.
fn probed_summary(profile: PiRpcCompatibilityProfile) -> PiRpcAvailabilitySummary {
    let version = match profile {
        PiRpcCompatibilityProfile::PiRpc0811P0 => PiRpcVersion::V0_81_1,
        PiRpcCompatibilityProfile::PiRpc0830P0 => PiRpcVersion::V0_83_0,
        PiRpcCompatibilityProfile::PiRpc0850P0 => PiRpcVersion::V0_85_0,
    };
    PiRpcAvailabilitySummary::new(version, PiRpcVersionEvidenceSource::LocalVersionProbe)
}

impl FakePiRpc {
    fn new() -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            commands: Mutex::new(Vec::new()),
            replies: Mutex::new(VecDeque::new()),
            events: Mutex::new(Some(events)),
            readiness: Mutex::new(None),
        }
    }

    fn emit(&self, event: PiRpcEvent) {
        let _ = self
            .events
            .lock()
            .expect("events lock")
            .as_ref()
            .expect("event source")
            .send(event);
    }

    fn commands(&self) -> Vec<PiRpcCommand> {
        self.commands.lock().expect("commands lock").clone()
    }

    fn push_reply(&self, reply: PortResult<PiRpcReply>) {
        self.replies.lock().expect("replies lock").push_back(reply);
    }

    fn probe_profile(&self, profile: PiRpcCompatibilityProfile) {
        *self.readiness.lock().expect("readiness lock") = Some(probed_summary(profile));
    }
}

#[async_trait]
impl PiRpcPort for FakePiRpc {
    async fn execute(&self, command: PiRpcCommand) -> PortResult<PiRpcReply> {
        self.commands.lock().expect("commands lock").push(command);
        self.replies
            .lock()
            .expect("replies lock")
            .pop_front()
            .unwrap_or(Ok(PiRpcReply::Accepted))
    }

    fn subscribe(&self) -> broadcast::Receiver<PiRpcEvent> {
        self.events
            .lock()
            .expect("events lock")
            .as_ref()
            .expect("event source")
            .subscribe()
    }

    fn readiness(&self) -> Option<PiRpcAvailabilitySummary> {
        self.readiness.lock().expect("readiness lock").clone()
    }
}

fn target() -> ManagedExecutorTarget {
    ManagedExecutorTarget {
        task_id: "task-1".to_string(),
        session_id: "session-1".to_string(),
    }
}

async fn executor_with_ready_generation(
    inner: &Arc<FakePiRpc>,
    profile: PiRpcCompatibilityProfile,
) -> PiRpcManagedExecutor {
    inner.probe_profile(profile);
    let executor = PiRpcManagedExecutor::new(inner.clone());
    inner.emit(PiRpcEvent::Ready { generation: 9 });
    // The forwarder translates asynchronously; wait until the generation is
    // observed so the command calls below are deterministic.
    let generation_seen = executor.clone();
    wait_for(move || {
        let executor = generation_seen.clone();
        async move { executor.current_generation().await == Some(9) }
    })
    .await;
    executor
}

async fn wait_for<F, Fut>(mut predicate: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if predicate().await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("condition reached");
}

#[tokio::test]
async fn executor_wrapper_fails_closed_before_the_executor_generation_is_observed() {
    let inner = Arc::new(FakePiRpc::new());
    let executor = PiRpcManagedExecutor::new(inner.clone());

    let error = executor
        .prompt(ManagedExecutorPromptRequest {
            target: target(),
            content: "hello".to_string(),
        })
        .await
        .expect_err("an unobserved generation must fail closed");
    assert_eq!(error.kind, PortErrorKind::NotAvailable);
    assert!(
        inner.commands().is_empty(),
        "fail-closed answers must not reach the executor"
    );
}

#[tokio::test]
async fn executor_wrapper_forwards_prompt_follow_up_abort_and_entry_reads() {
    let inner = Arc::new(FakePiRpc::new());
    let executor =
        executor_with_ready_generation(&inner, PiRpcCompatibilityProfile::PiRpc0830P0).await;
    let _ = executor.subscribe();

    executor
        .prompt(ManagedExecutorPromptRequest {
            target: target(),
            content: "fix the bug".to_string(),
        })
        .await
        .expect("prompt accepted");

    executor
        .follow_up(ManagedExecutorPromptRequest {
            target: target(),
            content: "continue".to_string(),
        })
        .await
        .expect("follow-up accepted");

    let abort = executor.abort(target()).await.expect("abort accepted");
    assert_eq!(
        abort,
        halo_runtime_ports::ManagedExecutorAbortOutcome::Cooperative
    );

    inner.push_reply(Ok(PiRpcReply::Entries {
        entry_count: 4,
        leaf_cursor: Some("cursor-digest".to_string()),
    }));
    let entries = executor.read_entries(target()).await.expect("entries read");
    assert_eq!(entries.entry_count, 4);
    assert_eq!(entries.leaf_cursor.as_deref(), Some("cursor-digest"));

    let commands = inner.commands();
    assert!(matches!(
        &commands[0],
        PiRpcCommand::SendUserInput { content, generation, task_id, session_id }
            if content == "fix the bug" && *generation == 9
                && task_id == "task-1" && session_id == "session-1"
    ));
    assert!(matches!(
        &commands[1],
        PiRpcCommand::FollowUp { content, .. } if content == "continue"
    ));
    assert!(matches!(
        &commands[2],
        PiRpcCommand::AbortSession { .. }
    ));
    assert!(matches!(&commands[3], PiRpcCommand::GetEntries { .. }));
}

#[tokio::test]
async fn executor_wrapper_resolves_approvals_with_the_closed_outcome_vocabulary() {
    let inner = Arc::new(FakePiRpc::new());
    let executor =
        executor_with_ready_generation(&inner, PiRpcCompatibilityProfile::PiRpc0830P0).await;
    let _ = executor.subscribe();

    executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target(),
            call_id: "call-1".to_string(),
            outcome: ManagedExecutorApprovalOutcome::AllowedOnce,
        })
        .await
        .expect("allowed-once forwarded");
    executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target(),
            call_id: "call-2".to_string(),
            outcome: ManagedExecutorApprovalOutcome::Rejected,
        })
        .await
        .expect("rejected forwarded");

    assert!(matches!(
        &inner.commands()[0],
        PiRpcCommand::ResolveOperation { decision: PiRpcOperationDecision::AllowOnce, operation_id, .. }
            if operation_id == "call-1"
    ));
    assert!(matches!(
        &inner.commands()[1],
        PiRpcCommand::ResolveOperation { decision: PiRpcOperationDecision::Deny, .. }
    ));

    let recorded = inner.commands().len();
    for outcome in [
        ManagedExecutorApprovalOutcome::Cancelled,
        ManagedExecutorApprovalOutcome::Unavailable,
    ] {
        let error = executor
            .resolve_approval(ManagedExecutorApprovalDecision {
                target: target(),
                call_id: "call-3".to_string(),
                outcome,
            })
            .await
            .expect_err("outcomes this executor cannot express fail closed");
        assert_eq!(error.kind, PortErrorKind::InvalidRequest);
    }
    assert_eq!(
        inner.commands().len(),
        recorded,
        "fail-closed outcomes never reach the executor"
    );
}

#[tokio::test]
async fn executor_wrapper_declares_pi_p0_capability_and_sandbox_facts_honestly() {
    let inner = Arc::new(FakePiRpc::new());
    inner.probe_profile(PiRpcCompatibilityProfile::PiRpc0830P0);
    let executor = PiRpcManagedExecutor::new(inner.clone());

    let profile = executor.capability_profile();
    assert_eq!(
        profile,
        ManagedExecutorCapabilityProfile {
            adapter_identity: "pi-rpc-p0".to_string(),
            compatibility_profile: "pi-rpc-0.83.0-p0".to_string(),
            // 0.83.0 has not adopted steering or native queue events; pi
            // has no native sandbox mode enumeration.
            steer: false,
            queue_events: false,
            approval_channel: true,
            entry_read: true,
            native_sandbox_modes: false,
        }
    );

    // Pi executes with the developer's full privileges and has no native
    // sandbox; the first-party approval gate only partially constrains it.
    // Reported as-is, never upgraded.
    assert_eq!(
        executor.sandbox_facts(),
        ManagedExecutorSandboxFacts {
            mode: ManagedExecutorSandboxMode::DangerFullAccess,
            enforcement: ManagedExecutorSandboxEnforcement::Partial,
        }
    );
}

#[test]
fn pi_events_normalize_into_the_unified_fact_bearing_vocabulary() {
    let mut state = PiEventNormalization::default();
    state.generation = Some(9);

    // Token-level deltas accumulate as live activity and never emit directly.
    let frames = normalize_pi_rpc_event(
        &PiRpcEvent::MessageUpdated {
            generation: 9,
            session_id: "s".to_string(),
            text: "Solving".to_string(),
        },
        &mut state,
    );
    assert!(frames.is_empty());

    // Settlement is the committed boundary: exactly one reply fact input.
    let settled = normalize_pi_rpc_event(
        &PiRpcEvent::AgentSettled {
            generation: 9,
            session_id: "s".to_string(),
        },
        &mut state,
    );
    assert_eq!(settled.len(), 1);
    assert!(matches!(
        &settled[0],
        ManagedExecutorEvent::AgentReplyCommitted { session_id, summary }
            if session_id == "s" && summary == "Solving"
    ));

    // A second settlement with no new text commits nothing.
    let settled_again = normalize_pi_rpc_event(
        &PiRpcEvent::AgentSettled {
            generation: 9,
            session_id: "s".to_string(),
        },
        &mut state,
    );
    assert!(settled_again.is_empty());

    let cancelled = normalize_pi_rpc_event(
        &PiRpcEvent::SessionStopped {
            generation: 9,
            session_id: "s".to_string(),
            cancellation_mode: halo_runtime_ports::PiRpcCancellationMode::Forced,
        },
        &mut state,
    );
    assert!(matches!(&cancelled[0], ManagedExecutorEvent::Interrupted { session_id } if session_id == "s"));

    let failed = normalize_pi_rpc_event(
        &PiRpcEvent::SessionFailed {
            generation: 9,
            session_id: "s".to_string(),
            reason: PiRpcFailureKind::Protocol,
        },
        &mut state,
    );
    assert!(matches!(
        &failed[0],
        ManagedExecutorEvent::AttemptFailed { session_id, attempt, reason }
            if session_id == "s" && *attempt == 1
                && *reason == halo_runtime_ports::ManagedExecutorFailureKind::Protocol
    ));

    let asked = normalize_pi_rpc_event(
        &PiRpcEvent::OperationRequested {
            generation: 9,
            session_id: "s".to_string(),
            operation_id: "call-5".to_string(),
            kind: PiRpcOperationKind::Permission,
            summary: PiRpcOperationSummary {
                tool_name: "write".to_string(),
                arguments: "[redacted]".to_string(),
                risk_level: PiRpcOperationRiskLevel::HighRisk,
            },
            redacted_tool_call_id: None,
        },
        &mut state,
    );
    assert!(matches!(
        &asked[0],
        ManagedExecutorEvent::ApprovalAsked { call_id, tool_name, risk_level, kind, .. }
            if call_id == "call-5" && tool_name == "write"
                && *risk_level == halo_runtime_ports::ManagedExecutorRiskLevel::HighRisk
                && *kind == ManagedExecutorApprovalKind::Permission
    ));

    // A resolution observed without a forwarded decision is honestly
    // unavailable; one the wrapper forwarded carries its outcome.
    let resolved_unavailable = normalize_pi_rpc_event(
        &PiRpcEvent::OperationResolved {
            generation: 9,
            session_id: "s".to_string(),
            operation_id: "call-5".to_string(),
        },
        &mut state,
    );
    assert!(matches!(
        &resolved_unavailable[0],
        ManagedExecutorEvent::ApprovalDecided { call_id, outcome, .. }
            if call_id == "call-5"
                && *outcome == ManagedExecutorApprovalOutcome::Unavailable
    ));

    state.pending_outcomes.insert(
        "call-6".to_string(),
        ManagedExecutorApprovalOutcome::AllowedOnce,
    );
    let resolved_forwarded = normalize_pi_rpc_event(
        &PiRpcEvent::OperationResolved {
            generation: 9,
            session_id: "s".to_string(),
            operation_id: "call-6".to_string(),
        },
        &mut state,
    );
    assert!(matches!(
        &resolved_forwarded[0],
        ManagedExecutorEvent::ApprovalDecided { call_id, outcome, .. }
            if call_id == "call-6"
                && *outcome == ManagedExecutorApprovalOutcome::AllowedOnce
    ));
}

#[test]
fn pi_events_and_simulated_dsh_events_project_to_identical_fact_kinds() {
    // The same scenario as the runtime-ports neutrality contract, now driven
    // through the REAL pi normalization path. The DSH side simulates the acp
    // profile's committed session updates; both must land on the same
    // executor-neutral fact-kind sequence.

    #[derive(Debug)]
    enum SimulatedDshAcpEvent {
        PromptAccepted { echo: String },
        ToolCallStarted { call_id: String, tool: String },
        PermissionRequested { call_id: String, tool: String },
        PermissionResolved { call_id: String, granted: bool },
        ToolCallCompleted { call_id: String, tool: String, failed: bool },
        AgentMessageCommitted { text: String },
    }

    fn normalize_simulated_dsh(events: &[SimulatedDshAcpEvent]) -> Vec<ManagedExecutorEvent> {
        events
            .iter()
            .filter_map(|event| match event {
                SimulatedDshAcpEvent::PromptAccepted { echo } => {
                    Some(ManagedExecutorEvent::UserMessageCommitted {
                        session_id: "s".to_string(),
                        summary: echo.clone(),
                    })
                }
                SimulatedDshAcpEvent::ToolCallStarted { call_id, tool } => {
                    Some(ManagedExecutorEvent::ToolActivityCommitted {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        phase: ManagedExecutorToolPhase::Started,
                        tool_name: tool.clone(),
                        is_error: false,
                    })
                }
                SimulatedDshAcpEvent::PermissionRequested { call_id, tool } => {
                    Some(ManagedExecutorEvent::ApprovalAsked {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        kind: ManagedExecutorApprovalKind::Permission,
                        tool_name: tool.clone(),
                        redacted_arguments: "[redacted]".to_string(),
                        risk_level: halo_runtime_ports::ManagedExecutorRiskLevel::Standard,
                    })
                }
                SimulatedDshAcpEvent::PermissionResolved { call_id, granted } => {
                    Some(ManagedExecutorEvent::ApprovalDecided {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        outcome: if *granted {
                            ManagedExecutorApprovalOutcome::AllowedOnce
                        } else {
                            ManagedExecutorApprovalOutcome::Rejected
                        },
                    })
                }
                SimulatedDshAcpEvent::ToolCallCompleted {
                    call_id,
                    tool,
                    failed,
                } => Some(ManagedExecutorEvent::ToolActivityCommitted {
                    session_id: "s".to_string(),
                    call_id: call_id.clone(),
                    phase: ManagedExecutorToolPhase::Ended,
                    tool_name: tool.clone(),
                    is_error: *failed,
                }),
                SimulatedDshAcpEvent::AgentMessageCommitted { text } => {
                    Some(ManagedExecutorEvent::AgentReplyCommitted {
                        session_id: "s".to_string(),
                        summary: text.clone(),
                    })
                }
            })
            .collect()
    }

    // pi path: prompt acceptance surfaces through the wrapper reply path, the
    // rest through the real protocol event normalization.
    let mut state = PiEventNormalization::default();
    let mut pi_normalized = vec![ManagedExecutorEvent::UserMessageCommitted {
        session_id: "s".to_string(),
        summary: "fix the bug".to_string(),
    }];
    for event in [
        PiRpcEvent::MessageUpdated {
            generation: 9,
            session_id: "s".to_string(),
            text: "fixing".to_string(),
        },
        PiRpcEvent::ToolExecutionStarted {
            generation: 9,
            session_id: "s".to_string(),
            redacted_tool_call_id: "c1".to_string(),
            tool_name: "edit".to_string(),
        },
        PiRpcEvent::OperationRequested {
            generation: 9,
            session_id: "s".to_string(),
            operation_id: "c2".to_string(),
            kind: PiRpcOperationKind::Permission,
            summary: PiRpcOperationSummary {
                tool_name: "bash".to_string(),
                arguments: "[redacted]".to_string(),
                risk_level: PiRpcOperationRiskLevel::Standard,
            },
            redacted_tool_call_id: None,
        },
        PiRpcEvent::OperationResolved {
            generation: 9,
            session_id: "s".to_string(),
            operation_id: "c2".to_string(),
        },
        PiRpcEvent::ToolExecutionEnded {
            generation: 9,
            session_id: "s".to_string(),
            redacted_tool_call_id: "c1".to_string(),
            tool_name: "edit".to_string(),
            is_error: false,
        },
        PiRpcEvent::AgentSettled {
            generation: 9,
            session_id: "s".to_string(),
        },
    ] {
        pi_normalized.extend(normalize_pi_rpc_event(&event, &mut state));
    }

    let dsh_normalized = normalize_simulated_dsh(&[
        SimulatedDshAcpEvent::PromptAccepted { echo: "fix the bug".to_string() },
        SimulatedDshAcpEvent::ToolCallStarted { call_id: "c1".to_string(), tool: "edit".to_string() },
        SimulatedDshAcpEvent::PermissionRequested { call_id: "c2".to_string(), tool: "bash".to_string() },
        SimulatedDshAcpEvent::PermissionResolved { call_id: "c2".to_string(), granted: true },
        SimulatedDshAcpEvent::ToolCallCompleted { call_id: "c1".to_string(), tool: "edit".to_string(), failed: false },
        SimulatedDshAcpEvent::AgentMessageCommitted { text: "fixing".to_string() },
    ]);

    let project_kinds = |events: &[ManagedExecutorEvent]| -> Vec<ManagedEventFactKind> {
        events
            .iter()
            .flat_map(|event| {
                project_managed_executor_event("task-1", event)
                    .into_iter()
                    .map(|draft| draft.kind)
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    assert_eq!(
        project_kinds(&pi_normalized),
        vec![
            ManagedEventFactKind::UserMessageSummary,
            ManagedEventFactKind::ToolActivity,
            ManagedEventFactKind::AgentOperationRequest,
            ManagedEventFactKind::AgentOperationDecision,
            ManagedEventFactKind::ToolActivity,
            ManagedEventFactKind::AgentReplySummary,
        ]
    );
    assert_eq!(project_kinds(&pi_normalized), project_kinds(&dsh_normalized));
}

#[tokio::test]
async fn executor_wrapper_streams_translated_events_to_subscribers() {
    let inner = Arc::new(FakePiRpc::new());
    let executor =
        executor_with_ready_generation(&inner, PiRpcCompatibilityProfile::PiRpc0830P0).await;
    let mut rx = executor.subscribe();

    executor
        .prompt(ManagedExecutorPromptRequest {
            target: target(),
            content: "hello".to_string(),
        })
        .await
        .expect("prompt accepted");
    inner.emit(PiRpcEvent::MessageUpdated {
        generation: 9,
        session_id: "s".to_string(),
        text: "partial".to_string(),
    });
    inner.emit(PiRpcEvent::AgentSettled {
        generation: 9,
        session_id: "s".to_string(),
    });

    let mut seen = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    let settled = matches!(
                        &event,
                        ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "partial"
                    );
                    seen.push(event);
                    if settled {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                Err(_) => break,
            }
        }
    })
    .await
    .expect("translated events arrive");

    assert!(seen.iter().any(|event| matches!(
        event,
        ManagedExecutorEvent::UserMessageCommitted { summary, .. } if summary == "hello"
    )));
    // The streamed frame itself never appears: only the settled reply does.
    assert!(!seen.iter().any(|event| matches!(
        event,
        ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "partialpartial"
    )));
}

#[tokio::test]
async fn the_adopted_0850_profile_declares_steering_and_queue_event_capabilities() {
    let inner = Arc::new(FakePiRpc::new());
    inner.probe_profile(PiRpcCompatibilityProfile::PiRpc0850P0);
    let executor = PiRpcManagedExecutor::new(inner.clone());

    let profile = executor.capability_profile();
    assert_eq!(profile.adapter_identity, "pi-rpc-p0");
    assert_eq!(
        profile.compatibility_profile,
        "pi-rpc-0.85.0-p0",
        "the 0.85.0 archive is selected by the readiness fact"
    );
    assert!(profile.steer, "steer is adopted by the 0.85.0 profile");
    assert!(
        profile.queue_events,
        "queue_update projection is adopted by the 0.85.0 profile"
    );
    assert!(profile.approval_channel && profile.entry_read);
    assert!(!profile.native_sandbox_modes);
}

#[tokio::test]
async fn an_unprobed_executor_fails_closed_on_every_capability_claim() {
    let inner = Arc::new(FakePiRpc::new());
    let executor = PiRpcManagedExecutor::new(inner.clone());

    let profile = executor.capability_profile();
    assert_eq!(profile.compatibility_profile, "unprobed");
    assert!(!profile.steer && !profile.queue_events);
    assert!(!profile.approval_channel && !profile.entry_read);
}

#[tokio::test]
async fn steering_forwards_the_pi_steer_command_for_the_adopted_profile() {
    let inner = Arc::new(FakePiRpc::new());
    inner.probe_profile(PiRpcCompatibilityProfile::PiRpc0850P0);
    let executor =
        executor_with_ready_generation(&inner, PiRpcCompatibilityProfile::PiRpc0850P0).await;

    executor
        .steer(ManagedExecutorPromptRequest {
            target: target(),
            content: "stop and fix the failing test first".to_string(),
        })
        .await
        .expect("the adopted profile forwards steering");

    assert!(
        matches!(inner.commands().last(), Some(PiRpcCommand::Steer { .. })),
        "steering must cross as the Pi steer command"
    );
}

#[tokio::test]
async fn steering_fails_closed_without_touching_the_executor_for_unadopted_profiles() {
    let inner = Arc::new(FakePiRpc::new());
    inner.probe_profile(PiRpcCompatibilityProfile::PiRpc0830P0);
    let executor =
        executor_with_ready_generation(&inner, PiRpcCompatibilityProfile::PiRpc0830P0).await;

    let error = executor
        .steer(ManagedExecutorPromptRequest {
            target: target(),
            content: "too early for this profile".to_string(),
        })
        .await
        .expect_err("an unadopted profile must refuse steering");
    assert_eq!(error.kind, PortErrorKind::InvalidRequest);
    assert!(
        inner.commands().is_empty(),
        "refused steering must not reach the executor"
    );
}

#[test]
fn the_pi_execution_surface_never_consumes_the_bash_escape_hatch() {
    // ADR-0078 M3 guard: `bash` and `abort_bash` are Pi's host-side shell
    // escape hatch. The consumed-command allowlist the adapter enforces at
    // its single outgoing chokepoint must never contain them.
    for command in halo_pi_rpc_adapter::PI_RPC_CONSUMED_COMMAND_TYPES {
        assert_ne!(*command, "bash", "bash must never be a consumed command");
        assert_ne!(
            *command, "abort_bash",
            "abort_bash must never be a consumed command"
        );
    }
    assert!(halo_pi_rpc_adapter::PI_RPC_CONSUMED_COMMAND_TYPES.contains(&"steer"));
}
