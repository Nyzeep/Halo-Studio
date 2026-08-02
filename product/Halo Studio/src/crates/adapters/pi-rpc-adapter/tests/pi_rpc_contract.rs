use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bitfun_pi_rpc_adapter::{PiRpcAdapter, PiRpcConfig};
use bitfun_runtime_ports::{
    PiRpcCommand, PiRpcEvent, PiRpcFailureKind, PiRpcOperationDecision, PiRpcPort, PiRpcReply,
    PiRpcSessionMode, PiRpcWorkspace, PortErrorKind,
};
use tokio::sync::broadcast;
use tokio::time::timeout;

static FIXTURE_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn fixture_environment(mode: &str) -> std::sync::MutexGuard<'static, ()> {
    let guard = FIXTURE_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", mode);
    guard
}

fn make_adapter(
    response_timeout: Duration,
    operation_timeout: Duration,
    abort_grace_period: Duration,
) -> PiRpcAdapter {
    make_adapter_with_selection(
        response_timeout,
        operation_timeout,
        abort_grace_period,
        None,
        None,
    )
}

fn make_adapter_with_selection(
    response_timeout: Duration,
    operation_timeout: Duration,
    abort_grace_period: Duration,
    provider: Option<&str>,
    model: Option<&str>,
) -> PiRpcAdapter {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let extension_path = [
        manifest_dir.join("src").join("halo_permission_gate.ts"),
        manifest_dir
            .join("..")
            .join("src")
            .join("halo_permission_gate.ts"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .expect("first-party permission extension is present");
    PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(extension_path),
        provider: provider.map(str::to_string),
        model: model.map(str::to_string),
        temporary_root: None,
        response_timeout,
        operation_timeout,
        abort_grace_period,
    })
}

#[tokio::test]
async fn provider_and_model_selections_cannot_be_interpreted_as_pi_cli_options() {
    let _environment = fixture_environment("happy");
    let adapter = make_adapter_with_selection(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
        Some("--provider-injection"),
        None,
    );
    assert_eq!(
        start(&adapter, 1).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::CapabilityMismatch,
        }
    );

    let adapter = make_adapter_with_selection(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
        None,
        Some("--model-injection"),
    );
    assert_eq!(
        start(&adapter, 2).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::CapabilityMismatch,
        }
    );
}

fn workspace() -> PiRpcWorkspace {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    PiRpcWorkspace {
        workspace_id: "workspace-contract".to_string(),
        canonical_root: manifest_dir,
    }
}

async fn start(adapter: &PiRpcAdapter, generation: u64) -> PiRpcReply {
    adapter
        .execute(PiRpcCommand::Start {
            generation,
            workspace: workspace(),
        })
        .await
        .expect("start crosses the port without a transport error")
}

async fn create_session(adapter: &PiRpcAdapter, generation: u64) -> PiRpcReply {
    adapter
        .execute(PiRpcCommand::CreateSession {
            generation,
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
            mode: PiRpcSessionMode::Managed,
        })
        .await
        .expect("create session crosses the port without a transport error")
}

async fn send_input(adapter: &PiRpcAdapter, generation: u64, content: &str) -> PiRpcReply {
    adapter
        .execute(PiRpcCommand::SendUserInput {
            generation,
            session_id: "session-contract".to_string(),
            content: content.to_string(),
        })
        .await
        .expect("send input crosses the port without a transport error")
}

async fn shutdown(adapter: &PiRpcAdapter, generation: u64) {
    assert_eq!(
        adapter
            .execute(PiRpcCommand::Shutdown { generation })
            .await
            .expect("shutdown crosses the port without a transport error"),
        PiRpcReply::Accepted
    );
}

async fn wait_for_event<F>(
    receiver: &mut broadcast::Receiver<PiRpcEvent>,
    predicate: F,
) -> PiRpcEvent
where
    F: Fn(&PiRpcEvent) -> bool,
{
    let result = timeout(Duration::from_secs(2), async {
        loop {
            match receiver.recv().await {
                Ok(event) if predicate(&event) => return event,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {
                    panic!("Pi RPC event seam closed before the expected event")
                }
            }
        }
    })
    .await;
    match result {
        Ok(event) => event,
        Err(_) => {
            let mut pending = Vec::new();
            while let Ok(event) = receiver.try_recv() {
                pending.push(event);
            }
            panic!("fake Pi event arrived before the contract timeout; pending={pending:?}");
        }
    }
}

