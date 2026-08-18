use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::io::Write as _;
#[cfg(windows)]
use std::process::Stdio;

use halo_pi_rpc_adapter::{
    MemoryPiCredentialStore, MemoryPiRuntimeConfigurationRepository, PiRpcAdapter, PiRpcConfig,
    PiRuntimeConfigurationService,
};
use halo_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiRpcAvailabilitySummary, PiRpcCancellationMode,
    PiRpcCapability, PiRpcCommand, PiRpcCompatibilityProfile, PiRpcEvent, PiRpcFailureKind,
    PiRpcOperationDecision, PiRpcOperationRiskLevel, PiRpcPort, PiRpcReply, PiRpcSessionMode,
    PiRpcVersion, PiRpcVersionEvidenceSource, PiRpcWorkspace, PiRuntimeConfiguration,
    PiStartupOptions, PiThinkingLevel, PortErrorKind,
};
use tokio::sync::broadcast;
use tokio::time::timeout;

#[cfg(windows)]
use tokio::io::{AsyncBufReadExt, BufReader};
#[cfg(windows)]
use tokio::process::Command as TokioCommand;

#[cfg(windows)]
type WindowsHandle = *mut std::ffi::c_void;

#[cfg(windows)]
const PROCESS_TERMINATE: u32 = 0x0001;
#[cfg(windows)]
const SYNCHRONIZE: u32 = 0x0010_0000;
#[cfg(windows)]
const WAIT_TIMEOUT: u32 = 0x0000_0102;

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CloseHandle(handle: WindowsHandle) -> i32;
    fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> WindowsHandle;
    fn TerminateProcess(handle: WindowsHandle, exit_code: u32) -> i32;
    fn WaitForSingleObject(handle: WindowsHandle, milliseconds: u32) -> u32;
}

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
        persistent_session_root: None,
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
        persistent_session_root: None,
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
async fn start_projects_only_safe_runtime_handshake_capabilities_as_verified() {
    let _environment = fixture_environment("version_probe_0830");
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );

    let reply = adapter
        .execute(PiRpcCommand::Start {
            generation: 835,
            workspace: workspace(),
        })
        .await
        .expect("readiness start crosses the port");
    let summary = match reply {
        PiRpcReply::Ready { summary } => summary,
        other => panic!("readiness handshake must return a summary, got {other:?}"),
    };
    assert_eq!(
        summary.capabilities.verified,
        PiRpcCapability::verified_by_readiness_handshake().to_vec()
    );
    shutdown(&adapter, 835).await;
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
async fn start_fails_closed_when_abort_capability_is_missing() {
    let _environment = fixture_environment("missing_abort_capability");
    let adapter = make_adapter(
        Duration::from_millis(250),
        Duration::from_millis(100),
        Duration::from_millis(50),
    );

    assert_eq!(
        start(&adapter, 836).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Protocol,
        }
    );
    shutdown(&adapter, 836).await;
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
                task_id: "session-contract".to_string(),
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
                task_id: "session-contract".to_string(),
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
async fn configured_task_session_projects_authority_after_non_secret_readiness() {
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

    assert!(matches!(
        adapter
            .execute(PiRpcCommand::Start {
                generation: 127,
                workspace,
            })
            .await
            .expect("configured start crosses the port"),
        PiRpcReply::Ready { .. }
    ));
    assert_eq!(
        adapter
            .execute(PiRpcCommand::CreateSession {
                generation: 127,
                task_id: "configured-task".to_string(),
                session_id: "configured-session".to_string(),
                mode: PiRpcSessionMode::Managed,
            })
            .await
            .expect("configured task session crosses the port"),
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
async fn configured_readiness_does_not_read_credentials_before_task_session_creation() {
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

        let generation = 128 + index as u64;
        assert!(
            matches!(
                adapter
                    .execute(PiRpcCommand::Start {
                        generation,
                        workspace: configured_workspace(workspace_root.path()),
                    })
                    .await
                    .expect("credential failure crosses the port"),
                PiRpcReply::Ready { .. }
            ),
            "credential case {case} must not block non-secret readiness"
        );
        assert_eq!(
            adapter
                .execute(PiRpcCommand::CreateSession {
                    generation,
                    task_id: format!("credential-{case}-task"),
                    session_id: format!("credential-{case}-session"),
                    mode: PiRpcSessionMode::Managed,
                })
                .await
                .expect("task session creation crosses the port"),
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::Authentication,
            },
            "credential case {case} must fail closed when the task session is created"
        );
        shutdown(&adapter, generation).await;
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
async fn fake_pi_native_model_and_thinking_task_checks_fail_closed() {
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

        let generation = 140 + index as u64;
        assert!(matches!(
            adapter
                .execute(PiRpcCommand::Start {
                    generation,
                    workspace: configured_workspace(workspace_root.path()),
                })
                .await
                .expect("native readiness failure crosses the port"),
            PiRpcReply::Ready { .. }
        ));
        assert_eq!(
            adapter
                .execute(PiRpcCommand::CreateSession {
                    generation,
                    task_id: format!("native-{mode}-task"),
                    session_id: format!("native-{mode}-session"),
                    mode: PiRpcSessionMode::Managed,
                })
                .await
                .expect("native task validation crosses the port"),
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::CapabilityMismatch,
            },
            "fake Pi mode {mode} must fail closed for the task session"
        );
        shutdown(&adapter, generation).await;
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
async fn configured_task_session_rejects_a_project_pi_directory_before_spawn() {
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

    assert!(matches!(
        adapter
            .execute(PiRpcCommand::Start {
                generation: 143,
                workspace: configured_workspace(workspace_root.path()),
            })
            .await
            .expect("project Pi rejection crosses the port"),
        PiRpcReply::Ready { .. }
    ));
    assert_eq!(
        adapter
            .execute(PiRpcCommand::CreateSession {
                generation: 143,
                task_id: "project-pi-task".to_string(),
                session_id: "project-pi-session".to_string(),
                mode: PiRpcSessionMode::Managed,
            })
            .await
            .expect("project Pi task creation crosses the port"),
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::CapabilityMismatch,
        }
    );
    shutdown(&adapter, 143).await;
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
    assert_eq!(start(&adapter, 1).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, 1).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::CapabilityMismatch,
        }
    );
    shutdown(&adapter, 1).await;

    let adapter = make_adapter_with_selection(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
        None,
        Some("--model-injection"),
    );
    assert_eq!(start(&adapter, 2).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, 2).await,
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::CapabilityMismatch,
        }
    );
    shutdown(&adapter, 2).await;
}

