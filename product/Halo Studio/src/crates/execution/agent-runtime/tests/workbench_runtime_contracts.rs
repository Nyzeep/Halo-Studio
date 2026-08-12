use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bitfun_agent_runtime::halo_workbench::{
    HaloWorkbenchActivityStatus, HaloWorkbenchCapability, HaloWorkbenchIntent,
    HaloWorkbenchIntentRequest, HaloWorkbenchMessageRole, HaloWorkbenchOperationDecision,
    HaloWorkbenchPendingOperationPhase, HaloWorkbenchPhase, HaloWorkbenchRuntime,
    HaloWorkbenchSessionMode, HaloWorkbenchSessionPhase, HaloWorkbenchWorkspaceInput,
    HALO_WORKBENCH_SCHEMA_VERSION,
};
use bitfun_runtime_ports::{
    ClockPort, PiProviderReadiness, PiProviderReadinessPort, PiRpcAvailabilitySummary,
    PiRpcCommand, PiRpcEvent, PiRpcFailureKind, PiRpcOperationKind, PiRpcPort, PiRpcReply,
    PiRpcVersion, PiRpcVersionEvidenceSource, PortError, PortErrorKind, PortResult,
    RuntimeServiceCapability, RuntimeServicePort, WorkbenchTaskBaseline, WorkbenchTaskBaselinePort,
    WorkbenchWorkspaceFacts, WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest,
    PI_RPC_ADAPTER_IDENTITY,
};
use tokio::sync::{broadcast, Notify, Semaphore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    Probe,
    Start,
    CreateSession,
    SendUserInput,
    FollowUp,
    StopSession,
    AbortSession,
    EndSession,
    ResolveOperation,
    Shutdown,
}

impl CommandKind {
    fn of(command: &PiRpcCommand) -> Option<Self> {
        match command {
            PiRpcCommand::Probe { .. } => Some(Self::Probe),
            PiRpcCommand::Start { .. } => Some(Self::Start),
            PiRpcCommand::CreateSession { .. } => Some(Self::CreateSession),
            PiRpcCommand::SendUserInput { .. } => Some(Self::SendUserInput),
            PiRpcCommand::FollowUp { .. } => Some(Self::FollowUp),
            PiRpcCommand::StopSession { .. } => Some(Self::StopSession),
            PiRpcCommand::AbortSession { .. } => Some(Self::AbortSession),
            PiRpcCommand::EndSession { .. } => Some(Self::EndSession),
            PiRpcCommand::ResolveOperation { .. } => Some(Self::ResolveOperation),
            PiRpcCommand::Shutdown { .. } => Some(Self::Shutdown),
        }
    }
}

struct CommandGate {
    enabled: AtomicBool,
    started: AtomicBool,
    started_notify: Notify,
    release: Semaphore,
}

impl CommandGate {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            started: AtomicBool::new(false),
            started_notify: Notify::new(),
            release: Semaphore::new(0),
        }
    }

    fn block(&self) {
        self.started.store(false, Ordering::Release);
        self.enabled.store(true, Ordering::Release);
    }

    async fn wait_until_started(&self) {
        while !self.started.load(Ordering::Acquire) {
            self.started_notify.notified().await;
        }
    }

    fn release(&self) {
        self.release.add_permits(1);
    }

    async fn wait_if_enabled(&self) {
        if !self.enabled.swap(false, Ordering::AcqRel) {
            return;
        }
        self.started.store(true, Ordering::Release);
        self.started_notify.notify_waiters();
        self.release
            .acquire()
            .await
            .expect("test command gate remains open")
            .forget();
    }
}

struct DeterministicPiRpc {
    events: Mutex<Option<broadcast::Sender<PiRpcEvent>>>,
    commands: Mutex<Vec<PiRpcCommand>>,
    replies: Mutex<VecDeque<PortResult<PiRpcReply>>>,
    probe_gate: CommandGate,
    start_gate: CommandGate,
    create_session_gate: CommandGate,
    send_user_input_gate: CommandGate,
    stop_session_gate: CommandGate,
    end_session_gate: CommandGate,
    resolve_gate: CommandGate,
    shutdown_gate: CommandGate,
}

impl DeterministicPiRpc {
    fn new() -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            events: Mutex::new(Some(events)),
            commands: Mutex::new(Vec::new()),
            replies: Mutex::new(VecDeque::new()),
            probe_gate: CommandGate::new(),
            start_gate: CommandGate::new(),
            create_session_gate: CommandGate::new(),
            send_user_input_gate: CommandGate::new(),
            stop_session_gate: CommandGate::new(),
            end_session_gate: CommandGate::new(),
            resolve_gate: CommandGate::new(),
            shutdown_gate: CommandGate::new(),
        }
    }

    fn push_reply(&self, reply: PortResult<PiRpcReply>) {
        self.replies.lock().expect("replies lock").push_back(reply);
    }

    fn emit(&self, event: PiRpcEvent) {
        self.events
            .lock()
            .expect("events lock")
            .as_ref()
            .expect("runtime event source is open")
            .send(event)
            .expect("runtime listener is active");
    }

    fn commands(&self) -> Vec<PiRpcCommand> {
        self.commands.lock().expect("commands lock").clone()
    }

    fn clear_commands(&self) {
        self.commands.lock().expect("commands lock").clear();
    }

    fn generation_for(&self, kind: CommandKind) -> u64 {
        self.commands()
            .iter()
            .rev()
            .find_map(|command| match (kind, command) {
                (CommandKind::Probe, PiRpcCommand::Probe { generation, .. })
                | (CommandKind::Start, PiRpcCommand::Start { generation, .. })
                | (CommandKind::Shutdown, PiRpcCommand::Shutdown { generation }) => {
                    Some(*generation)
                }
                _ => None,
            })
            .expect("matching command generation")
    }

    fn count(&self, kind: CommandKind) -> usize {
        self.commands()
            .iter()
            .filter(|command| CommandKind::of(command) == Some(kind))
            .count()
    }

    async fn gate(&self, command: &PiRpcCommand) {
        match CommandKind::of(command) {
            Some(CommandKind::Probe) => self.probe_gate.wait_if_enabled().await,
            Some(CommandKind::Start) => self.start_gate.wait_if_enabled().await,
            Some(CommandKind::CreateSession) => self.create_session_gate.wait_if_enabled().await,
            Some(CommandKind::SendUserInput) => self.send_user_input_gate.wait_if_enabled().await,
            Some(CommandKind::FollowUp) => self.send_user_input_gate.wait_if_enabled().await,
            Some(CommandKind::StopSession) => self.stop_session_gate.wait_if_enabled().await,
            Some(CommandKind::AbortSession) => self.stop_session_gate.wait_if_enabled().await,
            Some(CommandKind::EndSession) => self.end_session_gate.wait_if_enabled().await,
            Some(CommandKind::ResolveOperation) => self.resolve_gate.wait_if_enabled().await,
            Some(CommandKind::Shutdown) => self.shutdown_gate.wait_if_enabled().await,
            None => {}
        }
    }

    fn default_reply(command: &PiRpcCommand) -> PiRpcReply {
        match command {
            PiRpcCommand::Probe { .. } => PiRpcReply::Available {
                summary: PiRpcAvailabilitySummary::new(
                    PiRpcVersion::V0_83_0,
                    PiRpcVersionEvidenceSource::LocalVersionProbe,
                ),
            },
            PiRpcCommand::Start { .. } => PiRpcReply::Ready {
                summary: PiRpcAvailabilitySummary::new(
                    PiRpcVersion::V0_83_0,
                    PiRpcVersionEvidenceSource::LocalVersionProbe,
                )
                .with_readiness_handshake_verified(),
            },
            _ => PiRpcReply::Accepted,
        }
    }
}