#[tokio::test]
async fn port_projects_crlf_tail_unicode_message_and_tool_events_without_raw_ids() {
    let _environment = fixture_environment("cr");
    let generation = 11;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();

    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert!(matches!(
        wait_for_event(&mut events, |event| matches!(
            event,
            PiRpcEvent::Ready { .. }
        ))
        .await,
        PiRpcEvent::Ready { generation: 11 }
    ));
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "first input").await,
        PiRpcReply::Accepted
    );

    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::MessageUpdated { .. })
    })
    .await;
    let started = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::ToolExecutionStarted { .. })
    })
    .await;
    let updated = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::ToolExecutionUpdated { .. })
    })
    .await;
    let ended = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::ToolExecutionEnded { .. })
    })
    .await;
    for event in [&started, &updated, &ended] {
        let debug = format!("{event:?}");
        assert!(!debug.contains("raw-secret-tool-call-id"));
    }
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::AgentSettled { .. })
    })
    .await;
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn handshake_requires_idle_state_and_requests_entries_since_cursor() {
    let _environment = fixture_environment("require_since");
    let generation = 12;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn child_environment_does_not_inherit_a_secret_canary() {
    let _environment = fixture_environment("env_canary");
    std::env::set_var("HALO_PI_RPC_SECRET_CANARY", "synthetic-secret");
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );

    assert_eq!(start(&adapter, 125).await, PiRpcReply::Accepted);
    std::env::remove_var("HALO_PI_RPC_SECRET_CANARY");
    shutdown(&adapter, 125).await;
}

#[tokio::test]
async fn standard_and_managed_sessions_use_adapter_owned_storage_and_clean_it_up() {
    let _environment = fixture_environment("happy");
    let storage_root = tempfile::tempdir().expect("temporary adapter root");
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let extension_path = manifest_dir.join("src").join("halo_permission_gate.ts");
    let adapter = PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(extension_path),
        provider: None,
        model: None,
        temporary_root: Some(storage_root.path().to_path_buf()),
        response_timeout: Duration::from_secs(1),
        operation_timeout: Duration::from_secs(1),
        abort_grace_period: Duration::from_millis(100),
    });

    assert_eq!(start(&adapter, 126).await, PiRpcReply::Accepted);
    assert_eq!(
        adapter
            .execute(PiRpcCommand::CreateSession {
                generation: 126,
                task_id: "standard-task".to_string(),
                session_id: "standard-session".to_string(),
                mode: PiRpcSessionMode::Standard,
            })
            .await
            .expect("standard session crosses the port"),
        PiRpcReply::Accepted
    );
    assert_eq!(
        adapter
            .execute(PiRpcCommand::CreateSession {
                generation: 126,
                task_id: "managed-task".to_string(),
                session_id: "managed-session".to_string(),
                mode: PiRpcSessionMode::Managed,
            })
            .await
            .expect("managed session crosses the port"),
        PiRpcReply::Accepted
    );
    shutdown(&adapter, 126).await;

    assert_eq!(
        std::fs::read_dir(storage_root.path())
            .expect("storage root remains inspectable")
            .count(),
        0,
        "adapter-owned config/session directories must be removed after shutdown"
    );
}

