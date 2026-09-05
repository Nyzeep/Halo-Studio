//! Contract tests for the DSH adapter (issue #55, ADR-0078).
//!
//! Every wire interaction runs against the `dsh_acp_fixture` child, mirroring
//! the pi adapter's fake-process contract pattern: handshake/readiness,
//! prompt round-trip, cancel reclaim ladder, one-shot permission decisions,
//! credential env injection without disk footprint, DSH_HOME isolation, and
//! the degraded SDK canary channel keeping the fact chain intact.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use halo_dsh_adapter::{
    DshAdapter, DshChannelKind, DshConfig, DshCredentialRef, DshManagedExecutor,
    MemoryDshCredentialStore, DSH_API_KEY_ENV,
};
use halo_runtime_ports::{
    ManagedExecutorApprovalDecision, ManagedExecutorApprovalOutcome, ManagedExecutorEvent,
    ManagedExecutorPort, ManagedExecutorPromptRequest, ManagedExecutorSandboxEnforcement,
    ManagedExecutorSandboxFacts, ManagedExecutorSandboxMode, ManagedExecutorTarget,
    PortErrorKind,
};
use tokio::sync::broadcast;

const WAIT: Duration = Duration::from_secs(5);

static FIXTURE_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn fixture_environment(mode: &str, channel: &str) -> std::sync::MutexGuard<'static, ()> {
    let guard = FIXTURE_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    env::set_var("HALO_DSH_FIXTURE_MODE", mode);
    env::set_var("HALO_DSH_FIXTURE_CHANNEL", channel);
    guard
}

struct TestSetup {
    adapter: Arc<DshAdapter>,
    executor: DshManagedExecutor,
    events: broadcast::Receiver<ManagedExecutorEvent>,
    // Keeps the adapter-owned directory tree alive for the whole test.
    _temporary: tempfile::TempDir,
    temporary_root: PathBuf,
    workspace: PathBuf,
}

fn make_adapter(
    channel: DshChannelKind,
    customize: impl FnOnce(&mut DshConfig),
) -> TestSetup {
    let temporary = tempfile::tempdir().expect("adapter-owned temporary root");
    let temporary_root = temporary.path().to_path_buf();
    let workspace = temporary_root.join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace directory");
    let mut config = DshConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_dsh_acp_fixture"))),
        channel,
        workspace: Some(workspace.clone()),
        temporary_root: Some(temporary_root.clone()),
        response_timeout: WAIT,
        operation_timeout: WAIT,
        abort_grace_period: Duration::from_millis(300),
        ..Default::default()
    };
    // The controlled child environment is fully replaced by the adapter, so
    // fixture-control variables ride the reviewed extra_environment channel.
    for key in ["HALO_DSH_FIXTURE_MODE", "HALO_DSH_FIXTURE_CHANNEL"] {
        if let Ok(value) = env::var(key) {
            config.extra_environment.insert(key.to_string(), value);
        }
    }
    customize(&mut config);
    let adapter = Arc::new(DshAdapter::with_config(config));
    let executor = DshManagedExecutor::new(adapter.clone());
    let events = executor.subscribe();
    TestSetup {
        adapter,
        executor,
        events,
        _temporary: temporary,
        temporary_root,
        workspace,
    }
}

fn target_for(task_id: &str, session_id: &str) -> ManagedExecutorTarget {
    ManagedExecutorTarget {
        task_id: task_id.to_string(),
        session_id: session_id.to_string(),
    }
}

fn request(target: &ManagedExecutorTarget, content: &str) -> ManagedExecutorPromptRequest {
    ManagedExecutorPromptRequest {
        target: target.clone(),
        content: content.to_string(),
    }
}

async fn collect_events(
    receiver: &mut broadcast::Receiver<ManagedExecutorEvent>,
    count: usize,
) -> Vec<ManagedExecutorEvent> {
    let deadline = Instant::now() + WAIT;
    let mut collected = Vec::new();
    while collected.len() < count {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for executor events");
        match tokio::time::timeout(remaining, receiver.recv()).await {
            Ok(Ok(event)) => collected.push(event),
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            other => panic!("executor event stream closed unexpectedly: {other:?}"),
        }
    }
    collected
}