#[async_trait]
impl PiRpcPort for DeterministicPiRpc {
    async fn execute(&self, command: PiRpcCommand) -> PortResult<PiRpcReply> {
        self.commands
            .lock()
            .expect("commands lock")
            .push(command.clone());
        self.gate(&command).await;
        self.replies
            .lock()
            .expect("replies lock")
            .pop_front()
            .unwrap_or_else(|| Ok(Self::default_reply(&command)))
    }

    fn subscribe(&self) -> broadcast::Receiver<PiRpcEvent> {
        self.events
            .lock()
            .expect("events lock")
            .as_ref()
            .expect("runtime event source is open")
            .subscribe()
    }
}

struct TrustedWorkspaceFacts;

#[async_trait]
impl WorkbenchWorkspaceFactsPort for TrustedWorkspaceFacts {
    async fn inspect(
        &self,
        request: WorkbenchWorkspaceFactsRequest,
    ) -> PortResult<WorkbenchWorkspaceFacts> {
        Ok(WorkbenchWorkspaceFacts {
            workspace_id: request.workspace_id,
            canonical_root: request.root,
            trusted: true,
            git_repository: true,
        })
    }
}

struct RevocableWorkspaceFacts {
    trusted: AtomicBool,
}

struct FixedTaskBaseline;

#[async_trait]
impl WorkbenchTaskBaselinePort for FixedTaskBaseline {
    async fn capture(
        &self,
        request: bitfun_runtime_ports::WorkbenchTaskBaselineRequest,
    ) -> PortResult<WorkbenchTaskBaseline> {
        Ok(WorkbenchTaskBaseline {
            head: "test-head".to_string(),
            canonical_root: request.canonical_root,
            existing_changed_files: vec![
                "already-tracked.rs".to_string(),
                "untracked-note.txt".to_string(),
            ],
            working_tree_fingerprint: "a".repeat(64),
            captured_at_ms: 1_234,
        })
    }
}

struct MismatchedTaskBaseline;

#[async_trait]
impl WorkbenchTaskBaselinePort for MismatchedTaskBaseline {
    async fn capture(
        &self,
        _request: bitfun_runtime_ports::WorkbenchTaskBaselineRequest,
    ) -> PortResult<WorkbenchTaskBaseline> {
        Ok(WorkbenchTaskBaseline {
            head: "wrong-workspace-head".to_string(),
            canonical_root: PathBuf::from("C:/work/wrong-workspace"),
            existing_changed_files: vec!["wrong-workspace-file.rs".to_string()],
            working_tree_fingerprint: "b".repeat(64),
            captured_at_ms: 1_234,
        })
    }
}

impl RevocableWorkspaceFacts {
    fn new() -> Self {
        Self {
            trusted: AtomicBool::new(true),
        }
    }

    fn revoke(&self) {
        self.trusted.store(false, Ordering::Release);
    }
}

#[async_trait]
impl WorkbenchWorkspaceFactsPort for RevocableWorkspaceFacts {
    async fn inspect(
        &self,
        request: WorkbenchWorkspaceFactsRequest,
    ) -> PortResult<WorkbenchWorkspaceFacts> {
        Ok(WorkbenchWorkspaceFacts {
            workspace_id: request.workspace_id,
            canonical_root: request.root,
            trusted: self.trusted.load(Ordering::Acquire),
            git_repository: true,
        })
    }
}

struct AvailableProviderReadiness;

#[async_trait]
impl PiProviderReadinessPort for AvailableProviderReadiness {
    async fn check(&self) -> PortResult<PiProviderReadiness> {
        Ok(PiProviderReadiness { available: true })
    }
}

struct FailingProviderReadiness;

#[async_trait]
impl PiProviderReadinessPort for FailingProviderReadiness {
    async fn check(&self) -> PortResult<PiProviderReadiness> {
        Err(PortError::new(
            PortErrorKind::Backend,
            "provider-readiness-port-canary",
        ))
    }
}

struct FixedClock;

impl RuntimeServicePort for FixedClock {
    fn capability(&self) -> RuntimeServiceCapability {
        RuntimeServiceCapability::Clock
    }
}

impl ClockPort for FixedClock {
    fn now_unix_millis(&self) -> i64 {
        1_234
    }
}

fn build_runtime(adapter: Arc<DeterministicPiRpc>) -> HaloWorkbenchRuntime {
    build_runtime_with_provider_readiness(adapter, Arc::new(AvailableProviderReadiness))
}

fn build_runtime_with_provider_readiness(
    adapter: Arc<DeterministicPiRpc>,
    provider_readiness: Arc<dyn PiProviderReadinessPort>,
) -> HaloWorkbenchRuntime {
    build_runtime_with_ports(adapter, Arc::new(TrustedWorkspaceFacts), provider_readiness)
}

fn build_runtime_with_ports(
    adapter: Arc<DeterministicPiRpc>,
    workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
    provider_readiness: Arc<dyn PiProviderReadinessPort>,
) -> HaloWorkbenchRuntime {
    HaloWorkbenchRuntime::new_with_task_baseline(
        adapter,
        workspace_facts,
        provider_readiness,
        Arc::new(FixedTaskBaseline),
        Arc::new(FixedClock),
    )
}

fn build_runtime_with_baseline(
    adapter: Arc<DeterministicPiRpc>,
    task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
) -> HaloWorkbenchRuntime {
    HaloWorkbenchRuntime::new_with_task_baseline(
        adapter,
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        task_baseline,
        Arc::new(FixedClock),
    )
}

fn open_request(request_id: &str, workspace_id: &str) -> HaloWorkbenchIntentRequest {
    HaloWorkbenchIntentRequest {
        request_id: request_id.to_string(),
        intent: HaloWorkbenchIntent::OpenWorkspace {
            workspace: HaloWorkbenchWorkspaceInput {
                workspace_id: workspace_id.to_string(),
                display_name: format!("Workspace {workspace_id}"),
                root_path: PathBuf::from(format!("C:/work/{workspace_id}")),
            },
        },
    }
}

