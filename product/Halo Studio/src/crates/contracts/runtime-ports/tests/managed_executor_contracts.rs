//! Contract tests for the unified `ManagedExecutorPort` seam (ADR-0078) and
//! the executor-neutral fact projection shared by every managed executor
//! adapter (ADR-0080).
//!
//! These tests pin the closed decision vocabulary, the honest capability and
//! sandbox facts, and the fact-kind equivalence of equivalent executor input
//! normalized from differently shaped protocol events.

use halo_runtime_ports::{
    ManagedExecutorAbortOutcome, ManagedExecutorApprovalDecision, ManagedExecutorApprovalKind,
    ManagedExecutorApprovalOutcome, ManagedExecutorCapabilityProfile, ManagedExecutorEvent,
    ManagedExecutorFailureKind, ManagedExecutorPromptRequest, ManagedExecutorRiskLevel,
    ManagedExecutorSandboxEnforcement,
    ManagedExecutorSandboxFacts, ManagedExecutorSandboxMode, ManagedExecutorTarget,
    ManagedExecutorToolPhase, ManagedEventFactKind, normalize_managed_event_summary,
    project_managed_executor_event,
};

fn pi_p0_profile() -> ManagedExecutorCapabilityProfile {
    ManagedExecutorCapabilityProfile {
        adapter_identity: "pi-rpc-p0".to_string(),
        compatibility_profile: "pi-rpc-0.83.0-p0".to_string(),
        steer: false,
        queue_events: false,
        approval_channel: true,
        entry_read: true,
        native_sandbox_modes: false,
    }
}

#[test]
fn capability_profile_flags_are_explicit_and_survive_serialization() {
    let profile = pi_p0_profile();

    assert_eq!(profile.adapter_identity, "pi-rpc-p0");
    // pi 0.83.0 honest profile: an approval channel and entry reads exist,
    // steer waits for the M3 profile upgrade, and turn queueing stays owned
    // by Halo. Declared false must serialize as false, never be upgraded.
    assert!(!profile.steer);
    assert!(!profile.queue_events);
    assert!(profile.approval_channel);
    assert!(profile.entry_read);
    assert!(!profile.native_sandbox_modes);

    let encoded = serde_json::to_string(&profile).expect("profile serialization");
    let decoded: ManagedExecutorCapabilityProfile =
        serde_json::from_str(&encoded).expect("profile deserialization");
    assert_eq!(decoded, profile);
}

#[test]
fn sandbox_contract_is_a_closed_mode_and_enforcement_vocabulary() {
    // The contract layer enumerates executor sandbox modes and reports
    // enforcement honestly; it never introduces an execution backend.
    let facts = ManagedExecutorSandboxFacts {
        mode: ManagedExecutorSandboxMode::DangerFullAccess,
        enforcement: ManagedExecutorSandboxEnforcement::Partial,
    };
    assert_eq!(
        serde_json::to_value(facts).expect("sandbox facts serialization"),
        serde_json::json!({
            "mode": "danger_full_access",
            "enforcement": "partial",
        })
    );

    for mode in [
        ManagedExecutorSandboxMode::ReadOnly,
        ManagedExecutorSandboxMode::WorkspaceWrite,
        ManagedExecutorSandboxMode::DangerFullAccess,
    ] {
        let encoded = serde_json::to_string(&mode).expect("mode serialization");
        let decoded: ManagedExecutorSandboxMode =
            serde_json::from_str(&encoded).expect("mode deserialization");
        assert_eq!(decoded, mode);
    }
}

#[test]
fn approval_outcome_is_the_closed_fail_closed_four_value_vocabulary() {
    // ADR-0078/0012: the decision vocabulary is closed and defaults to the
    // fail-closed outcome. The four-value loop below documents the closed set;
    // adding a value must go through contract review.
    let outcomes = [
        ManagedExecutorApprovalOutcome::AllowedOnce,
        ManagedExecutorApprovalOutcome::Rejected,
        ManagedExecutorApprovalOutcome::Cancelled,
        ManagedExecutorApprovalOutcome::Unavailable,
    ];
    for outcome in outcomes {
        let encoded = serde_json::to_string(&outcome).expect("outcome serialization");
        let decoded: ManagedExecutorApprovalOutcome =
            serde_json::from_str(&encoded).expect("outcome deserialization");
        assert_eq!(decoded, outcome);
    }

    assert_eq!(
        ManagedExecutorApprovalOutcome::default(),
        ManagedExecutorApprovalOutcome::Unavailable
    );

    let summary = serde_json::to_value(ManagedExecutorApprovalKind::Permission)
        .expect("approval kind serialization");
    assert_eq!(summary, serde_json::json!("permission"));
}

