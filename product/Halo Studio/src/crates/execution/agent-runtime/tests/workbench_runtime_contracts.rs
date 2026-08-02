use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bitfun_agent_runtime::halo_workbench::{
    HaloWorkbenchIntent, HaloWorkbenchIntentRequest, HaloWorkbenchOperationDecision,
    HaloWorkbenchPendingOperationPhase, HaloWorkbenchPhase, HaloWorkbenchRuntime,
    HaloWorkbenchSessionMode, HaloWorkbenchSessionPhase, HaloWorkbenchWorkspaceInput,
    HALO_WORKBENCH_SCHEMA_VERSION,
};
use bitfun_runtime_ports::{
    ClockPort, PiProviderReadiness, PiProviderReadinessPort, PiRpcCommand, PiRpcEvent,
    PiRpcFailureKind, PiRpcOperationKind, PiRpcPort, PiRpcReply, PortError, PortErrorKind,
    PortResult, RuntimeServiceCapability, RuntimeServicePort, WorkbenchWorkspaceFacts,
    WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest, PI_RPC_ADAPTER_IDENTITY,
};
use tokio::sync::{broadcast, Notify, Semaphore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    Probe,
    Start,
    CreateSession,
    SendUserInput,
    StopSession,
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
            PiRpcCommand::StopSession { .. } => Some(Self::StopSession),
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

    fn close_events(&self) {
        self.events.lock().expect("events lock").take();
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
            Some(CommandKind::StopSession) => self.stop_session_gate.wait_if_enabled().await,
            Some(CommandKind::EndSession) => self.end_session_gate.wait_if_enabled().await,
            Some(CommandKind::ResolveOperation) => self.resolve_gate.wait_if_enabled().await,
            Some(CommandKind::Shutdown) => self.shutdown_gate.wait_if_enabled().await,
            None => {}
        }
    }

    fn command_gate(&self, kind: CommandKind) -> &CommandGate {
        match kind {
            CommandKind::Probe => &self.probe_gate,
            CommandKind::Start => &self.start_gate,
            CommandKind::CreateSession => &self.create_session_gate,
            CommandKind::SendUserInput => &self.send_user_input_gate,
            CommandKind::StopSession => &self.stop_session_gate,
            CommandKind::EndSession => &self.end_session_gate,
            CommandKind::ResolveOperation => &self.resolve_gate,
            CommandKind::Shutdown => &self.shutdown_gate,
        }
    }

    fn default_reply(command: &PiRpcCommand) -> PiRpcReply {
        match command {
            PiRpcCommand::Probe { .. } => PiRpcReply::Available,
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

struct FailingWorkspaceFacts;

#[async_trait]
impl WorkbenchWorkspaceFactsPort for FailingWorkspaceFacts {
    async fn inspect(
        &self,
        _request: WorkbenchWorkspaceFactsRequest,
    ) -> PortResult<WorkbenchWorkspaceFacts> {
        Err(PortError::new(
            PortErrorKind::Backend,
            "workspace-port-canary",
        ))
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
    HaloWorkbenchRuntime::new(
        adapter,
        workspace_facts,
        provider_readiness,
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
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: request_id.to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
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

async fn assert_session_command_is_fenced_by_close(
    kind: CommandKind,
    build_intent: fn(String) -> HaloWorkbenchIntent,
) {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-session-fence", "fence").await;
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "create-session-fence").await;
    adapter.clear_commands();
    adapter.command_gate(kind).block();

    let command_runtime = runtime.clone();
    let command = tokio::spawn(async move {
        command_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: format!("fenced-{kind:?}"),
                intent: build_intent(session_id),
            })
            .await
    });
    adapter.command_gate(kind).wait_until_started().await;

    let close_runtime = runtime.clone();
    adapter.shutdown_gate.block();
    let close = tokio::spawn(async move {
        close_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: format!("close-fenced-{kind:?}"),
                intent: HaloWorkbenchIntent::CloseWorkspace,
            })
            .await
    });
    wait_for_phase(&runtime, HaloWorkbenchPhase::Stopping).await;
    assert_eq!(adapter.count(CommandKind::Shutdown), 0);
    adapter.command_gate(kind).release();

    let error = command
        .await
        .expect("session command task")
        .expect_err("close supersedes an in-flight session command");
    assert_eq!(error.code, "runtime_not_ready");
    adapter.shutdown_gate.wait_until_started().await;
    adapter.shutdown_gate.release();
    close.await.expect("close task").expect("close accepted");
    assert_eq!(adapter.count(kind), 1);
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
}

#[tokio::test]
async fn initial_snapshot_is_disconnected_and_names_only_the_p0_adapter() {
    let runtime = build_runtime(Arc::new(DeterministicPiRpc::new()));

    let snapshot = runtime.snapshot();

    assert_eq!(snapshot.schema_version, HALO_WORKBENCH_SCHEMA_VERSION);
    assert_eq!(snapshot.phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(snapshot.adapter.identity, PI_RPC_ADAPTER_IDENTITY);
    assert!(!snapshot.adapter.available);
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
    assert_eq!(wire["pendingOperations"], serde_json::json!([]));
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
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "create-trust-revoke").await;
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
    adapter.push_reply(Ok(PiRpcReply::Available));
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
    adapter.push_reply(Ok(PiRpcReply::Available));
    adapter.push_reply(Ok(PiRpcReply::Accepted));
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
    second
        .await
        .expect("second input task")
        .expect("second input accepted");
    assert_eq!(adapter.count(CommandKind::SendUserInput), 2);
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
                mode: HaloWorkbenchSessionMode::Managed,
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
                mode: HaloWorkbenchSessionMode::Managed,
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