async fn wait_for_phase(runtime: &HaloWorkbenchRuntime, phase: HaloWorkbenchPhase) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime.snapshot().phase == phase {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("runtime reaches expected phase");
}

async fn open_ready(
    runtime: &HaloWorkbenchRuntime,
    adapter: &DeterministicPiRpc,
    request_id: &str,
    workspace_id: &str,
) -> u64 {
    runtime
        .submit(open_request(request_id, workspace_id))
        .await
        .expect("workspace open accepted");
    let generation = adapter.generation_for(CommandKind::Start);
    adapter.emit(PiRpcEvent::Ready { generation });
    wait_for_phase(runtime, HaloWorkbenchPhase::Ready).await;
    generation
}

async fn wait_for_session_phase(
    runtime: &HaloWorkbenchRuntime,
    session_id: &str,
    phase: HaloWorkbenchSessionPhase,
) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime
                .snapshot()
                .sessions
                .iter()
                .any(|session| session.session_id == session_id && session.phase == phase)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("session reaches expected phase");
}

async fn create_idle_session(
    runtime: &HaloWorkbenchRuntime,
    adapter: &DeterministicPiRpc,
    generation: u64,
    request_id: &str,
) -> String {
    create_session_with_mode(
        runtime,
        adapter,
        generation,
        request_id,
        HaloWorkbenchSessionMode::Standard,
    )
    .await
}

async fn create_session_with_mode(
    runtime: &HaloWorkbenchRuntime,
    adapter: &DeterministicPiRpc,
    generation: u64,
    request_id: &str,
    mode: HaloWorkbenchSessionMode,
) -> String {
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: request_id.to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: request_id.to_string(),
                mode,
            },
        })
        .await
        .expect("session create accepted");
    let session_id = receipt.session_id.expect("local session id");
    adapter.emit(PiRpcEvent::SessionCreated {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(runtime, &session_id, HaloWorkbenchSessionPhase::Idle).await;
    session_id
}

async fn wait_for_pending_operation(runtime: &HaloWorkbenchRuntime, operation_id: &str) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime
                .snapshot()
                .pending_operations
                .iter()
                .any(|operation| operation.operation_id == operation_id)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("operation becomes pending");
}

async fn wait_for_no_pending_operation(runtime: &HaloWorkbenchRuntime, operation_id: &str) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime
                .snapshot()
                .pending_operations
                .iter()
                .all(|operation| operation.operation_id != operation_id)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("operation leaves the pending set");
}

#[tokio::test]
async fn initial_snapshot_is_disconnected_and_names_only_the_p0_adapter() {
    let runtime = build_runtime(Arc::new(DeterministicPiRpc::new()));

    let snapshot = runtime.snapshot();

    assert_eq!(snapshot.schema_version, HALO_WORKBENCH_SCHEMA_VERSION);
    assert_eq!(snapshot.phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(snapshot.adapter.identity, PI_RPC_ADAPTER_IDENTITY);
    assert!(!snapshot.adapter.available);
    assert_eq!(snapshot.adapter.readiness, None);
    assert_eq!(snapshot.workspace, None);
    assert!(snapshot.sessions.is_empty());
    assert!(snapshot.pending_operations.is_empty());
    assert_eq!(snapshot.last_sequence, 0);
    assert_eq!(snapshot.state_version, 0);
    assert_eq!(snapshot.error, None);

    let wire = serde_json::to_value(snapshot).expect("snapshot serializes");
    assert_eq!(wire["schemaVersion"], 1);
    assert_eq!(wire["phase"], "disconnected");
    assert_eq!(wire["adapter"]["identity"], "pi-rpc-p0");
    assert_eq!(wire["adapter"]["readiness"], serde_json::Value::Null);
    assert_eq!(wire["pendingOperations"], serde_json::json!([]));
}

#[tokio::test]
async fn public_snapshot_projects_safe_probe_profile_without_pi_private_identifiers() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    open_ready(
        &runtime,
        &adapter,
        "open-safe-probe-profile",
        "safe-probe-profile",
    )
    .await;

    let snapshot = runtime.snapshot();
    let readiness = snapshot
        .adapter
        .readiness
        .as_ref()
        .expect("probe profile is committed to the public snapshot");
    assert_eq!(readiness.version.version, PiRpcVersion::V0_83_0);
    assert_eq!(
        readiness.capabilities.required,
        HaloWorkbenchCapability::required_p0().to_vec()
    );
    assert_eq!(
        readiness.capabilities.verified,
        HaloWorkbenchCapability::verified_by_readiness_handshake().to_vec()
    );
    let wire = serde_json::to_string(&snapshot).expect("snapshot serializes");
    assert!(wire.len() < 4096, "snapshot remains bounded");
    for sensitive in [
        "raw-secret",
        "toolCallId",
        "Authorization",
        "HALO_PI_CREDENTIAL",
        "PI_CODING_AGENT_DIR",
        "api.example.test",
        "models",
        "provider",
        "gpt-5",
        "entry-",
        "http://",
        "https://",
    ] {
        assert!(
            !wire.contains(sensitive),
            "public runtime snapshot leaked sensitive field {sensitive}: {wire}"
        );
    }
}

#[tokio::test]
async fn runtime_rejects_incomplete_adapter_readiness_summaries_before_projection() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let mut summary = PiRpcAvailabilitySummary::new(
        PiRpcVersion::V0_83_0,
        PiRpcVersionEvidenceSource::LocalVersionProbe,
    );
    summary.capabilities.required.pop();
    adapter.push_reply(Ok(PiRpcReply::Available { summary }));
    let runtime = build_runtime(adapter);

    let error = runtime
        .submit(open_request(
            "open-incomplete-readiness",
            "incomplete-readiness",
        ))
        .await
        .expect_err("incomplete capability summary fails closed");

    assert_eq!(error.code, "pi_capability_mismatch");
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.phase, HaloWorkbenchPhase::Failed);
    assert_eq!(snapshot.adapter.readiness, None);
}

#[tokio::test]
async fn public_seam_exposes_all_six_runtime_phases() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    adapter.probe_gate.block();
    adapter.start_gate.block();
    let runtime = build_runtime(adapter.clone());
    let open_runtime = runtime.clone();
    let open = tokio::spawn(async move {
        open_runtime
            .submit(open_request("open-six-states", "alpha"))
            .await
    });

    adapter.probe_gate.wait_until_started().await;
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Probing);
    adapter.probe_gate.release();
    adapter.start_gate.wait_until_started().await;
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Starting);
    adapter.start_gate.release();
    open.await.expect("open task").expect("open accepted");

    let generation = adapter.generation_for(CommandKind::Start);
    adapter.emit(PiRpcEvent::Ready { generation });
    wait_for_phase(&runtime, HaloWorkbenchPhase::Ready).await;

    adapter.shutdown_gate.block();
    let close_runtime = runtime.clone();
    let close = tokio::spawn(async move {
        close_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "close-six-states".to_string(),
                intent: HaloWorkbenchIntent::CloseWorkspace,
            })
            .await
    });
    adapter.shutdown_gate.wait_until_started().await;
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Stopping);
    adapter.shutdown_gate.release();
    close.await.expect("close task").expect("close accepted");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);

    let failed_adapter = Arc::new(DeterministicPiRpc::new());
    failed_adapter.push_reply(Ok(PiRpcReply::Unavailable {
        reason: PiRpcFailureKind::NotInstalled,
    }));
    let failed_runtime = build_runtime(failed_adapter);
    failed_runtime
        .submit(open_request("open-failed", "failed"))
        .await
        .expect_err("unavailable adapter is surfaced");
    assert_eq!(failed_runtime.snapshot().phase, HaloWorkbenchPhase::Failed);
}