async fn drain_and_assert_no_facts(receiver: &mut broadcast::Receiver<ManagedExecutorEvent>) {
    tokio::time::sleep(Duration::from_millis(150)).await;
    loop {
        match receiver.try_recv() {
            Ok(event) => panic!("unexpected executor event after reclaim: {event:?}"),
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Closed) => break,
            Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
        }
    }
}

fn scan_tree_for_canary(root: &Path, canary: &str) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if entry.file_name().to_string_lossy().contains(canary) {
                return true;
            }
            if let Ok(content) = std::fs::read(&path) {
                if content.windows(canary.len()).any(|window| window == canary.as_bytes()) {
                    return true;
                }
            }
        }
    }
    false
}

fn asked_call_id(events: &[ManagedExecutorEvent]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            ManagedExecutorEvent::ApprovalAsked { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .expect("approval asked event is observed")
}

#[tokio::test]
async fn initialize_handshake_ready_prompt_round_trip_and_capability_profile() {
    let _guard = fixture_environment("happy", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    setup
        .executor
        .prompt(request(&target, "hello fixture"))
        .await
        .expect("prompt settles on the anchored profile");

    let events = collect_events(&mut setup.events, 4).await;
    let mut tool_started = 0;
    let mut tool_ended = 0;
    let mut reply = None;
    let mut user = None;
    for event in &events {
        match event {
            ManagedExecutorEvent::ToolActivityCommitted { phase, call_id, .. } => {
                assert!(call_id.starts_with("dsh-call-"), "raw ids never cross the port");
                match phase {
                    halo_runtime_ports::ManagedExecutorToolPhase::Started => tool_started += 1,
                    halo_runtime_ports::ManagedExecutorToolPhase::Ended => tool_ended += 1,
                    _ => panic!("unexpected tool phase: {phase:?}"),
                }
            }
            ManagedExecutorEvent::AgentReplyCommitted { summary, .. } => {
                reply = Some(summary.clone())
            }
            ManagedExecutorEvent::UserMessageCommitted { summary, .. } => {
                user = Some(summary.clone())
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert_eq!((tool_started, tool_ended), (1, 1));
    assert_eq!(reply.as_deref(), Some("fixture reply"));
    assert_eq!(user.as_deref(), Some("hello fixture"));

    // M1 fact projection: every collected event projects into fact drafts.
    let drafts = DshManagedExecutor::project_to_facts("task-1", &events);
    assert!(drafts.len() >= 4, "facts must project for the whole chain");

    let profile = setup.executor.capability_profile();
    assert_eq!(profile.adapter_identity, "halo-dsh-adapter");
    assert_eq!(profile.compatibility_profile, "0.1.3-alpha.1");
    assert!(profile.approval_channel);
    assert!(!profile.steer);
    assert!(!profile.queue_events);
    assert!(!profile.entry_read);
    assert!(!profile.native_sandbox_modes);
    let sandbox: ManagedExecutorSandboxFacts = setup.executor.sandbox_facts();
    assert_eq!(sandbox.mode, ManagedExecutorSandboxMode::WorkspaceWrite);
    assert_eq!(sandbox.enforcement, ManagedExecutorSandboxEnforcement::Partial);

    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn unanchored_initialize_fails_closed_with_attempt_fact() {
    let _guard = fixture_environment("unsupported_agent", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let error = setup
        .executor
        .prompt(request(&target, "hello"))
        .await
        .expect_err("unknown agent identity must fail closed");
    assert_eq!(error.kind, PortErrorKind::NotAvailable);
    let events = collect_events(&mut setup.events, 1).await;
    match &events[0] {
        ManagedExecutorEvent::AttemptFailed { attempt, reason, .. } => {
            assert_eq!(*attempt, 1);
            assert_eq!(reason.as_str(), "unsupported_version");
        }
        other => panic!("expected attempt failure, got {other:?}"),
    }
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn drifted_protocol_version_fails_closed() {
    let _guard = fixture_environment("wrong_protocol", "acp");
    let setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let error = setup
        .executor
        .prompt(request(&target, "hello"))
        .await
        .expect_err("drifted protocol version must fail closed");
    assert_eq!(error.kind, PortErrorKind::NotAvailable);
    // An unanchored declared version fails closed before any spawn.
    let unanchored = make_adapter(DshChannelKind::Acp, |config| {
        config.declared_version = "9.9.9".to_string();
    });
    let error = unanchored
        .executor
        .prompt(request(&target, "hello"))
        .await
        .expect_err("unanchored version must fail closed");
    assert_eq!(error.kind, PortErrorKind::NotAvailable);
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn unknown_session_updates_are_filtered_without_breaking_the_chain() {
    let _guard = fixture_environment("unknown_update", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    setup
        .executor
        .prompt(request(&target, "hello"))
        .await
        .expect("unknown update kinds are filtered, never fatal");
    let events = collect_events(&mut setup.events, 4).await;
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AgentReplyCommitted { .. })));
    assert!(!events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AttemptFailed { .. })));
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn request_permission_allow_once_maps_to_the_unified_decision() {
    let _guard = fixture_environment("permission", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let executor = setup.executor.clone();
    let prompt_request = request(&target, "do work");
    let prompt = tokio::spawn(async move { executor.prompt(prompt_request).await });

    let asked = collect_events(&mut setup.events, 1).await;
    let call_id = asked_call_id(&asked);
    match &asked[0] {
        ManagedExecutorEvent::ApprovalAsked {
            call_id,
            tool_name,
            redacted_arguments,
            ..
        } => {
            assert_eq!(tool_name, "write");
            assert!(redacted_arguments.starts_with("dsh-input-"));
            assert!(!redacted_arguments.contains("notes.txt"));
            assert!(!call_id.contains("raw-permission-tool-call"));
        }
        other => panic!("expected approval asked, got {other:?}"),
    }

    setup
        .executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target.clone(),
            call_id: call_id.clone(),
            outcome: ManagedExecutorApprovalOutcome::AllowedOnce,
        })
        .await
        .expect("allow-once decision forwards");

    let settled = collect_events(&mut setup.events, 3).await;
    assert!(settled
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::ApprovalDecided { outcome: ManagedExecutorApprovalOutcome::AllowedOnce, .. })));
    assert!(settled
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "approved reply")));

    prompt
        .await
        .expect("prompt task joins")
        .expect("prompt settles after the one-shot decision");
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn request_permission_reject_once_maps_to_the_unified_decision() {
    let _guard = fixture_environment("permission", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let executor = setup.executor.clone();
    let prompt_request = request(&target, "do work");
    let prompt = tokio::spawn(async move { executor.prompt(prompt_request).await });

    let asked = collect_events(&mut setup.events, 1).await;
    let call_id = asked_call_id(&asked);
    setup
        .executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target.clone(),
            call_id,
            outcome: ManagedExecutorApprovalOutcome::Rejected,
        })
        .await
        .expect("reject-once decision forwards");

    let settled = collect_events(&mut setup.events, 3).await;
    assert!(settled
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::ApprovalDecided { outcome: ManagedExecutorApprovalOutcome::Rejected, .. })));
    assert!(settled
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "rejected reply")));

    prompt
        .await
        .expect("prompt task joins")
        .expect("prompt settles after rejection");
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn request_permission_timeout_audits_unavailable_and_answers_cancelled() {
    let _guard = fixture_environment("permission", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |config| {
        config.operation_timeout = Duration::from_millis(200);
    });
    let target = target_for("task-1", "session-1");
    setup
        .executor
        .prompt(request(&target, "do work"))
        .await
        .expect("unanswered permission resolves fail-closed without breaking the turn");

    let events = collect_events(&mut setup.events, 4).await;
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::ApprovalDecided { outcome: ManagedExecutorApprovalOutcome::Unavailable, .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "no decision")));
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn request_permission_cancelled_outcome_is_recorded_never_inferred() {
    let _guard = fixture_environment("permission", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let executor = setup.executor.clone();
    let prompt_request = request(&target, "do work");
    let prompt = tokio::spawn(async move { executor.prompt(prompt_request).await });

    let asked = collect_events(&mut setup.events, 1).await;
    let call_id = asked_call_id(&asked);
    setup
        .executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target.clone(),
            call_id,
            outcome: ManagedExecutorApprovalOutcome::Cancelled,
        })
        .await
        .expect("cancelled decision forwards");

    let settled = collect_events(&mut setup.events, 3).await;
    assert!(settled
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::ApprovalDecided { outcome: ManagedExecutorApprovalOutcome::Cancelled, .. })));
    assert!(settled
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "no decision")));
    prompt
        .await
        .expect("prompt task joins")
        .expect("prompt settles");
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn unavailable_decisions_never_reach_the_executor() {
    let _guard = fixture_environment("happy", "acp");
    let setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let error = setup
        .executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target.clone(),
            call_id: "dsh-operation-none".to_string(),
            outcome: ManagedExecutorApprovalOutcome::Unavailable,
        })
        .await
        .expect_err("unavailable is not expressible and must not be forwarded");
    assert_eq!(error.kind, PortErrorKind::InvalidRequest);
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn cooperative_cancel_settles_interrupted_once() {
    let _guard = fixture_environment("cancel", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let executor = setup.executor.clone();
    let prompt_request = request(&target, "long work");
    let prompt = tokio::spawn(async move { executor.prompt(prompt_request).await });

    // Wait until the turn is running, then cancel.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let outcome = setup.executor.abort(target.clone()).await.expect("abort");
    assert_eq!(
        outcome,
        halo_runtime_ports::ManagedExecutorAbortOutcome::Cooperative
    );
    // A wire-confirmed cancellation is a legitimate settlement: the turn
    // ends with `Ok`, and the `Interrupted` fact carries the cancellation.
    prompt
        .await
        .expect("prompt task joins")
        .expect("the cancelled turn settles");

    // The user message fact and the interruption fact both land, exactly once.
    let events = collect_events(&mut setup.events, 2).await;
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::Interrupted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::UserMessageCommitted { .. })));
    drain_and_assert_no_facts(&mut setup.events).await;
}