#[test]
fn prompt_requests_redact_content_from_debug_formatting() {
    let request = ManagedExecutorPromptRequest {
        target: ManagedExecutorTarget {
            task_id: "task-1".to_string(),
            session_id: "session-1".to_string(),
        },
        content: "secret developer prompt body".to_string(),
    };

    let rendered = format!("{request:?}");
    assert!(!rendered.contains("secret developer prompt body"));
    assert!(rendered.contains("task-1"));
}

#[test]
fn abort_outcome_reports_cooperative_and_reclaimed_cancellation() {
    // Cooperative: the executor acknowledged abort and settled inside the
    // bounded grace period. Reclaimed: the owned transport was closed and the
    // child reclaimed after that grace period or an abort transport failure.
    assert_ne!(
        ManagedExecutorAbortOutcome::Cooperative,
        ManagedExecutorAbortOutcome::Reclaimed
    );
}

#[test]
fn normalized_summaries_redact_sensitive_lines_and_bound_bytes() {
    let value = format!("Authorization: bearer secret\n{}", "界".repeat(300));
    let summary = normalize_managed_event_summary(&value).expect("safe summary boundary");

    assert!(summary.starts_with("[redacted]"));
    assert!(!summary.contains("bearer"));
    assert!(summary.len() <= 512);
    assert!(summary.is_char_boundary(summary.len()));
}

#[test]
fn normalized_summaries_fail_closed_on_raw_like_payloads() {
    for value in [
        "api_key=secret",
        "event jsonl payload",
        "\0 raw payload",
        "\"prompt\": {\"raw\": true}",
        "credential blob",
    ] {
        assert!(
            normalize_managed_event_summary(value).is_err(),
            "unsafe payload {value} must fail closed"
        );
    }
}

#[test]
fn unified_events_project_to_executor_neutral_fact_kinds() {
    let task_id = "task-1";
    let cases: Vec<(ManagedExecutorEvent, ManagedEventFactKind)> = vec![
        (
            ManagedExecutorEvent::UserMessageCommitted {
                session_id: "s".to_string(),
                summary: "hello".to_string(),
            },
            ManagedEventFactKind::UserMessageSummary,
        ),
        (
            ManagedExecutorEvent::AgentReplyCommitted {
                session_id: "s".to_string(),
                summary: "reply".to_string(),
            },
            ManagedEventFactKind::AgentReplySummary,
        ),
        (
            ManagedExecutorEvent::ToolActivityCommitted {
                session_id: "s".to_string(),
                call_id: "call".to_string(),
                phase: ManagedExecutorToolPhase::Started,
                tool_name: "read".to_string(),
                is_error: false,
            },
            ManagedEventFactKind::ToolActivity,
        ),
        (
            ManagedExecutorEvent::ApprovalAsked {
                session_id: "s".to_string(),
                call_id: "call".to_string(),
                kind: ManagedExecutorApprovalKind::Permission,
                tool_name: "write".to_string(),
                redacted_arguments: "{}".to_string(),
                risk_level: ManagedExecutorRiskLevel::Standard,
            },
            ManagedEventFactKind::AgentOperationRequest,
        ),
        (
            ManagedExecutorEvent::ApprovalDecided {
                session_id: "s".to_string(),
                call_id: "call".to_string(),
                outcome: ManagedExecutorApprovalOutcome::Rejected,
            },
            ManagedEventFactKind::AgentOperationDecision,
        ),
        (
            ManagedExecutorEvent::AttemptFailed {
                session_id: "s".to_string(),
                attempt: 1,
                reason: ManagedExecutorFailureKind::Protocol,
            },
            ManagedEventFactKind::AttemptFailed,
        ),
        (
            ManagedExecutorEvent::Interrupted {
                session_id: "s".to_string(),
            },
            ManagedEventFactKind::TaskInterrupted,
        ),
    ];

    for (event, kind) in cases {
        let drafts = project_managed_executor_event(task_id, &event);
        assert_eq!(drafts.len(), 1, "event {event:?} projects one fact draft");
        assert_eq!(drafts[0].kind, kind, "event {event:?} stays kind neutral");
        assert!(!drafts[0].identity.is_empty());
        assert!(drafts[0].redacted_summary.len() <= 512);
    }
}