#[tokio::test]
async fn provider_readiness_port_failure_is_committed_to_the_authoritative_snapshot() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime =
        build_runtime_with_provider_readiness(adapter, Arc::new(FailingProviderReadiness));

    let error = runtime
        .submit(open_request(
            "open-provider-readiness-port-failure",
            "provider-readiness",
        ))
        .await
        .expect_err("provider readiness failure is surfaced");

    assert_eq!(error.code, "provider_readiness_unavailable");
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.phase, HaloWorkbenchPhase::Failed);
    assert_eq!(
        snapshot.error.as_ref().map(|error| error.code.as_str()),
        Some("provider_readiness_unavailable")
    );
    assert_eq!(
        snapshot
            .adapter
            .readiness
            .as_ref()
            .map(|readiness| readiness.version.version),
        Some(PiRpcVersion::V0_83_0)
    );
}

#[tokio::test]
async fn trust_revoke_closes_the_runtime_before_forwarding_session_input() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let facts = Arc::new(RevocableWorkspaceFacts::new());
    let runtime = build_runtime_with_ports(
        adapter.clone(),
        facts.clone(),
        Arc::new(AvailableProviderReadiness),
    );
    let generation = open_ready(&runtime, &adapter, "open-trust-revoke", "trust-revoke").await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-trust-revoke".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "trust-revoke".to_string(),
                root_path: PathBuf::from("C:/work/trust-revoke"),
            },
        })
        .await
        .expect("managed workspace confirmation accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-trust-revoke",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    facts.revoke();
    adapter.clear_commands();

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-after-trust-revoke".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id,
                content: "must-not-forward".to_string(),
            },
        })
        .await
        .expect_err("revoked trust fails closed");

    assert_eq!(error.code, "workspace_untrusted");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(adapter.count(CommandKind::SendUserInput), 0);
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
}

#[tokio::test]
async fn managed_task_requires_confirmation_and_records_existing_git_baseline_before_starting() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-managed-baseline",
        "managed-baseline",
    )
    .await;

    let unconfirmed = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-unconfirmed-managed-task".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "managed-baseline-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
            },
        })
        .await
        .expect_err("managed session requires explicit workspace confirmation");
    assert_eq!(unconfirmed.code, "managed_workspace_confirmation_required");
    assert_eq!(adapter.count(CommandKind::CreateSession), 0);

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-managed-baseline".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "managed-baseline".to_string(),
                root_path: PathBuf::from("C:/work/managed-baseline"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-confirmed-managed-task".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "managed-baseline-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
            },
        })
        .await
        .expect("confirmed managed session is created");
    let session_id = receipt.session_id.expect("managed session id");
    let session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session is projected");
    let baseline = session
        .baseline
        .expect("baseline is captured before Pi starts the task");
    assert_eq!(baseline.head, "test-head");
    assert_eq!(
        baseline.canonical_root,
        PathBuf::from("C:/work/managed-baseline")
    );
    assert_eq!(
        baseline.existing_changed_files,
        vec![
            "already-tracked.rs".to_string(),
            "untracked-note.txt".to_string(),
        ]
    );
    assert_eq!(session.phase, HaloWorkbenchSessionPhase::Creating);
    assert_eq!(adapter.count(CommandKind::CreateSession), 1);
    assert!(adapter.commands().iter().any(|command| {
        matches!(
            command,
            PiRpcCommand::CreateSession {
                generation: command_generation,
                task_id,
                session_id: command_session_id,
                mode: bitfun_runtime_ports::PiRpcSessionMode::Managed,
            } if *command_generation == generation
                && task_id == "managed-baseline-task"
                && command_session_id == &session_id
        )
    }));
}

#[tokio::test]
async fn managed_task_rejects_a_baseline_from_a_different_workspace_before_starting_pi() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_baseline(adapter.clone(), Arc::new(MismatchedTaskBaseline));
    let _generation = open_ready(
        &runtime,
        &adapter,
        "open-mismatched-managed-baseline",
        "mismatched-managed-baseline",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-mismatched-managed-baseline".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "mismatched-managed-baseline".to_string(),
                root_path: PathBuf::from("C:/work/mismatched-managed-baseline"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-mismatched-managed-baseline".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "mismatched-managed-baseline-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
            },
        })
        .await
        .expect_err("a baseline must belong to the confirmed workspace");

    assert_eq!(error.code, "task_baseline_unavailable");
    assert_eq!(adapter.count(CommandKind::CreateSession), 0);
    assert!(runtime
        .snapshot()
        .sessions
        .iter()
        .all(|session| session.baseline.is_none()));
}