fn workspace() -> PiRpcWorkspace {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    PiRpcWorkspace {
        workspace_id: "workspace-contract".to_string(),
        canonical_root: manifest_dir,
    }
}

async fn start(adapter: &PiRpcAdapter, generation: u64) -> PiRpcReply {
    let reply = adapter
        .execute(PiRpcCommand::Start {
            generation,
            workspace: workspace(),
        })
        .await
        .expect("start crosses the port without a transport error");
    match reply {
        PiRpcReply::Ready { .. } => PiRpcReply::Accepted,
        other => other,
    }
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
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
            content: content.to_string(),
        })
        .await
        .expect("send input crosses the port without a transport error")
}

async fn send_follow_up(adapter: &PiRpcAdapter, generation: u64, content: &str) -> PiRpcReply {
    adapter
        .execute(PiRpcCommand::FollowUp {
            generation,
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
            content: content.to_string(),
        })
        .await
        .expect("follow-up crosses the port without a transport error")
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
async fn port_redacts_assistant_credential_markers_without_stalling() {
    let _environment = fixture_environment("sensitive_message_projection");
    let generation = 116;
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
        send_input(&adapter, generation, "redact assistant output").await,
        PiRpcReply::Accepted
    );

    let text = match wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::MessageUpdated { .. })
    })
    .await
    {
        PiRpcEvent::MessageUpdated { text, .. } => text,
        _ => unreachable!(),
    };
    assert!(text.contains("Bearer [redacted]"));
    assert!(text.contains("Authorization: [redacted]"));
    assert!(text.contains("Cookie: [redacted]"));
    assert!(text.contains("password=[redacted]"));
    assert!(text.contains("token=[redacted]"));
    assert!(text.contains("\"sessionId\":\"[redacted]\""));
    assert!(text.contains("\"entryId\":\"[redacted]\""));
    assert!(text.contains("\"toolCallId\":\"[redacted]\""));
    assert!(text.contains("safe response"));
    for canary in [
        "raw-bearer-token",
        "basic-auth-canary",
        "cookie-canary",
        "message-password-canary",
        "raw-token",
        "sk-live-canary",
        "message-session-id-canary",
        "message-entry-id-canary",
        "message-tool-call-id-canary",
    ] {
        assert!(!text.contains(canary), "assistant text leaked {canary}");
    }
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn port_redacts_sensitive_tool_labels_before_public_events() {
    let _environment = fixture_environment("sensitive_tool_projection");
    let generation = 117;
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
        send_input(&adapter, generation, "redact tool label").await,
        PiRpcReply::Accepted
    );

    let mut labels = Vec::new();
    for _ in 0..3 {
        let event = wait_for_event(&mut events, |event| {
            matches!(
                event,
                PiRpcEvent::ToolExecutionStarted { .. }
                    | PiRpcEvent::ToolExecutionUpdated { .. }
                    | PiRpcEvent::ToolExecutionEnded { .. }
            )
        })
        .await;
        let label = match event {
            PiRpcEvent::ToolExecutionStarted { tool_name, .. }
            | PiRpcEvent::ToolExecutionUpdated { tool_name, .. }
            | PiRpcEvent::ToolExecutionEnded { tool_name, .. } => tool_name,
            _ => unreachable!(),
        };
        labels.push(label);
    }

    for label in labels {
        assert!(label.contains("Authorization: [redacted]"));
        assert!(label.contains("Cookie: [redacted]"));
        assert!(label.contains("password=[redacted]"));
        assert!(label.contains("sessionId=[redacted]"));
        assert!(label.contains("entryId: [redacted]"));
        assert!(label.contains("toolCallId=[redacted]"));
        for canary in [
            "tool-basic-canary",
            "tool-cookie-canary",
            "tool-password-canary",
            "tool-session-id-canary",
            "tool-entry-id-canary",
            "tool-tool-call-id-canary",
        ] {
            assert!(!label.contains(canary), "tool label leaked {canary}");
        }
    }
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
async fn running_session_transport_and_protocol_breaks_fail_closed_without_completion() {
    for (index, (mode, reason)) in [
        ("running_eof", PiRpcFailureKind::Transport),
        ("running_bad_json", PiRpcFailureKind::Protocol),
        ("running_partial_eof", PiRpcFailureKind::Protocol),
        ("running_exit", PiRpcFailureKind::Transport),
    ]
    .into_iter()
    .enumerate()
    {
        let _environment = fixture_environment(mode);
        let generation = 117 + index as u64;
        let storage_root = tempfile::tempdir().expect("adapter storage root");
        let adapter = PiRpcAdapter::with_config(PiRpcConfig {
            executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
            extension_path: Some(fixture_extension_path()),
            temporary_root: Some(storage_root.path().to_path_buf()),
            response_timeout: Duration::from_millis(500),
            operation_timeout: Duration::from_millis(100),
            abort_grace_period: Duration::from_millis(50),
            ..PiRpcConfig::default()
        });
        let mut events = adapter.subscribe();

        assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
        assert_eq!(
            create_session(&adapter, generation).await,
            PiRpcReply::Accepted
        );
        assert_eq!(
            send_input(&adapter, generation, "running interruption").await,
            PiRpcReply::Accepted
        );
        wait_for_event(&mut events, |event| {
            matches!(event, PiRpcEvent::SessionRunning { .. })
        })
        .await;

        let failed = wait_for_event(&mut events, |event| {
            matches!(
                event,
                PiRpcEvent::SessionFailed {
                    reason: observed,
                    ..
                } if *observed == reason
            )
        })
        .await;
        assert!(matches!(failed, PiRpcEvent::SessionFailed { .. }));

        assert!(timeout(Duration::from_millis(100), async {
            loop {
                match events.recv().await {
                    Ok(PiRpcEvent::SessionStopped { .. }) => {
                        panic!("transport/protocol failure must not look like cancellation")
                    }
                    Ok(_) => {}
                    Err(error) => panic!("event stream failed: {error}"),
                }
            }
        })
        .await
        .is_err());

        assert_eq!(
            create_session(&adapter, generation).await,
            PiRpcReply::Accepted,
            "failed session must be removed before a new run can use the same id"
        );
        shutdown(&adapter, generation).await;
        assert_eq!(
            std::fs::read_dir(storage_root.path())
                .expect("adapter-owned temporary root remains inspectable")
                .count(),
            0,
            "running {mode} failure must clean config and extension directories"
        );
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
    let config = PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(extension_path),
        provider: None,
        model: None,
        runtime_configuration: None,
        credential_store: None,
        provider_capabilities: None,
        temporary_root: Some(storage_root.path().to_path_buf()),
        persistent_session_root: None,
        response_timeout: Duration::from_secs(1),
        operation_timeout: Duration::from_secs(1),
        abort_grace_period: Duration::from_millis(100),
    };
    let adapter = PiRpcAdapter::with_config(config.clone());

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
        1,
        "only the persistent standard-session root may remain after shutdown"
    );
    assert!(storage_root.path().join("pi-sessions").is_dir());

    drop(adapter);
    let reopened = PiRpcAdapter::with_config(config);
    assert_eq!(start(&reopened, 127).await, PiRpcReply::Accepted);
    assert_eq!(
        reopened
            .execute(PiRpcCommand::CreateSession {
                generation: 127,
                task_id: "standard-task".to_string(),
                session_id: "reopened-standard-session".to_string(),
                mode: PiRpcSessionMode::Standard,
            })
            .await
            .expect("standard session can be reopened for the same task"),
        PiRpcReply::Accepted
    );
    shutdown(&reopened, 127).await;
    assert_eq!(
        std::fs::read_dir(storage_root.path())
            .expect("persistent root remains after reopen")
            .count(),
        1
    );
}