#[tokio::test]
async fn stdin_eof_reclaim_ladder_force_reclaims_stuck_children() {
    let _guard = fixture_environment("hang_prompt", "acp");
    let mut setup = make_adapter(DshChannelKind::Acp, |config| {
        config.abort_grace_period = Duration::from_millis(150);
    });
    let target = target_for("task-1", "session-1");
    let executor = setup.executor.clone();
    let prompt_request = request(&target, "never settles");
    let prompt = tokio::spawn(async move { executor.prompt(prompt_request).await });

    tokio::time::sleep(Duration::from_millis(150)).await;
    let outcome = setup.executor.abort(target.clone()).await.expect("abort");
    assert_eq!(
        outcome,
        halo_runtime_ports::ManagedExecutorAbortOutcome::Reclaimed
    );
    let error = prompt
        .await
        .expect("prompt task joins")
        .expect_err("the force-reclaimed turn fails with Cancelled");
    assert_eq!(error.kind, PortErrorKind::Cancelled);

    let events = collect_events(&mut setup.events, 1).await;
    assert!(matches!(&events[0], ManagedExecutorEvent::Interrupted { .. }));
    drain_and_assert_no_facts(&mut setup.events).await;
}

#[tokio::test]
async fn shutdown_closes_stdin_and_the_child_exits_zero() {
    let _guard = fixture_environment("happy", "acp");
    let dsh_home = tempfile::tempdir().expect("explicit DSH home");
    let sentinel = dsh_home.path().join(".halo-fixture-exit-marker");
    let home_path = dsh_home.path().to_path_buf();
    let setup = make_adapter(DshChannelKind::Acp, |config| {
        config.dsh_home = Some(home_path);
    });
    let target = target_for("task-1", "session-1");
    setup
        .executor
        .prompt(request(&target, "hello"))
        .await
        .expect("prompt settles");
    setup.adapter.shutdown().await;

    // The fixture writes the marker only on its natural post-EOF exit path
    // inside its managed DSH_HOME; a forced reclaim never gets there.
    let deadline = Instant::now() + WAIT;
    loop {
        if std::fs::read_to_string(&sentinel)
            .map(|content| content == "exit-0")
            .unwrap_or(false)
        {
            break;
        }
        assert!(Instant::now() < deadline, "child never exited gracefully");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn credential_env_injection_never_leaves_a_disk_footprint() {
    let _guard = fixture_environment("env_check", "acp");
    let setup = make_adapter(DshChannelKind::Acp, |config| {
        let store = Arc::new(MemoryDshCredentialStore::new());
        store.insert(DSH_API_KEY_ENV, "synthetic-dsh-credential-canary");
        config.credential_ref = Some(
            DshCredentialRef::new(DSH_API_KEY_ENV).expect("credential ref"),
        );
        config.credential_store = Some(store);
    });
    let target = target_for("task-1", "session-1");
    // The env_check fixture exits(2) unless: argv is exactly `--profile acp`,
    // `DSH_HOME` is a managed directory, the credential rides only in the
    // environment, no `.env` exists, and `session/new` cwd equals the child
    // cwd. A settled prompt therefore proves the whole controlled launch.
    setup
        .executor
        .prompt(request(&target, "hello"))
        .await
        .expect("controlled launch with injected credentials passes validation");

    // The workspace projection is the directory the child actually ran in.
    assert!(setup.workspace.exists());
    // Negative test: no managed disk path ever contains the credential value.
    assert!(!scan_tree_for_canary(&setup.temporary_root, "synthetic-dsh-credential-canary"));
    // DSH_HOME isolation: exactly one adapter-owned home under the root.
    let homes = std::fs::read_dir(&setup.temporary_root)
        .expect("temporary root")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("halo-dsh-home-"))
        .count();
    assert_eq!(homes, 1, "the managed DSH_HOME stays inside the adapter root");
    setup.adapter.shutdown().await;
    assert!(!scan_tree_for_canary(&setup.temporary_root, "synthetic-dsh-credential-canary"));
}

#[tokio::test]
async fn sdk_canary_channel_degrades_honestly_and_keeps_the_fact_chain() {
    let _guard = fixture_environment("happy", "sdk");
    let mut setup = make_adapter(DshChannelKind::Sdk, |_| {});
    let target = target_for("task-1", "session-1");
    setup
        .executor
        .prompt(request(&target, "canary prompt"))
        .await
        .expect("the degraded channel still completes a prompt round-trip");

    let events = collect_events(&mut setup.events, 4).await;
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::ToolActivityCommitted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::AgentReplyCommitted { summary, .. } if summary == "canary reply")));
    assert!(events
        .iter()
        .any(|event| matches!(event, ManagedExecutorEvent::UserMessageCommitted { .. })));

    let profile = setup.executor.capability_profile();
    assert!(!profile.approval_channel, "the canary has no approval wire");
    assert_eq!(profile.compatibility_profile, "0.1.3-alpha.1+sdk-canary");
    let error = setup
        .executor
        .resolve_approval(ManagedExecutorApprovalDecision {
            target: target.clone(),
            call_id: "dsh-operation-none".to_string(),
            outcome: ManagedExecutorApprovalOutcome::AllowedOnce,
        })
        .await
        .expect_err("no decision is ever fabricated on the degraded channel");
    assert_eq!(error.kind, PortErrorKind::InvalidRequest);
    setup.adapter.shutdown().await;
}

#[tokio::test]
async fn read_entries_reports_honest_unavailability() {
    let _guard = fixture_environment("happy", "acp");
    let setup = make_adapter(DshChannelKind::Acp, |_| {});
    let target = target_for("task-1", "session-1");
    let error = setup
        .executor
        .read_entries(target)
        .await
        .expect_err("no committed-entry read exists on the anchored wire");
    assert_eq!(error.kind, PortErrorKind::NotAvailable);
    assert!(!setup.executor.capability_profile().entry_read);
}