#[tokio::test]
async fn managed_first_turn_projects_redacted_activity_and_fences_late_events() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-managed-first-turn-projection",
        "managed-first-turn-projection",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-managed-first-turn-projection".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "managed-first-turn-projection".to_string(),
                root_path: PathBuf::from("C:/work/managed-first-turn-projection"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-managed-first-turn-projection".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "managed-first-turn-projection-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
            },
        })
        .await
        .expect("managed session is created");
    let session_id = receipt.session_id.expect("managed session id");
    adapter.emit(PiRpcEvent::SessionCreated {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Idle).await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-managed-first-turn-projection".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "Inspect the focused change".to_string(),
            },
        })
        .await
        .expect("first managed prompt is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    adapter.emit(PiRpcEvent::MessageUpdated {
        generation,
        session_id: session_id.clone(),
        text: "Safe response Authorization: Basic runtime-basic-canary Cookie: runtime-cookie-canary password=runtime-password-canary token=runtime-message-canary sessionId=runtime-session-id-canary entryId: runtime-entry-id-canary toolCallId=\"runtime-tool-call-id-canary\"".to_string(),
    });
    for (status, is_error) in [
        (HaloWorkbenchActivityStatus::Started, false),
        (HaloWorkbenchActivityStatus::Updated, false),
        (HaloWorkbenchActivityStatus::Completed, false),
    ] {
        let event = match status {
            HaloWorkbenchActivityStatus::Started => PiRpcEvent::ToolExecutionStarted {
                generation,
                session_id: session_id.clone(),
                redacted_tool_call_id: "raw-tool-call-id-canary".to_string(),
                tool_name: "write Authorization: Basic runtime-tool-basic-canary Cookie: runtime-tool-cookie-canary password=runtime-tool-password-canary token=runtime-tool-canary".to_string(),
            },
            HaloWorkbenchActivityStatus::Updated => PiRpcEvent::ToolExecutionUpdated {
                generation,
                session_id: session_id.clone(),
                redacted_tool_call_id: "raw-tool-call-id-canary".to_string(),
                tool_name: "write Authorization: Basic runtime-tool-basic-canary Cookie: runtime-tool-cookie-canary password=runtime-tool-password-canary token=runtime-tool-canary".to_string(),
            },
            HaloWorkbenchActivityStatus::Completed => PiRpcEvent::ToolExecutionEnded {
                generation,
                session_id: session_id.clone(),
                redacted_tool_call_id: "raw-tool-call-id-canary".to_string(),
                tool_name: "write Authorization: Basic runtime-tool-basic-canary Cookie: runtime-tool-cookie-canary password=runtime-tool-password-canary token=runtime-tool-canary".to_string(),
                is_error,
            },
            _ => unreachable!("the first-turn fixture only emits tool lifecycle states"),
        };
        adapter.emit(event);
    }
    adapter.emit(PiRpcEvent::AgentSettled {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::WaitingDeveloper,
    )
    .await;

    let settled_snapshot = runtime.snapshot();
    let settled_session = settled_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session remains in the public snapshot");
    assert_eq!(settled_session.messages.len(), 2);
    assert_eq!(
        settled_session.messages[0].role,
        HaloWorkbenchMessageRole::User
    );
    assert_eq!(
        settled_session.messages[0].content,
        "Inspect the focused change"
    );
    assert_eq!(
        settled_session.messages[1].role,
        HaloWorkbenchMessageRole::Assistant
    );
    assert_eq!(
        settled_session.messages[1].content,
        "Safe response Authorization: [redacted] Cookie: [redacted] password=[redacted] token=[redacted] sessionId=[redacted] entryId: [redacted] toolCallId=\"[redacted]\""
    );
    assert_eq!(settled_session.activities.len(), 1);
    assert_eq!(
        settled_session.activities[0].status,
        HaloWorkbenchActivityStatus::Completed
    );
    assert_eq!(
        settled_session.activities[0].label,
        "write Authorization: [redacted] Cookie: [redacted] password=[redacted] token=[redacted]"
    );
    assert!(settled_session.activities[0]
        .activity_id
        .starts_with("activity-"));
    assert!(!settled_session.activities[0].is_error);
    let public = serde_json::to_string(&settled_snapshot).expect("snapshot serializes");
    for canary in [
        "runtime-message-canary",
        "runtime-basic-canary",
        "runtime-cookie-canary",
        "runtime-password-canary",
        "runtime-tool-canary",
        "runtime-tool-basic-canary",
        "runtime-tool-cookie-canary",
        "runtime-tool-password-canary",
        "runtime-session-id-canary",
        "runtime-entry-id-canary",
        "runtime-tool-call-id-canary",
        "raw-tool-call-id-canary",
    ] {
        assert!(!public.contains(canary), "public snapshot leaked {canary}");
    }

    let settled_version = settled_snapshot.state_version;
    adapter.emit(PiRpcEvent::MessageUpdated {
        generation,
        session_id: session_id.clone(),
        text: "late-message-canary".to_string(),
    });
    adapter.emit(PiRpcEvent::ToolExecutionUpdated {
        generation,
        session_id: session_id.clone(),
        redacted_tool_call_id: "raw-tool-call-id-canary".to_string(),
        tool_name: "late-tool-canary".to_string(),
    });
    adapter.emit(PiRpcEvent::MessageUpdated {
        generation,
        session_id: "foreign-session".to_string(),
        text: "foreign-message-canary".to_string(),
    });
    adapter.emit(PiRpcEvent::AgentSettled {
        generation: generation.saturating_add(1),
        session_id,
    });
    tokio::task::yield_now().await;

    let after_late_events = runtime.snapshot();
    assert_eq!(after_late_events.state_version, settled_version);
    let after_late_session = after_late_events
        .sessions
        .iter()
        .find(|session| session.task_id == "managed-first-turn-projection-task")
        .expect("managed session remains isolated from late events");
    assert_eq!(after_late_session.messages.len(), 2);
    assert_eq!(after_late_session.activities.len(), 1);
    let after_late_public =
        serde_json::to_string(&after_late_events).expect("late-event snapshot serializes");
    for canary in [
        "late-message-canary",
        "late-tool-canary",
        "foreign-message-canary",
    ] {
        assert!(
            !after_late_public.contains(canary),
            "late or foreign event leaked {canary}"
        );
    }
}

#[tokio::test]
async fn session_snapshot_binds_workspace_task_and_session_and_rejects_duplicate_task() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-explicit-binding",
        "explicit-binding",
    )
    .await;
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "task-explicit-binding").await;

    let session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("created session is projected");
    assert_eq!(session.workspace_id, "explicit-binding");
    assert_eq!(session.task_id, "task-explicit-binding");
    assert_eq!(session.session_id, session_id);

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "duplicate-task-binding".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "task-explicit-binding".to_string(),
                mode: HaloWorkbenchSessionMode::Standard,
            },
        })
        .await
        .expect_err("one active session is allowed per workspace/task binding");
    assert_eq!(error.code, "task_already_active");
    assert_eq!(adapter.count(CommandKind::CreateSession), 1);
}

