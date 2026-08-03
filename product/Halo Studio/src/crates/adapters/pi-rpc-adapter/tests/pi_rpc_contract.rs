use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bitfun_pi_rpc_adapter::{
    MemoryPiCredentialStore, MemoryPiRuntimeConfigurationRepository, PiRpcAdapter, PiRpcConfig,
    PiRuntimeConfigurationService,
};
use bitfun_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiRpcAvailabilitySummary, PiRpcCapability,
    PiRpcCommand, PiRpcCompatibilityProfile, PiRpcEvent, PiRpcFailureKind, PiRpcOperationDecision,
    PiRpcPort, PiRpcReply, PiRpcSessionMode, PiRpcVersion, PiRpcVersionEvidenceSource,
    PiRpcWorkspace, PiRuntimeConfiguration, PiStartupOptions, PiThinkingLevel, PortErrorKind,
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
        runtime_configuration: None,
        credential_store: None,
        provider_capabilities: None,
        temporary_root: None,
        response_timeout,
        operation_timeout,
        abort_grace_period,
    })
}

fn fixture_extension_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest_dir.join("src").join("halo_permission_gate.ts"),
        manifest_dir
            .join("..")
            .join("src")
            .join("halo_permission_gate.ts"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .expect("first-party permission extension is present")
}

fn configured_workspace(root: &Path) -> PiRpcWorkspace {
    PiRpcWorkspace {
        workspace_id: "configured-workspace".to_string(),
        canonical_root: root.to_path_buf(),
    }
}

fn configured_configuration(credential_ref: String) -> PiRuntimeConfiguration {
    PiRuntimeConfiguration {
        provider_id: "openai".to_string(),
        base_url: Some("https://api.example.test/v1".to_string()),
        model_id: "gpt-5".to_string(),
        thinking_level: PiThinkingLevel::Medium,
        startup_options: PiStartupOptions::default(),
        credential_ref,
    }
}

fn configured_adapter(
    configuration: Arc<PiRuntimeConfigurationService>,
    credentials: Arc<MemoryPiCredentialStore>,
    storage_root: &Path,
) -> PiRpcAdapter {
    PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(fixture_extension_path()),
        provider: None,
        model: None,
        runtime_configuration: Some(configuration),
        credential_store: Some(credentials),
        provider_capabilities: None,
        temporary_root: Some(storage_root.to_path_buf()),
        response_timeout: Duration::from_secs(1),
        operation_timeout: Duration::from_secs(1),
        abort_grace_period: Duration::from_millis(100),
    })
}

#[tokio::test]
async fn version_probe_uses_private_config_and_cleans_it_on_success_or_failure() {
    for (index, mode) in ["version_probe_requires_isolation", "version_probe_failure"]
        .into_iter()
        .enumerate()
    {
        let _environment = fixture_environment(mode);
        let storage_root = tempfile::tempdir().expect("adapter storage root");
        let adapter = PiRpcAdapter::with_config(PiRpcConfig {
            executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
            temporary_root: Some(storage_root.path().to_path_buf()),
            ..PiRpcConfig::default()
        });

        let reply = adapter
            .execute(PiRpcCommand::Probe {
                generation: index as u64,
                workspace: configured_workspace(storage_root.path()),
            })
            .await
            .expect("version probe crosses the public port");
        if mode == "version_probe_failure" {
            assert_eq!(
                reply,
                PiRpcReply::Unavailable {
                    reason: PiRpcFailureKind::UnsupportedVersion,
                },
                "fixture mode {mode}"
            );
        } else {
            assert_eq!(
                reply,
                PiRpcReply::Available {
                    summary: PiRpcAvailabilitySummary::new(
                        PiRpcVersion::V0_81_1,
                        PiRpcVersionEvidenceSource::LocalVersionProbe,
                    ),
                },
                "fixture mode {mode}"
            );
        }
        assert_eq!(
            std::fs::read_dir(storage_root.path())
                .expect("adapter storage root remains inspectable")
                .count(),
            0,
            "version probe config directory must be cleaned for {mode}"
        );
    }
}