#[tokio::test]
async fn failed_standard_session_start_removes_only_the_new_empty_task_directory() {
    let _environment = fixture_environment("happy");
    let storage_root = tempfile::tempdir().expect("temporary adapter root");
    let adapter = PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(fixture_extension_path()),
        temporary_root: Some(storage_root.path().to_path_buf()),
        response_timeout: Duration::from_millis(250),
        operation_timeout: Duration::from_millis(100),
        abort_grace_period: Duration::from_millis(50),
        ..PiRpcConfig::default()
    });

    assert_eq!(start(&adapter, 128).await, PiRpcReply::Accepted);
    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "bad_json");
    assert_eq!(
        adapter
            .execute(PiRpcCommand::CreateSession {
                generation: 128,
                task_id: "failed-standard-task".to_string(),
                session_id: "failed-standard-session".to_string(),
                mode: PiRpcSessionMode::Standard,
            })
            .await
            .expect("failed standard create crosses the port"),
        PiRpcReply::Unavailable {
            reason: PiRpcFailureKind::Protocol,
        }
    );

    let workspace_root = storage_root.path().join("pi-sessions").join("workspaces");
    assert_eq!(
        std::fs::read_dir(&workspace_root)
            .expect("persistent workspace root remains inspectable")
            .count(),
        0,
        "failed startup must not leave an empty task directory"
    );

    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "happy");
    shutdown(&adapter, 128).await;
    std::env::remove_var("HALO_PI_RPC_FIXTURE_MODE");
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
async fn follow_up_requires_a_prompt_and_abort_variant_crosses_the_same_seam() {
    let _environment = fixture_environment("graceful_abort");
    let generation = 15;
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

    let error = adapter
        .execute(PiRpcCommand::FollowUp {
            generation,
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
            content: "must not replay before prompt".to_string(),
        })
        .await
        .expect_err("follow-up before a prompt is rejected at the adapter seam");
    assert_eq!(error.kind, PortErrorKind::InvalidRequest);

    assert_eq!(
        send_input(&adapter, generation, "first").await,
        PiRpcReply::Accepted
    );
    wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionRunning { .. })
    })
    .await;
    assert_eq!(
        send_follow_up(&adapter, generation, "continue explicitly").await,
        PiRpcReply::Accepted
    );

    assert_eq!(
        adapter
            .execute(PiRpcCommand::AbortSession {
                generation,
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
            })
            .await
            .expect("explicit abort crosses the port"),
        PiRpcReply::Accepted
    );
    let stopped = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionStopped { .. })
    })
    .await;
    assert!(matches!(
        stopped,
        PiRpcEvent::SessionStopped {
            cancellation_mode: PiRpcCancellationMode::Native,
            ..
        }
    ));
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
async fn readiness_probe_is_not_reused_as_the_task_session() {
    let _environment = fixture_environment("readiness_probe_eof");
    let generation = 23;
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
async fn session_commands_fail_closed_when_the_task_scope_does_not_match() {
    let _environment = fixture_environment("happy");
    let adapter = make_adapter(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(100),
    );
    let generation = 24;
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );

    let error = adapter
        .execute(PiRpcCommand::SendUserInput {
            generation,
            task_id: "another-task".to_string(),
            session_id: "session-contract".to_string(),
            content: "must not cross task boundary".to_string(),
        })
        .await
        .expect_err("cross-task input is denied at the adapter seam");
    assert_eq!(error.kind, PortErrorKind::PermissionDenied);
    shutdown(&adapter, generation).await;
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
                task_id: "session-contract".to_string(),
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
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
            })
            .await
            .expect("forced stop crosses the port"),
        PiRpcReply::Accepted
    );
    let stopped = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionStopped { .. })
    })
    .await;
    assert!(matches!(
        stopped,
        PiRpcEvent::SessionStopped {
            cancellation_mode: PiRpcCancellationMode::Forced,
            ..
        }
    ));
    assert!(adapter
        .execute(PiRpcCommand::SendUserInput {
            generation,
            task_id: "session-contract".to_string(),
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
            task_id: "session-contract".to_string(),
            session_id: "session-contract".to_string(),
        })
        .await;
    assert_eq!(
        result.expect("an abort transport failure is still an accepted forced cancellation"),
        PiRpcReply::Accepted
    );
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "abort exceeded its hard grace period: {:?}",
        started.elapsed()
    );
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn malformed_abort_response_is_forced_cancellation_without_failure_terminal() {
    let _environment = fixture_environment("abort_bad_json");
    let generation = 34;
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

    assert_eq!(
        adapter
            .execute(PiRpcCommand::AbortSession {
                generation,
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
            })
            .await
            .expect("malformed abort response still reaches forced cancellation"),
        PiRpcReply::Accepted
    );

    let stopped = timeout(Duration::from_millis(250), async {
        loop {
            match events.recv().await {
                Ok(PiRpcEvent::SessionStopped {
                    cancellation_mode, ..
                }) => return cancellation_mode,
                Ok(PiRpcEvent::SessionFailed { .. }) => {
                    panic!("abort response failure must not emit a second terminal state")
                }
                Ok(_) => {}
                Err(error) => panic!("event stream failed: {error}"),
            }
        }
    })
    .await
    .expect("forced cancellation emits a stopped event");
    assert_eq!(stopped, PiRpcCancellationMode::Forced);
    assert!(timeout(Duration::from_millis(100), async {
        loop {
            match events.recv().await {
                Ok(PiRpcEvent::SessionFailed { .. }) => {
                    panic!("abort response failure must not emit a second terminal state")
                }
                Ok(_) => {}
                Err(error) => panic!("event stream failed: {error}"),
            }
        }
    })
    .await
    .is_err());
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn forced_abort_emits_only_stopped_and_removes_the_session_registry_entry() {
    let _environment = fixture_environment("hang_abort_response");
    let generation = 33;
    let storage_root = tempfile::tempdir().expect("adapter storage root");
    let adapter = PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(fixture_extension_path()),
        temporary_root: Some(storage_root.path().to_path_buf()),
        response_timeout: Duration::from_millis(500),
        operation_timeout: Duration::from_secs(1),
        abort_grace_period: Duration::from_millis(40),
        ..PiRpcConfig::default()
    });
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
            .execute(PiRpcCommand::AbortSession {
                generation,
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
            })
            .await
            .expect("forced abort crosses the public port"),
        PiRpcReply::Accepted
    );

    let stopped = wait_for_event(&mut events, |event| {
        matches!(event, PiRpcEvent::SessionStopped { .. })
    })
    .await;
    assert!(matches!(
        stopped,
        PiRpcEvent::SessionStopped {
            cancellation_mode: PiRpcCancellationMode::Forced,
            ..
        }
    ));
    assert!(timeout(Duration::from_millis(100), async {
        loop {
            match events.recv().await {
                Ok(PiRpcEvent::SessionFailed { .. }) => {
                    panic!("forced cancellation must not emit a second failure terminal")
                }
                Ok(_) => {}
                Err(error) => panic!("event stream failed: {error}"),
            }
        }
    })
    .await
    .is_err());

    assert_eq!(
        adapter
            .execute(PiRpcCommand::CreateSession {
                generation,
                task_id: "session-contract".to_string(),
                session_id: "session-contract".to_string(),
                mode: PiRpcSessionMode::Managed,
            })
            .await
            .expect("the terminated session registry entry must be removed"),
        PiRpcReply::Accepted,
        "forced abort must remove the session from the adapter registry"
    );
    shutdown(&adapter, generation).await;
    assert_eq!(
        std::fs::read_dir(storage_root.path())
            .expect("adapter-owned temporary root remains inspectable")
            .count(),
        0,
        "forced abort and shutdown must clean config and extension directories"
    );
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
async fn extension_decision_projects_a_redacted_arguments_summary() {
    let _environment = fixture_environment("extension");
    let generation = 42;
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
    let summary = match requested {
        PiRpcEvent::OperationRequested { summary, .. } => summary,
        _ => unreachable!(),
    };
    assert_eq!(summary.tool_name, "write");
    assert_eq!(summary.risk_level, PiRpcOperationRiskLevel::Standard);
    assert!(summary.arguments.contains("[redacted]"));
    assert!(!summary.arguments.contains("/workspace/notes.txt"));
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn extension_sensitive_arguments_are_redacted_from_the_summary() {
    let _environment = fixture_environment("extension_sensitive");
    let generation = 43;
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
    let summary = match requested {
        PiRpcEvent::OperationRequested { summary, .. } => summary,
        _ => unreachable!(),
    };
    assert_eq!(summary.tool_name, "bash");
    assert_eq!(summary.risk_level, PiRpcOperationRiskLevel::Standard);
    for forbidden in [
        "fake-secret",
        "Bearer",
        "/home/nyzee/.ssh/id_rsa",
        "raw-pi-session-id",
        "raw-pi-entry-id",
        "raw-pi-tool-call-id",
        "the-answer-is-42",
    ] {
        assert!(
            !summary.arguments.contains(forbidden),
            "summary leaked {forbidden}: {}",
            summary.arguments
        );
    }
    shutdown(&adapter, generation).await;
}

#[tokio::test]
async fn browser_computer_use_external_side_effect_is_classified_high_risk() {
    let _environment = fixture_environment("extension_high_risk");
    let generation = 44;
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
    let summary = match requested {
        PiRpcEvent::OperationRequested { summary, .. } => summary,
        _ => unreachable!(),
    };
    assert_eq!(summary.tool_name, "browser");
    assert_eq!(summary.risk_level, PiRpcOperationRiskLevel::HighRisk);
    assert!(!summary.arguments.contains("https://example.test/submit"));
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

#[cfg(windows)]
#[tokio::test]
async fn hard_application_termination_reclaims_the_fake_pi_process_tree() {
    if std::env::var_os("HALO_PI_RPC_JOB_HOST").is_some() {
        run_hard_termination_job_host().await;
        return;
    }

    let storage_root = tempfile::tempdir().expect("adapter storage root");
    let mut host = TokioCommand::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("hard_application_termination_reclaims_the_fake_pi_process_tree")
        .arg("--nocapture")
        .env("HALO_PI_RPC_JOB_HOST", "1")
        .env("HALO_PI_RPC_JOB_ROOT", storage_root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hard-termination fixture host");
    let stdout = host.stdout.take().expect("fixture host stdout");
    let mut lines = BufReader::new(stdout).lines();
    let child_pids = match timeout(Duration::from_secs(10), async {
        let mut pids = Vec::new();
        loop {
            let line = lines
                .next_line()
                .await
                .expect("read fixture host stdout")
                .expect("fixture host exited before publishing fake Pi process ids");
            if let Some(pid) = line.strip_prefix("HALO_TEST_CHILD_PID=") {
                pids.push(
                    pid.parse::<u32>()
                        .expect("fixture host published a numeric fake Pi process id"),
                );
                if pids.len() == 2 {
                    return pids;
                }
            }
        }
    })
    .await
    {
        Ok(pids) => pids,
        Err(_) => {
            let _ = host.kill().await;
            let _ = host.wait().await;
            panic!("fixture host did not publish root and descendant fake Pi process ids");
        }
    };
    assert_eq!(child_pids.len(), 2);
    assert_ne!(child_pids[0], child_pids[1]);
    for child_pid in &child_pids {
        assert!(
            windows_process_is_alive(*child_pid),
            "fixture must report live root and descendant fake Pi processes before the host is terminated"
        );
    }

    host.kill().await.expect("force-terminate fixture host");
    host.wait().await.expect("reap fixture host");
    for child_pid in child_pids {
        if !wait_for_windows_process_exit(child_pid).await {
            terminate_fake_fixture_process(child_pid);
            panic!("a fake Pi process survived hard application termination");
        }
    }
}

#[cfg(windows)]
async fn run_hard_termination_job_host() {
    std::env::set_var("HALO_PI_RPC_FIXTURE_MODE", "pid_report_descendant");
    let storage_root =
        PathBuf::from(std::env::var_os("HALO_PI_RPC_JOB_ROOT").expect("fixture host storage root"));
    let adapter = PiRpcAdapter::with_config(PiRpcConfig {
        executable: Some(PathBuf::from(env!("CARGO_BIN_EXE_pi_rpc_fixture"))),
        extension_path: Some(fixture_extension_path()),
        temporary_root: Some(storage_root),
        response_timeout: Duration::from_secs(1),
        operation_timeout: Duration::from_secs(1),
        abort_grace_period: Duration::from_millis(100),
        ..PiRpcConfig::default()
    });
    let generation = 81;
    let mut events = adapter.subscribe();
    assert_eq!(start(&adapter, generation).await, PiRpcReply::Accepted);
    assert_eq!(
        create_session(&adapter, generation).await,
        PiRpcReply::Accepted
    );
    assert_eq!(
        send_input(&adapter, generation, "report child pid").await,
        PiRpcReply::Accepted
    );

    loop {
        match events.recv().await.expect("fixture host event stream") {
            PiRpcEvent::MessageUpdated { text, .. } => {
                let Some(pids) = text.strip_prefix("fixture-pids:") else {
                    continue;
                };
                let Some((root_pid, descendant_pid)) = pids.split_once(',') else {
                    panic!("fixture host published malformed fake Pi process ids");
                };
                println!("HALO_TEST_CHILD_PID={root_pid}");
                println!("HALO_TEST_CHILD_PID={descendant_pid}");
                std::io::stdout()
                    .flush()
                    .expect("flush fixture host fake Pi process ids");
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            }
            _ => {}
        }
    }
}

#[cfg(windows)]
async fn wait_for_windows_process_exit(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while windows_process_is_alive(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    true
}

#[cfg(windows)]
fn windows_process_is_alive(pid: u32) -> bool {
    // SAFETY: the PID was emitted by this test's controlled fake fixture; the
    // returned process handle is checked and closed before this function exits.
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    // SAFETY: `handle` is a valid process handle from `OpenProcess`; a zero
    // timeout only queries state and cannot block the contract test.
    let wait_result = unsafe { WaitForSingleObject(handle, 0) };
    // SAFETY: `handle` is no longer used after this call.
    let _ = unsafe { CloseHandle(handle) };
    wait_result == WAIT_TIMEOUT
}

#[cfg(windows)]
fn terminate_fake_fixture_process(pid: u32) {
    // SAFETY: the PID was emitted by this test's fake fixture. This is an
    // emergency cleanup path after the red assertion, never a production PID.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return;
    }
    // SAFETY: `handle` has PROCESS_TERMINATE access and is closed immediately.
    let _ = unsafe { TerminateProcess(handle, 1) };
    // SAFETY: `handle` is no longer used after this call.
    let _ = unsafe { CloseHandle(handle) };
}