#[test]
fn approval_audit_pair_associates_asked_and_decided_by_call_id() {
    let asked = project_managed_executor_event(
        "task-1",
        &ManagedExecutorEvent::ApprovalAsked {
            session_id: "s".to_string(),
            call_id: "call-7".to_string(),
            kind: ManagedExecutorApprovalKind::Permission,
            tool_name: "write".to_string(),
            redacted_arguments: "{}".to_string(),
            risk_level: ManagedExecutorRiskLevel::HighRisk,
        },
    )
    .remove(0);
    let decided = project_managed_executor_event(
        "task-1",
        &ManagedExecutorEvent::ApprovalDecided {
            session_id: "s".to_string(),
            call_id: "call-7".to_string(),
            outcome: ManagedExecutorApprovalOutcome::AllowedOnce,
        },
    )
    .remove(0);

    // The asked/decided audit pair must stay correlatable in the fact log by
    // the executor-neutral call id.
    assert!(asked.redacted_summary.contains("call-7"));
    assert!(decided.redacted_summary.contains("call-7"));
    assert_ne!(
        decided.redacted_summary, asked.redacted_summary,
        "decided must record the outcome, not repeat the ask"
    );
    assert!(decided.redacted_summary.contains("allowed_once"));
}

#[test]
fn failed_attempts_are_independently_identified_per_attempt() {
    let first = project_managed_executor_event(
        "task-1",
        &ManagedExecutorEvent::AttemptFailed {
            session_id: "s".to_string(),
            attempt: 1,
            reason: ManagedExecutorFailureKind::Transport,
        },
    )
    .remove(0);
    let second = project_managed_executor_event(
        "task-1",
        &ManagedExecutorEvent::AttemptFailed {
            session_id: "s".to_string(),
            attempt: 2,
            reason: ManagedExecutorFailureKind::Transport,
        },
    )
    .remove(0);

    // Two failed attempts with the same reason stay two facts: attempts are
    // never merged back into one continuous history.
    assert_ne!(first.identity, second.identity);
}