#[tokio::test]
async fn version_probe_rejects_unknown_or_malformed_versions() {
    for mode in ["version_probe_unknown", "version_probe_malformed"] {
        let _environment = fixture_environment(mode);
        let storage_root = tempfile::tempdir().expect("adapter storage root");
        let adapter = PiRpcAdapter::with_config(PiRpcConfig {
            executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
            temporary_root: Some(storage_root.path().to_path_buf()),
            ..PiRpcConfig::default()
        });

        assert_eq!(
            adapter
                .execute(PiRpcCommand::Probe {
                    generation: 200,
                    workspace: configured_workspace(storage_root.path()),
                })
                .await
                .expect("version probe crosses the public port"),
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::UnsupportedVersion,
            },
            "fixture mode {mode} must fail closed"
        );
        assert_eq!(
            std::fs::read_dir(storage_root.path())
                .expect("adapter storage root remains inspectable")
                .count(),
            0,
            "version probe directories must be cleaned for {mode}"
        );
    }
}

#[tokio::test]
async fn public_probe_projects_only_safe_version_and_capability_profile_fields() {
    let _environment = fixture_environment("version_probe_0830");
    let storage_root = tempfile::tempdir().expect("adapter storage root");
    let adapter = PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        temporary_root: Some(storage_root.path().to_path_buf()),
        ..PiRpcConfig::default()
    });

    let reply = adapter
        .execute(PiRpcCommand::Probe {
            generation: 831,
            workspace: configured_workspace(storage_root.path()),
        })
        .await
        .expect("safe public probe crosses the port");

    let summary = match reply {
        PiRpcReply::Available { summary } => summary,
        other => panic!("0.83.0 profile must be available, got {other:?}"),
    };
    assert_eq!(summary.version.version, PiRpcVersion::V0_83_0);
    assert_eq!(
        summary.version.profile,
        PiRpcCompatibilityProfile::PiRpc0830P0
    );
    assert_eq!(
        summary.capabilities.required,
        PiRpcCapability::required_p0().to_vec()
    );
    let wire = serde_json::to_string(&summary).expect("summary serializes");
    assert!(wire.len() < 2048, "public profile summary stays bounded");
    for sensitive in [
        "pi_rpc_fixture",
        "session-contract",
        "entry-1",
        "raw-secret",
        "toolCallId",
        "Authorization",
        "HALO_PI_CREDENTIAL",
        "PI_CODING_AGENT_DIR",
        "api.example.test",
        "models",
        "provider",
        "gpt-5",
        "C:\\",
        "D:\\",
        "http://",
        "https://",
    ] {
        assert!(
            !wire.contains(sensitive),
            "probe summary leaked sensitive field {sensitive}: {wire}"
        );
    }
}

#[tokio::test]
async fn audited_0830_profile_is_allowed_by_start_readiness() {
    let _environment = fixture_environment("version_probe_0830");
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );

    assert_eq!(start(&adapter, 834).await, PiRpcReply::Accepted);
    shutdown(&adapter, 834).await;
}

#[tokio::test]
async fn start_fails_closed_when_a_required_readiness_capability_is_missing() {
    let _environment = fixture_environment("missing_get_entries_capability");
    let adapter = make_adapter(
        Duration::from_millis(250),
        Duration::from_millis(100),
        Duration::from_millis(50),
    );

    assert_eq!(
        start(&adapter, 832).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Protocol,
        }
    );
    shutdown(&adapter, 832).await;
}