#[tokio::test]
async fn single_pending_idless_response_remains_supported_by_the_port_contract() {
    let _environment = fixture_environment("idless");
    let generation = 13;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn response_ids_allow_out_of_order_prompt_and_follow_up_replies() {
    let _environment = fixture_environment("out_of_order");
    let generation = 14;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );

    let first = send_input(&adapter, generation, "first");
    let second = send_input(&adapter, generation, "second");
    let (first_reply, second_reply) = tokio::join!(first, second);
    assert_eq!(first_reply, PiRpcReply::Accepted);
    assert_eq!(second_reply, PiRpcReply::Accepted);
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn eof_and_protocol_failures_are_fail_closed_at_the_port() {
    for (mode, reason) in [
        ("eof", PiRpcFailureKind::Transport),
        ("bad_json", PiRpcFailureKind::Protocol),
        ("partial_eof", PiRpcFailureKind::Protocol),
        ("unknown_response", PiRpcFailureKind::Protocol),
        ("unknown_event", PiRpcFailureKind::Protocol),
        ("not_ready", PiRpcFailureKind::Protocol),
        ("bad_entries", PiRpcFailureKind::Protocol),
    ] {
        let _environment = fixture_environment(mode);
        let adapter = make_adapter(
            Duration::from_millis(250),
            Duration::from_millis(100),
            Duration::from_millis(50),
        );
        assert_eq!(
            start(&adapter, 20).await,
            PiRpcReply::Unavailable { reason },
            "fixture mode {mode}"
        );
        shutdown(&adapter, 20).await;
        std::env::remove_var("HALO_PI_RPC_FIXTURE_MODE");
    }
}

#[tokio::test]
async fn failed_prepared_session_handshake_cannot_be_reused_on_retry() {
    for (mode, reason) in [
        ("eof", PiRpcFailureKind::Transport),
        ("bad_json", PiRpcFailureKind::Protocol),
    ] {
        let _environment = fixture_environment(mode);
        let adapter = make_adapter(
            Duration::from_millis(250),
            Duration::from_millis(100),
            Duration::from_millis(50),
        );

        assert_eq!(
            start(&adapter, 21).await,
            PiRpcReply::Unavailable { reason },
            "fixture mode {mode}"
        );

        std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "happy");
        assert_eq!(start(&adapter, 21).await, PiRpcReply::Accepted);
        assert_eq!(create_session(&adapter, 21).await, PiRpcReply::Accepted);
        shutdown(&adapter, 21).await;
        std::env::remove_var("HALO_PI_RPC_FIXTURE_MODE");
    }
}

#[tokio::test]
async fn prepared_child_failure_after_start_fences_the_generation() {
    let _environment = fixture_environment("ready_then_eof");
    let generation = 23;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();

    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert!(matches!(
        wait_for_event(&mut events, |event| matches!(
            event,
            PiRpcEvent::Failed {
                generation: 23,
                reason: PiRpcFailureKind::Transport,
            }
        ))
        .await,
        PiRpcEvent::Failed { .. }
    ));
    assert_eq!(
        start(&adapter, generation).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Transport,
        }
    );
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Transport,
        }
    );
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn repeated_start_is_idempotent_and_repeated_create_is_rejected_safely() {
    let _environment = fixture_environment("happy");
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );

    assert_eq!(start(&adapter, 22).await, PiRpcReply::Accepted);
    assert_eq!(start(&adapter, 22).await, PiRpcReply::Accepted);
    assert_eq!(create_session(&adapter, 22).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, 22).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Internal
        }
    );
    shutdown(&adapter, 22).await;
}