#[test]
fn equivalent_executor_scenarios_project_to_the_same_fact_kind_sequence() {
    // One scenario: prompt accepted, a tool runs, an approval is asked and
    // allowed once, the tool completes and the agent settles with a reply.
    // The scenario is expressed twice: once as pi-shaped RPC events and once
    // as DSH acp-shaped session events. Each goes through its own
    // normalization path into the unified vocabulary; the projected fact-kind
    // sequence must be identical.

    #[derive(Debug)]
    enum PiShapedEvent {
        MessageUpdated { text: String },
        ToolExecutionStarted { call_id: String, tool: String },
        ToolExecutionEnded { call_id: String, tool: String, is_error: bool },
        OperationRequested { call_id: String, tool: String },
        OperationResolved { call_id: String },
        AgentSettled,
    }

    #[derive(Debug)]
    enum DshShapedEvent {
        PromptAccepted { echo: String },
        AgentMessageCommitted { text: String },
        ToolCallStarted { call_id: String, tool: String },
        ToolCallCompleted { call_id: String, tool: String, failed: bool },
        PermissionRequested { call_id: String, tool: String },
        PermissionResolved { call_id: String, granted: bool },
        TurnSettled,
    }

    // pi normalization: settlement (agent_settled) is the committed boundary,
    // so the streamed assistant text only lands as one committed reply fact
    // and token-level update frames never reach the fact vocabulary.
    fn normalize_pi(
        prompt_accepted_summary: Option<&str>,
        events: &[PiShapedEvent],
    ) -> Vec<ManagedExecutorEvent> {
        let mut normalized = Vec::new();
        if let Some(summary) = prompt_accepted_summary {
            normalized.push(ManagedExecutorEvent::UserMessageCommitted {
                session_id: "s".to_string(),
                summary: summary.to_string(),
            });
        }
        let mut streamed = String::new();
        for event in events {
            match event {
                PiShapedEvent::MessageUpdated { text } => streamed.push_str(text),
                PiShapedEvent::ToolExecutionStarted { call_id, tool } => {
                    normalized.push(ManagedExecutorEvent::ToolActivityCommitted {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        phase: ManagedExecutorToolPhase::Started,
                        tool_name: tool.clone(),
                        is_error: false,
                    });
                }
                PiShapedEvent::ToolExecutionEnded {
                    call_id,
                    tool,
                    is_error,
                } => {
                    normalized.push(ManagedExecutorEvent::ToolActivityCommitted {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        phase: ManagedExecutorToolPhase::Ended,
                        tool_name: tool.clone(),
                        is_error: *is_error,
                    });
                }
                PiShapedEvent::OperationRequested { call_id, tool } => {
                    normalized.push(ManagedExecutorEvent::ApprovalAsked {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        kind: ManagedExecutorApprovalKind::Permission,
                        tool_name: tool.clone(),
                        redacted_arguments: "[redacted]".to_string(),
                        risk_level: ManagedExecutorRiskLevel::Standard,
                    });
                }
                PiShapedEvent::OperationResolved { call_id } => {
                    normalized.push(ManagedExecutorEvent::ApprovalDecided {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        outcome: ManagedExecutorApprovalOutcome::AllowedOnce,
                    });
                }
                PiShapedEvent::AgentSettled => {
                    let committed = std::mem::take(&mut streamed);
                    if !committed.is_empty() {
                        normalized.push(ManagedExecutorEvent::AgentReplyCommitted {
                            session_id: "s".to_string(),
                            summary: committed,
                        });
                    }
                }
            }
        }
        normalized
    }

    // DSH acp normalization: prompt responses and agent messages are already
    // committed session updates; the settled turn itself contributes no fact.
    fn normalize_dsh(events: &[DshShapedEvent]) -> Vec<ManagedExecutorEvent> {
        events
            .iter()
            .filter_map(|event| match event {
                DshShapedEvent::PromptAccepted { echo } => {
                    Some(ManagedExecutorEvent::UserMessageCommitted {
                        session_id: "s".to_string(),
                        summary: echo.clone(),
                    })
                }
                DshShapedEvent::AgentMessageCommitted { text } => {
                    Some(ManagedExecutorEvent::AgentReplyCommitted {
                        session_id: "s".to_string(),
                        summary: text.clone(),
                    })
                }
                DshShapedEvent::ToolCallStarted { call_id, tool } => {
                    Some(ManagedExecutorEvent::ToolActivityCommitted {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        phase: ManagedExecutorToolPhase::Started,
                        tool_name: tool.clone(),
                        is_error: false,
                    })
                }
                DshShapedEvent::ToolCallCompleted {
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
                DshShapedEvent::PermissionRequested { call_id, tool } => {
                    Some(ManagedExecutorEvent::ApprovalAsked {
                        session_id: "s".to_string(),
                        call_id: call_id.clone(),
                        kind: ManagedExecutorApprovalKind::Permission,
                        tool_name: tool.clone(),
                        redacted_arguments: "[redacted]".to_string(),
                        risk_level: ManagedExecutorRiskLevel::Standard,
                    })
                }
                DshShapedEvent::PermissionResolved { call_id, granted } => {
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
                DshShapedEvent::TurnSettled => None,
            })
            .collect()
    }

    let pi_normalized = normalize_pi(
        Some("fix the bug"),
        &[
            PiShapedEvent::MessageUpdated { text: "fixing".to_string() },
            PiShapedEvent::ToolExecutionStarted { call_id: "c1".to_string(), tool: "edit".to_string() },
            PiShapedEvent::OperationRequested { call_id: "c2".to_string(), tool: "bash".to_string() },
            PiShapedEvent::OperationResolved { call_id: "c2".to_string() },
            PiShapedEvent::ToolExecutionEnded { call_id: "c1".to_string(), tool: "edit".to_string(), is_error: false },
            PiShapedEvent::AgentSettled,
        ],
    );

    let dsh_normalized = normalize_dsh(&[
        DshShapedEvent::PromptAccepted { echo: "fix the bug".to_string() },
        DshShapedEvent::ToolCallStarted { call_id: "c1".to_string(), tool: "edit".to_string() },
        DshShapedEvent::PermissionRequested { call_id: "c2".to_string(), tool: "bash".to_string() },
        DshShapedEvent::PermissionResolved { call_id: "c2".to_string(), granted: true },
        DshShapedEvent::ToolCallCompleted { call_id: "c1".to_string(), tool: "edit".to_string(), failed: false },
        DshShapedEvent::AgentMessageCommitted { text: "fixing".to_string() },
        DshShapedEvent::TurnSettled,
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

#[test]
fn approval_decisions_carry_the_call_id_and_closed_outcome() {
    let decision = ManagedExecutorApprovalDecision {
        target: ManagedExecutorTarget {
            task_id: "task-1".to_string(),
            session_id: "session-1".to_string(),
        },
        call_id: "call-3".to_string(),
        outcome: ManagedExecutorApprovalOutcome::AllowedOnce,
    };

    assert_eq!(decision.call_id, "call-3");
    assert_eq!(decision.outcome, ManagedExecutorApprovalOutcome::AllowedOnce);
}