#[tokio::test]
async fn agent_end_never_substitutes_for_agent_settled_during_abort() {
    let _environment = fixture_environment("agent_end_without_settled");
    let generation = 833;
    let adapter = make_adapter(
        Duration::from_millis(250),
        Duration::from_millis(100),
        Duration::from_millis(40),
    );
    let mut events = adapter.subscribe();

    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "agent-end-only").await,
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
            .expect("abort crosses the port"),
        PiRpcReply::Accepted
    );
    assert!(
        adapter
            .execute(PiRpcCommand::SendUserInput {
                generation,
                session_id: "session-contract".to_string(),
                content: "after agent_end".to_string(),
            })
            .await
            .is_err(),
        "agent_end must not leave the process reusable as a settled session"
    );
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn configured_start_projects_authority_into_isolated_pi_process_and_cleans_up() {
    let _environment = fixture_environment("credential_projection");
    let workspace_root = tempfile::tempdir().expect("workspace root");
    let storage_root = tempfile::tempdir().expect("adapter storage root");
    let credentials = Arc::new(MemoryPiCredentialStore::new());
    let credential_ref = credentials
        .write(
            "openai",
            PiCredentialSecret::new("synthetic-credential-canary"),
        )
        .await
        .expect("fixture credential is written through the port");
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let configuration = Arc::new(PiRuntimeConfigurationService::new_without_capabilities(
        repository,
    ));
    configuration
        .create(configured_configuration(credential_ref))
        .await
        .expect("fixture configuration is written through the port");
    let adapter = configured_adapter(configuration, credentials, storage_root.path());
    let workspace = configured_workspace(workspace_root.path());

    assert_eq!(
        adapter
            .execute(PiRpcCommand::Start {
                generation: 127,
                workspace,
            })
            .await
            .expect("configured start crosses the port"),
        PiRpcReply::Accepted
    );
    shutdown(&adapter, 127).await;

    assert_eq!(
        std::fs::read_dir(storage_root.path())
            .expect("adapter storage root remains inspectable")
            .count(),
        0,
        "config, session, and extension directories must be cleaned up"
    );
}

#[tokio::test]
async fn configured_start_fails_closed_before_child_creation_for_credential_errors() {
    let _environment = fixture_environment("happy");

    let cases = ["missing", "read_failure", "provider_mismatch"];
    for (index, case) in cases.into_iter().enumerate() {
        let workspace_root = tempfile::tempdir().expect("workspace root");
        let storage_root = tempfile::tempdir().expect("adapter storage root");
        let credentials = Arc::new(MemoryPiCredentialStore::new());
        let credential_ref = if case == "provider_mismatch" {
            credentials
                .write("anthropic", PiCredentialSecret::new("synthetic-secret"))
                .await
                .expect("mismatch fixture credential")
        } else if case == "read_failure" {
            let reference = credentials
                .write("openai", PiCredentialSecret::new("synthetic-secret"))
                .await
                .expect("read failure fixture credential");
            credentials.set_read_failure(true);
            reference
        } else {
            "halo-pi-credential-v1-missing".to_string()
        };
        let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
        let configuration = Arc::new(PiRuntimeConfigurationService::new_without_capabilities(
            repository,
        ));
        configuration
            .create(configured_configuration(credential_ref))
            .await
            .expect("fixture configuration is valid before credential lookup");
        let adapter = configured_adapter(configuration, credentials, storage_root.path());

        assert_eq!(
            adapter
                .execute(PiRpcCommand::Start {
                    generation: 128 + index as u64,
                    workspace: configured_workspace(workspace_root.path()),
                })
                .await
                .expect("credential failure crosses the port"),
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::Authentication,
            },
            "credential case {case} must fail closed"
        );
        assert_eq!(
            std::fs::read_dir(storage_root.path())
                .expect("failed-start storage root remains inspectable")
                .count(),
            0,
            "credential failure must not leave adapter-owned directories"
        );
    }
}

#[tokio::test]
async fn fake_pi_native_model_and_thinking_readiness_mismatches_fail_closed() {
    let _environment = fixture_environment("model_mismatch");
    for (index, mode) in ["model_mismatch", "thinking_mismatch"]
        .into_iter()
        .enumerate()
    {
        std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", mode);
        let workspace_root = tempfile::tempdir().expect("workspace root");
        let storage_root = tempfile::tempdir().expect("adapter storage root");
        let credentials = Arc::new(MemoryPiCredentialStore::new());
        let credential_ref = credentials
            .write("openai", PiCredentialSecret::new("synthetic-secret"))
            .await
            .expect("native readiness fixture credential");
        let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
        let configuration = Arc::new(PiRuntimeConfigurationService::new_without_capabilities(
            repository,
        ));
        configuration
            .create(configured_configuration(credential_ref))
            .await
            .expect("native readiness fixture configuration");
        let adapter = configured_adapter(configuration, credentials, storage_root.path());

        assert_eq!(
            adapter
                .execute(PiRpcCommand::Start {
                    generation: 140 + index as u64,
                    workspace: configured_workspace(workspace_root.path()),
                })
                .await
                .expect("native readiness failure crosses the port"),
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::CapabilityMismatch,
            },
            "fake Pi mode {mode} must fail closed"
        );
        assert_eq!(
            std::fs::read_dir(storage_root.path())
                .expect("readiness-failure storage root remains inspectable")
                .count(),
            0,
            "native readiness failure must clean adapter-owned directories"
        );
    }
}