#[tokio::test]
async fn graceful_abort_waits_for_agent_settled_and_stuck_abort_reclaims_child() {
    let _environment = fixture_environment("graceful_abort");
    let generation = 30;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(250),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "running").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionRunning { .. })
    })
    .await;
    assert_eq!(
        adapter
            .execute(PiRpcCommand::StopSession {
                generation,
                session_id: "session-contract".to_string(),
            })
            .await
            .expect("graceful stop crosses the port"),
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionStopped { .. })
    })
    .await;
    shutdown(&adapter, generation).await;

    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "hang_abort");
    let generation = 31;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(30),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "running").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionRunning { .. })
    })
    .await;
    assert_eq!(
        adapter
            .execute(PiRpcCommand::StopSession {
                generation,
                session_id: "session-contract".to_string(),
            })
            .await
            .expect("forced stop crosses the port"),
        PiRpcReply::Accepted
    );
    assert!(adapter
        .execute(PiRpcCommand::SendUserInput {
            generation,
            session_id: "session-contract".to_string(),
            content: "after stop".to_string(),
        })
        .await
        .is_err());
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn abort_response_timeout_cannot_extend_the_forced_reclaim_grace_period() {
    let _environment = fixture_environment("hang_abort_response");
    let generation = 32;
    let adapter = make_adapter(
        Duration::from_millis(500),
        Duration::from_secs(1),
        Duration::from_millis(40),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "running").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionRunning { .. })
    })
    .await;

    let started = Instant::now();
    let result = adapter
        .execute(PiRpcCommand::StopSession {
            generation,
            session_id: "session-contract".to_string(),
        })
        .await;
    assert!(result.is_err(), "missing abort response must fail closed");
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "abort exceeded its hard grace period: {:?}",
        started.elapsed()
    );
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn extension_decision_is_redacted_one_shot_and_duplicate_request_fails_closed() {
    let _environment = fixture_environment("extension");
    let generation = 40;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_millis(500),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "permission").await,
        PiRpcReply::Accepted
    );
    let requested = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::OperationRequested { .. })
    })
    .await;
    let (operation_id, redacted) = match requested {
        PiRpcEvent::OperationRequested {
            operation_id,
            redacted_tool_call_id,
            ..
        } => (
            operation_id,
            redacted_tool_call_id.expect("permission has a redacted id"),
        ),
        _ => unreachable!(),
    };
    assert!(!redacted.contains("raw-secret-permission-id"));
    assert_eq!(
        adapter
            .execute(PiRpcCommand::ResolveOperation {
                generation,
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
                operation_id: operation_id.clone(),
                decision: PiRpcOperationDecision::AllowOnce,
            })
            .await
            .expect("allow decision crosses the port"),
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::AgentSettled { .. })
    })
    .await;
    let second = adapter
        .execute(PiRpcCommand::ResolveOperation {
            generation,
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
            operation_id,
            decision: PiRpcOperationDecision::Deny,
        })
        .await
        .expect_err("the same operation cannot be decided twice");
    assert_eq!(second.kind, PortErrorKind::NotFound);
    assert!(matches!(
        wait_for_event(&mut events, |event| matches!(
            event,
            PiRpcEvent::SessionFailed {
                reason: PiRpcFailureKind::Protocol,
                ..
            }
        ))
        .await,
        PiRpcEvent::SessionFailed { .. }
    ));
    shutdown(&adapter, generation).await;

    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "extension_duplicate");
    let generation = 41;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_millis(500),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "permission").await,
        PiRpcReply::Accepted
    );
    let requested = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::OperationRequested { .. })
    })
    .await;
    let operation_id = match requested {
        PiRpcEvent::OperationRequested { operation_id, .. } => operation_id,
        _ => unreachable!(),
    };
    assert_eq!(
        adapter
            .execute(PiRpcCommand::ResolveOperation {
                generation,
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
                operation_id,
                decision: PiRpcOperationDecision::Deny,
            })
            .await
            .expect("deny decision crosses the port"),
        PiRpcReply::Accepted
    );
    let failed = wait_for_event(&mut events, |event| {
        matches!(
            event,
            PiRpcEvent::SessionFailed {
                reason: PiRpcFailureKind::Protocol,
                ..
            }
        )
    })
    .await;
    assert!(matches!(failed, PiRpcEvent::SessionFailed { .. }));
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn extension_response_id_mismatch_fails_closed() {
    let _environment = fixture_environment("extension");
    let generation = 45;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_millis(500),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "permission").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::OperationRequested { .. })
    })
    .await;

    let error = adapter
        .execute(PiRpcCommand::ResolveOperation {
            generation,
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
            operation_id: "pi-operation-not-the-requested-one".to_string(),
            decision: PiRpcOperationDecision::AllowOnce,
        })
        .await
        .expect_err("an unknown extension operation id is rejected");
    assert_eq!(error.kind, PortErrorKind::NotFound);
    assert!(matches!(
        wait_for_event(&mut events, |event| matches!(
            event,
            PiRpcEvent::SessionFailed {
                reason: PiRpcFailureKind::Protocol,
                ..
            }
        ))
        .await,
        PiRpcEvent::SessionFailed { .. }
    ));
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn stale_shutdown_generation_cannot_reclaim_an_active_pi_session() {
    let _environment = fixture_environment("happy");
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    assert_eq!(start(&adapter, 60).await, PiRpcReply::Accepted);
    assert_eq!(
        adapter
            .execute(PiRpcCommand::Shutdown { generation: 61 })
            .await
            .expect("stale shutdown crosses the port"),
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Transport
        }
    );
    assert_eq!(
        adapter
            .execute(PiRpcCommand::Shutdown { generation: 60 })
            .await
            .expect("active shutdown crosses the port"),
        PiRpcReply::Accepted
    );
}

#[tokio::test]
async fn extension_error_is_a_protocol_failure_and_timeout_decision_is_deny_path() {
    let _environment = fixture_environment("extension_error");
    let generation = 50;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_millis(500),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "extension error").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(
            event,
            PiRpcEvent::SessionFailed {
                reason: PiRpcFailureKind::Protocol,
                ..
            }
        )
    })
    .await;
    shutdown(&adapter, generation).await;

    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "extension_timeout");
    let generation = 51;
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_millis(40),
        Duration::from_millis(100),
    );
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "extension timeout").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::OperationRequested { .. })
    })
    .await;
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::OperationResolved { .. })
    })
    .await;
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::AgentSettled { .. })
    })
    .await;
    shutdown(&adapter, generation).await;
}