#[tokio::test]
async fn prompt_settled_follow_up_and_abort_obey_non_replay_lifecycle() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-session-lifecycle",
        "session-lifecycle",
    )
    .await;
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "task-session-lifecycle").await;

    let premature_follow_up = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "follow-up-before-settled".to_string(),
            intent: HaloWorkbenchIntent::FollowUp {
                session_id: session_id.clone(),
                content: "must wait".to_string(),
            },
        })
        .await
        .expect_err("follow-up cannot be sent before the first settlement");
    assert_eq!(premature_follow_up.code, "session_not_ready");
    assert_eq!(adapter.count(CommandKind::FollowUp), 0);

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "first-prompt-lifecycle".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "first prompt".to_string(),
            },
        })
        .await
        .expect("first prompt is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    let repeated_prompt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "repeated-prompt-lifecycle".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "must not replay".to_string(),
            },
        })
        .await
        .expect_err("a running session cannot accept a second prompt");
    assert_eq!(repeated_prompt.code, "session_busy");
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);

    adapter.emit(PiRpcEvent::AgentSettled {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::WaitingDeveloper,
    )
    .await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "follow-up-after-settled".to_string(),
            intent: HaloWorkbenchIntent::FollowUp {
                session_id: session_id.clone(),
                content: "continue explicitly".to_string(),
            },
        })
        .await
        .expect("follow-up is accepted only after agent_settled");
    assert_eq!(adapter.count(CommandKind::FollowUp), 1);
    adapter.emit(PiRpcEvent::AgentSettled {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::WaitingDeveloper,
    )
    .await;

    let abort_before_running = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "abort-after-settlement".to_string(),
            intent: HaloWorkbenchIntent::AbortSession {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect_err("abort is only for a running turn");
    assert_eq!(abort_before_running.code, "session_not_ready");
    assert_eq!(adapter.count(CommandKind::AbortSession), 0);

    let abort_target =
        create_idle_session(&runtime, &adapter, generation, "task-abort-lifecycle").await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "abort-prompt".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: abort_target.clone(),
                content: "abort me".to_string(),
            },
        })
        .await
        .expect("abort target prompt is accepted");
    wait_for_session_phase(&runtime, &abort_target, HaloWorkbenchSessionPhase::Running).await;

    adapter.stop_session_gate.block();
    let abort_runtime = runtime.clone();
    let abort_session_id = abort_target.clone();
    let abort = tokio::spawn(async move {
        abort_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "abort-running".to_string(),
                intent: HaloWorkbenchIntent::AbortSession {
                    session_id: abort_session_id,
                },
            })
            .await
    });
    adapter.stop_session_gate.wait_until_started().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime.snapshot().sessions.iter().any(|session| {
                session.session_id == abort_target
                    && session.phase == HaloWorkbenchSessionPhase::Stopping
            }) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("abort marks the session stopping before crossing the port");
    let stopping_snapshot = runtime.snapshot();
    let stopping = stopping_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == abort_target)
        .expect("abort target remains projected while abort is in flight");
    assert_eq!(stopping.phase, HaloWorkbenchSessionPhase::Stopping);

    adapter.emit(PiRpcEvent::AgentSettled {
        generation,
        session_id: abort_target.clone(),
    });
    tokio::task::yield_now().await;
    assert_eq!(
        runtime
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.session_id == abort_target)
            .expect("abort target remains projected")
            .phase,
        HaloWorkbenchSessionPhase::Stopping,
        "late settlement cannot reopen a stopping session"
    );
    adapter.stop_session_gate.release();
    abort
        .await
        .expect("abort task")
        .expect("abort command is accepted");
    adapter.emit(PiRpcEvent::SessionStopped {
        generation,
        session_id: abort_target.clone(),
    });
    wait_for_session_phase(
        &runtime,
        &abort_target,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;

    let repeated_abort = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "abort-interrupted".to_string(),
            intent: HaloWorkbenchIntent::AbortSession {
                session_id: abort_target.clone(),
            },
        })
        .await
        .expect_err("interrupted sessions cannot replay abort");
    assert_eq!(repeated_abort.code, "session_not_ready");
    let replayed_input = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "input-interrupted".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: abort_target,
                content: "must not replay after abort".to_string(),
            },
        })
        .await
        .expect_err("interrupted sessions cannot replay input");
    assert_eq!(replayed_input.code, "session_not_ready");
}

#[tokio::test]
async fn end_session_accepts_adapter_removal_before_the_runtime_event_arrives() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-end-session", "end-session").await;
    let session_id = create_idle_session(&runtime, &adapter, generation, "task-end-session").await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "end-session-command".to_string(),
            intent: HaloWorkbenchIntent::EndSession {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("end is accepted after the adapter removes its owned process");
    assert_eq!(adapter.count(CommandKind::EndSession), 1);

    adapter.emit(PiRpcEvent::SessionEnded {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Ended).await;
}

#[tokio::test]
async fn repeated_open_and_close_reuses_only_current_generation_cleanup() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    open_ready(&runtime, &adapter, "open-repeat-first", "repeat-first").await;

    runtime
        .submit(open_request("open-repeat-second", "repeat-second"))
        .await
        .expect("repeated open accepts after cleaning the old generation");
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
    let second_generation = adapter.generation_for(CommandKind::Start);
    adapter.emit(PiRpcEvent::Ready {
        generation: second_generation,
    });
    wait_for_phase(&runtime, HaloWorkbenchPhase::Ready).await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "close-repeat-first".to_string(),
            intent: HaloWorkbenchIntent::CloseWorkspace,
        })
        .await
        .expect("first close accepts");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(adapter.count(CommandKind::Shutdown), 2);

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "close-repeat-second".to_string(),
            intent: HaloWorkbenchIntent::CloseWorkspace,
        })
        .await
        .expect("repeated close is safe");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(adapter.count(CommandKind::Shutdown), 2);
}

#[tokio::test]
async fn prepared_handshake_failure_can_be_recovered_by_a_new_open() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    adapter.push_reply(Ok(PiRpcReply::Available {
        summary: PiRpcAvailabilitySummary::new(
            PiRpcVersion::V0_81_1,
            PiRpcVersionEvidenceSource::LocalVersionProbe,
        ),
    }));
    adapter.push_reply(Ok(PiRpcReply::Unavailable {
        reason: PiRpcFailureKind::Protocol,
    }));
    let runtime = build_runtime(adapter.clone());

    let first_error = runtime
        .submit(open_request("open-handshake-failure", "handshake-failure"))
        .await
        .expect_err("prepared handshake failure is surfaced");
    assert_eq!(first_error.code, "pi_protocol_error");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Failed);

    adapter.push_reply(Ok(PiRpcReply::Accepted));
    adapter.push_reply(Ok(PiRpcReply::Available {
        summary: PiRpcAvailabilitySummary::new(
            PiRpcVersion::V0_81_1,
            PiRpcVersionEvidenceSource::LocalVersionProbe,
        ),
    }));
    adapter.push_reply(Ok(PiRpcReply::Ready {
        summary: PiRpcAvailabilitySummary::new(
            PiRpcVersion::V0_81_1,
            PiRpcVersionEvidenceSource::LocalVersionProbe,
        )
        .with_readiness_handshake_verified(),
    }));
    runtime
        .submit(open_request("open-handshake-retry", "handshake-retry"))
        .await
        .expect("new open recovers after failed handshake cleanup");
    let generation = adapter.generation_for(CommandKind::Start);
    adapter.emit(PiRpcEvent::Ready { generation });
    wait_for_phase(&runtime, HaloWorkbenchPhase::Ready).await;
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
    assert_eq!(adapter.count(CommandKind::Probe), 2);
    assert_eq!(adapter.count(CommandKind::Start), 2);
}