#[tokio::test]
async fn configured_start_rejects_a_project_pi_directory_before_spawn() {
    let _environment = fixture_environment("happy");
    let workspace_root = tempfile::tempdir().expect("workspace root");
    std::fs::create_dir(workspace_root.path().join(".pi")).expect("project Pi directory");
    let storage_root = tempfile::tempdir().expect("adapter storage root");
    let credentials = Arc::new(MemoryPiCredentialStore::new());
    let credential_ref = credentials
        .write("openai", PiCredentialSecret::new("synthetic-secret"))
        .await
        .expect("project Pi fixture credential");
    let repository = Arc::new(MemoryPiRuntimeConfigurationRepository::new());
    let configuration = Arc::new(PiRuntimeConfigurationService::new_without_capabilities(
        repository,
    ));
    configuration
        .create(configured_configuration(credential_ref))
        .await
        .expect("project Pi fixture configuration");
    let adapter = configured_adapter(configuration, credentials, storage_root.path());

    assert_eq!(
        adapter
            .execute(PiRpcCommand::Start {
                generation: 143,
                workspace: configured_workspace(workspace_root.path()),
            })
            .await
            .expect("project Pi rejection crosses the port"),
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::CapabilityMismatch,
        }
    );
    assert_eq!(
        std::fs::read_dir(storage_root.path())
            .expect("project rejection storage root remains inspectable")
            .count(),
        0
    );
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
async fn malformed_event_schema_fails_closed_without_projecting_public_events() {
    for mode in [
        "malformed_message_update",
        "unsupported_message_update",
        "malformed_tool_execution_end",
        "malformed_extension_ui_request",
    ] {
        let _environment = fixture_environment(mode);
        let generation = 115;
        let adapter = make_adapter(
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_millis(100),
        );
        let mut events = adapter.subscribe();

        assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
        assert_eq!(
            create_session(&adapter, generation).await,
            PiRpcReply::Accepted
        );
        assert_eq!(
            send_input(&adapter, generation, "malformed event").await,
            PiRpcReply::Accepted
        );
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
async fn handshake_rejects_entries_repeated_at_the_since_cursor() {
    let _environment = fixture_environment("bad_since");
    let storage_root = tempfile::tempdir().expect("adapter storage root");
    let adapter = PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(fixture_extension_path()),
        temporary_root: Some(storage_root.path().to_path_buf()),
        response_timeout: Duration::from_millis(250),
        operation_timeout: Duration::from_millis(100),
        abort_grace_period: Duration::from_millis(50),
        ..PiRpcConfig::default()
    });

    assert_eq!(
        adapter
            .execute(PiRpcCommand::Start {
                generation: 116,
                workspace: configured_workspace(storage_root.path()),
            })
            .await
            .expect("readiness crosses the public port"),
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Protocol,
        }
    );
    shutdown(&adapter, 116).await;
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
        runtime_configuration: None,
        credential_store: None,
        provider_capabilities: None,
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
        // Keep the timeout path deterministic on Windows: the fixture still
        // exercises the bounded deny response, without making the assertion
        // depend on a sub-50ms child-process scheduling window.
        Duration::from_millis(250),
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
    // The timeout worker emits OperationResolved after writing the deny
    // response, while the fixture emits AgentSettled in response to that
    // write. Either event may win the broadcast race; retain both instead of
    // discarding the first one while waiting for the second.
    let first = wait_for_event(&mut events, |event| {
        matches!(
            event,
            PiRpcEvent::OperationResolved { .. } | PiRpcEvent::AgentSettled { .. }
        )
    })
    .await;
    let second = wait_for_event(&mut events, |event| {
        matches!(
            event,
            PiRpcEvent::OperationResolved { .. } | PiRpcEvent::AgentSettled { .. }
        )
    })
    .await;
    assert!(matches!(
        (&first, &second),
        (
            PiRpcEvent::OperationResolved { .. },
            PiRpcEvent::AgentSettled { .. }
        ) | (
            PiRpcEvent::AgentSettled { .. },
            PiRpcEvent::OperationResolved { .. }
        )
    ));
    shutdown(&adapter, generation).await;
}