#[tokio::test]
async fn concurrent_close_requests_share_one_cleanup_result() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    open_ready(&runtime, &adapter, "open-concurrent-close", "close").await;
    adapter.clear_commands();
    adapter.shutdown_gate.block();

    let first_runtime = runtime.clone();
    let first = tokio::spawn(async move {
        first_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "close-concurrent-first".to_string(),
                intent: HaloWorkbenchIntent::CloseWorkspace,
            })
            .await
    });
    adapter.shutdown_gate.wait_until_started().await;
    let second_runtime = runtime.clone();
    let second = tokio::spawn(async move {
        second_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "close-concurrent-second".to_string(),
                intent: HaloWorkbenchIntent::CloseWorkspace,
            })
            .await
    });
    tokio::task::yield_now().await;
    adapter.shutdown_gate.release();

    first.await.expect("first close task").expect("first close");
    second
        .await
        .expect("second close task")
        .expect("second close");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
}

#[tokio::test]
async fn concurrent_session_inputs_are_serialized_at_the_workbench_seam() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-input-serialization",
        "serialization",
    )
    .await;
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "create-input-serialization").await;
    adapter.clear_commands();
    adapter.send_user_input_gate.block();

    let first_runtime = runtime.clone();
    let first_session_id = session_id.clone();
    let first = tokio::spawn(async move {
        first_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "send-input-first".to_string(),
                intent: HaloWorkbenchIntent::SendUserInput {
                    session_id: first_session_id,
                    content: "first".to_string(),
                },
            })
            .await
    });
    adapter.send_user_input_gate.wait_until_started().await;

    let second_runtime = runtime.clone();
    let second_session_id = session_id.clone();
    let second = tokio::spawn(async move {
        second_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "send-input-second".to_string(),
                intent: HaloWorkbenchIntent::SendUserInput {
                    session_id: second_session_id,
                    content: "second".to_string(),
                },
            })
            .await
    });
    tokio::task::yield_now().await;
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);

    adapter.send_user_input_gate.release();
    first
        .await
        .expect("first input task")
        .expect("first input accepted");
    let second_error = second
        .await
        .expect("second input task")
        .expect_err("a second prompt cannot replay a running task");
    assert_eq!(second_error.code, "session_busy");
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);
}

#[tokio::test]
async fn close_fences_an_in_flight_operation_decision() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-resolve-fence", "resolve-fence").await;
    let session_id = create_idle_session(
        &runtime,
        &adapter,
        generation,
        "create-resolve-fence-session",
    )
    .await;
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id,
        operation_id: "resolve-close-race".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    wait_for_pending_operation(&runtime, "resolve-close-race").await;
    adapter.clear_commands();
    adapter.resolve_gate.block();
    adapter.shutdown_gate.block();

    let resolve_runtime = runtime.clone();
    let resolve = tokio::spawn(async move {
        resolve_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "resolve-before-close".to_string(),
                intent: HaloWorkbenchIntent::ResolveOperation {
                    operation_id: "resolve-close-race".to_string(),
                    decision: HaloWorkbenchOperationDecision::AllowOnce,
                },
            })
            .await
    });
    adapter.resolve_gate.wait_until_started().await;
    let close_runtime = runtime.clone();
    let close = tokio::spawn(async move {
        close_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "close-during-resolve".to_string(),
                intent: HaloWorkbenchIntent::CloseWorkspace,
            })
            .await
    });
    wait_for_phase(&runtime, HaloWorkbenchPhase::Stopping).await;
    assert_eq!(adapter.count(CommandKind::Shutdown), 0);
    adapter.resolve_gate.release();

    let error = resolve
        .await
        .expect("resolve task")
        .expect_err("close supersedes operation resolution");
    assert_eq!(error.code, "runtime_not_ready");
    adapter.shutdown_gate.wait_until_started().await;
    adapter.shutdown_gate.release();
    close.await.expect("close task").expect("close accepted");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
}

#[tokio::test]
async fn operation_decision_remains_pending_until_the_adapter_confirms_it() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-operation", "operation").await;
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-operation-session".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "operation-task".to_string(),
                mode: HaloWorkbenchSessionMode::Standard,
            },
        })
        .await
        .expect("session create accepted");
    let session_id = receipt.session_id.expect("local session id");
    adapter.emit(PiRpcEvent::SessionCreated {
        generation,
        session_id: session_id.clone(),
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: session_id.clone(),
        operation_id: "operation-local".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    tokio::task::yield_now().await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "resolve-operation".to_string(),
            intent: HaloWorkbenchIntent::ResolveOperation {
                operation_id: "operation-local".to_string(),
                decision: HaloWorkbenchOperationDecision::AllowOnce,
            },
        })
        .await
        .expect("decision submitted");
    let pending = runtime.snapshot().pending_operations;
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].phase,
        HaloWorkbenchPendingOperationPhase::DecisionSubmitted
    );
    assert_eq!(adapter.count(CommandKind::ResolveOperation), 1);

    adapter.emit(PiRpcEvent::OperationResolved {
        generation,
        session_id,
        operation_id: "operation-local".to_string(),
    });
    tokio::task::yield_now().await;
    assert!(runtime.snapshot().pending_operations.is_empty());
}

#[tokio::test]
async fn terminal_sessions_do_not_regress_accept_commands_or_keep_operations() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-terminal-session", "terminal").await;
    let ended_session =
        create_idle_session(&runtime, &adapter, generation, "create-ended-session").await;
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: ended_session.clone(),
        operation_id: "operation-before-end".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    wait_for_pending_operation(&runtime, "operation-before-end").await;
    adapter.emit(PiRpcEvent::SessionEnded {
        generation,
        session_id: ended_session.clone(),
    });
    wait_for_session_phase(&runtime, &ended_session, HaloWorkbenchSessionPhase::Ended).await;
    assert!(runtime.snapshot().pending_operations.is_empty());
    let ended_version = runtime.snapshot().state_version;

    adapter.emit(PiRpcEvent::SessionRunning {
        generation,
        session_id: ended_session.clone(),
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: ended_session.clone(),
        operation_id: "operation-after-end".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    tokio::task::yield_now().await;
    let after_late_events = runtime.snapshot();
    assert_eq!(after_late_events.state_version, ended_version);
    assert_eq!(
        after_late_events.sessions[0].phase,
        HaloWorkbenchSessionPhase::Ended
    );
    assert!(after_late_events.pending_operations.is_empty());

    adapter.clear_commands();
    for (request_id, intent) in [
        (
            "send-ended-session",
            HaloWorkbenchIntent::SendUserInput {
                session_id: ended_session.clone(),
                content: "late input".to_string(),
            },
        ),
        (
            "stop-ended-session",
            HaloWorkbenchIntent::StopSession {
                session_id: ended_session.clone(),
            },
        ),
        (
            "end-ended-session",
            HaloWorkbenchIntent::EndSession {
                session_id: ended_session.clone(),
            },
        ),
    ] {
        let error = runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: request_id.to_string(),
                intent,
            })
            .await
            .expect_err("terminal session rejects commands");
        assert_eq!(error.code, "session_terminal");
    }
    assert_eq!(adapter.count(CommandKind::SendUserInput), 0);
    assert_eq!(adapter.count(CommandKind::StopSession), 0);
    assert_eq!(adapter.count(CommandKind::EndSession), 0);

    let failed_session =
        create_idle_session(&runtime, &adapter, generation, "create-failed-session").await;
    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: failed_session.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    wait_for_session_phase(&runtime, &failed_session, HaloWorkbenchSessionPhase::Failed).await;
    adapter.emit(PiRpcEvent::SessionIdle {
        generation,
        session_id: failed_session.clone(),
    });
    tokio::task::yield_now().await;
    assert!(runtime.snapshot().sessions.iter().any(|session| {
        session.session_id == failed_session && session.phase == HaloWorkbenchSessionPhase::Failed
    }));
}

#[tokio::test]
async fn operation_resolution_must_match_the_owning_session() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-operation-association",
        "operation-association",
    )
    .await;
    let owner_session =
        create_idle_session(&runtime, &adapter, generation, "create-operation-owner").await;
    let other_session =
        create_idle_session(&runtime, &adapter, generation, "create-operation-other").await;
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: owner_session.clone(),
        operation_id: "session-bound-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    wait_for_pending_operation(&runtime, "session-bound-operation").await;

    adapter.emit(PiRpcEvent::OperationResolved {
        generation,
        session_id: other_session,
        operation_id: "session-bound-operation".to_string(),
    });
    tokio::task::yield_now().await;
    assert!(runtime
        .snapshot()
        .pending_operations
        .iter()
        .any(|operation| operation.operation_id == "session-bound-operation"));

    adapter.emit(PiRpcEvent::OperationResolved {
        generation,
        session_id: owner_session,
        operation_id: "session-bound-operation".to_string(),
    });
    wait_for_no_pending_operation(&runtime, "session-bound-operation").await;
}

#[tokio::test]
async fn concurrent_operation_decisions_cross_the_seam_exactly_once() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-decision-race", "decision-race").await;
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-decision-race-session".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "decision-race-task".to_string(),
                mode: HaloWorkbenchSessionMode::Standard,
            },
        })
        .await
        .expect("session create accepted");
    let session_id = receipt.session_id.expect("local session id");
    adapter.emit(PiRpcEvent::SessionCreated {
        generation,
        session_id: session_id.clone(),
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id,
        operation_id: "operation-race".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    tokio::task::yield_now().await;

    adapter.resolve_gate.block();
    let first_runtime = runtime.clone();
    let first = tokio::spawn(async move {
        first_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "resolve-race-first".to_string(),
                intent: HaloWorkbenchIntent::ResolveOperation {
                    operation_id: "operation-race".to_string(),
                    decision: HaloWorkbenchOperationDecision::AllowOnce,
                },
            })
            .await
    });
    adapter.resolve_gate.wait_until_started().await;

    let second = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "resolve-race-second".to_string(),
            intent: HaloWorkbenchIntent::ResolveOperation {
                operation_id: "operation-race".to_string(),
                decision: HaloWorkbenchOperationDecision::Deny,
            },
        })
        .await
        .expect_err("a pending one-time decision cannot be submitted twice");
    assert_eq!(second.code, "operation_decision_in_progress");
    assert_eq!(adapter.count(CommandKind::ResolveOperation), 1);

    adapter.resolve_gate.release();
    first
        .await
        .expect("first decision task")
        .expect("first decision accepted");
}

#[tokio::test]
async fn operation_decisions_are_limited_to_one_time_allow_or_deny() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-decision-kinds", "decision-kinds").await;
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-decision-kind-session".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "decision-kind-task".to_string(),
                mode: HaloWorkbenchSessionMode::Standard,
            },
        })
        .await
        .expect("session create accepted");
    let session_id = receipt.session_id.expect("local session id");
    adapter.emit(PiRpcEvent::SessionCreated {
        generation,
        session_id: session_id.clone(),
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: session_id.clone(),
        operation_id: "allow-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id,
        operation_id: "deny-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        redacted_tool_call_id: None,
    });
    tokio::task::yield_now().await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "answer-permission".to_string(),
            intent: HaloWorkbenchIntent::ResolveOperation {
                operation_id: "allow-operation".to_string(),
                decision: HaloWorkbenchOperationDecision::AllowOnce,
            },
        })
        .await
        .expect("allow decision is accepted");
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "deny-permission".to_string(),
            intent: HaloWorkbenchIntent::ResolveOperation {
                operation_id: "deny-operation".to_string(),
                decision: HaloWorkbenchOperationDecision::Deny,
            },
        })
        .await
        .expect("deny decision is accepted");

    assert_eq!(adapter.count(CommandKind::ResolveOperation), 2);
    assert!(runtime
        .snapshot()
        .pending_operations
        .iter()
        .all(|operation| operation.phase == HaloWorkbenchPendingOperationPhase::DecisionSubmitted));
}

#[tokio::test]
async fn blank_user_input_is_rejected_before_the_adapter() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-blank-input", "blank-input").await;
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "create-blank-input-session").await;
    adapter.clear_commands();

    let input_error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "blank-user-input".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: " \t\r\n ".to_string(),
            },
        })
        .await
        .expect_err("blank input is invalid");
    assert_eq!(input_error.code, "invalid_request");
    assert_eq!(adapter.count(CommandKind::SendUserInput), 0);
}

#[tokio::test]
async fn adapter_failures_and_debug_output_strip_sensitive_fields() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    adapter.push_reply(Err(PortError::new(
        PortErrorKind::Backend,
        "Bearer adapter-error-canary http://127.0.0.1:4096",
    )));
    let runtime = build_runtime(adapter.clone());
    let mut events = runtime.subscribe();
    let request = HaloWorkbenchIntentRequest {
        request_id: "redaction-request".to_string(),
        intent: HaloWorkbenchIntent::SendUserInput {
            session_id: "session-local".to_string(),
            content: "user-input-canary".to_string(),
        },
    };
    assert!(!format!("{request:?}").contains("user-input-canary"));

    let error = runtime
        .submit(open_request("open-redaction", "redaction"))
        .await
        .expect_err("adapter error is projected");
    let generation = adapter.generation_for(CommandKind::Probe);
    adapter.emit(PiRpcEvent::Ready { generation });
    tokio::task::yield_now().await;
    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.phase, HaloWorkbenchPhase::Failed);
    let mut published_events = Vec::new();
    while let Ok(event) = events.try_recv() {
        published_events.push(event);
    }
    let public = format!(
        "{:?} {} {} {}",
        error,
        serde_json::to_string(&snapshot).unwrap(),
        serde_json::to_string(&published_events).unwrap(),
        snapshot.error.as_ref().unwrap().summary
    );
    for canary in [
        "adapter-error-canary",
        "user-input-canary",
        "127.0.0.1:4096",
        "Bearer",
    ] {
        assert!(
            !public.contains(canary),
            "sensitive canary leaked: {canary}"
        );
    }
}
