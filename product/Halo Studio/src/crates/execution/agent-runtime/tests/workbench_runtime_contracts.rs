use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use halo_agent_runtime::halo_workbench::{
    HaloWorkbenchActivityStatus, HaloWorkbenchCancellationMode, HaloWorkbenchCapability,
    HaloWorkbenchDeliveryDecision, HaloWorkbenchIntent, HaloWorkbenchIntentRequest,
    HaloWorkbenchInterruptionHistoryPort, HaloWorkbenchMessageRole, HaloWorkbenchOperationDecision,
    HaloWorkbenchOperationRiskLevel, HaloWorkbenchPendingOperationPhase, HaloWorkbenchPhase,
    HaloWorkbenchRuntime, HaloWorkbenchSessionMode, HaloWorkbenchSessionPhase,
    HaloWorkbenchSessionSnapshot, HaloWorkbenchWorkspaceInput, HALO_WORKBENCH_SCHEMA_VERSION,
};
use halo_runtime_ports::{
    ClockPort, ManagedEventFactAppend, ManagedEventFactKind, ManagedEventFactRecord,
    ManagedEventFactStorePort, ManagedExecutorKind, ManagedExecutorPort,
    ManagedExecutorPromptRequest, ManagedExecutorTarget, PiProviderReadiness,
    PiProviderReadinessPort,
    PiRpcAvailabilitySummary, PiRpcCancellationMode, PiRpcCommand, PiRpcEvent, PiRpcFailureKind,
    PiRpcOperationKind, PiRpcOperationRiskLevel, PiRpcOperationSummary, PiRpcPort, PiRpcReply,
    PiRpcVersion, PiRpcVersionEvidenceSource, PortError, PortErrorKind, PortResult,
    RuntimeServiceCapability, RuntimeServicePort, WorkbenchDeliveryAttribution,
    WorkbenchDeliveryAttributionKind, WorkbenchDeliveryEvidence, WorkbenchDeliveryEvidencePort,
    WorkbenchDeliveryEvidenceRequest, WorkbenchDeliveryFingerprint,
    WorkbenchDeliveryFingerprintRequest, WorkbenchTaskBaseline, WorkbenchTaskBaselinePort,
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
            PiRpcCommand::Steer { .. } => None,
            PiRpcCommand::AbortSession { .. } => Some(Self::AbortSession),
            PiRpcCommand::EndSession { .. } => Some(Self::EndSession),
            PiRpcCommand::ResolveOperation { .. } => Some(Self::ResolveOperation),
            PiRpcCommand::Shutdown { .. } => Some(Self::Shutdown),
            // Entry reads never originate from the Workbench Runtime.
            PiRpcCommand::GetEntries { .. } => None,
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
    panic_command: Mutex<Option<CommandKind>>,
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
            panic_command: Mutex::new(None),
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

    fn panic_on(&self, command: CommandKind) {
        *self.panic_command.lock().expect("panic command lock") = Some(command);
    }

    fn take_panic_for(&self, command: &PiRpcCommand) -> bool {
        let mut panic_command = self.panic_command.lock().expect("panic command lock");
        if CommandKind::of(command) != *panic_command {
            return false;
        }
        panic_command.take();
        true
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
        if self.take_panic_for(&command) {
            panic!("controlled Pi RPC test failure");
        }
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
        request: halo_runtime_ports::WorkbenchTaskBaselineRequest,
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
        _request: halo_runtime_ports::WorkbenchTaskBaselineRequest,
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

#[derive(Clone)]
struct FixedDeliveryEvidence {
    fingerprint: WorkbenchDeliveryFingerprint,
    evidence: WorkbenchDeliveryEvidence,
}

impl FixedDeliveryEvidence {
    fn new(
        diff_preview: &str,
        changed_files: Vec<&str>,
        attribution: Vec<(String, WorkbenchDeliveryAttributionKind)>,
        settled_changed_files: Vec<&str>,
    ) -> Self {
        Self {
            fingerprint: WorkbenchDeliveryFingerprint {
                head: "test-head".to_string(),
                changed_files: settled_changed_files
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                working_tree_fingerprint: "c".repeat(64),
                captured_at_ms: 2_000,
            },
            evidence: WorkbenchDeliveryEvidence {
                captured_at_ms: 2_000,
                head: "test-head".to_string(),
                working_tree_fingerprint: "d".repeat(64),
                changed_files: changed_files.into_iter().map(str::to_string).collect(),
                diff_preview: diff_preview.to_string(),
                attribution: attribution
                    .into_iter()
                    .map(|(path, kind)| WorkbenchDeliveryAttribution { path, kind })
                    .collect(),
            },
        }
    }
}

#[async_trait]
impl WorkbenchDeliveryEvidencePort for FixedDeliveryEvidence {
    async fn capture(
        &self,
        _request: WorkbenchDeliveryEvidenceRequest,
    ) -> PortResult<WorkbenchDeliveryEvidence> {
        Ok(self.evidence.clone())
    }

    async fn capture_fingerprint(
        &self,
        _request: WorkbenchDeliveryFingerprintRequest,
    ) -> PortResult<WorkbenchDeliveryFingerprint> {
        Ok(self.fingerprint.clone())
    }
}

struct ControlledDeliveryEvidence {
    fixed: FixedDeliveryEvidence,
    fail_capture: AtomicBool,
    capture_gate: CommandGate,
}

impl ControlledDeliveryEvidence {
    fn new(fixed: FixedDeliveryEvidence) -> Self {
        Self {
            fixed,
            fail_capture: AtomicBool::new(false),
            capture_gate: CommandGate::new(),
        }
    }

    fn fail_capture(&self) {
        self.fail_capture.store(true, Ordering::Release);
    }

    fn allow_capture(&self) {
        self.fail_capture.store(false, Ordering::Release);
    }

    fn block_capture(&self) {
        self.capture_gate.block();
    }
}

#[async_trait]
impl WorkbenchDeliveryEvidencePort for ControlledDeliveryEvidence {
    async fn capture(
        &self,
        _request: WorkbenchDeliveryEvidenceRequest,
    ) -> PortResult<WorkbenchDeliveryEvidence> {
        self.capture_gate.wait_if_enabled().await;
        if self.fail_capture.load(Ordering::Acquire) {
            return Err(PortError::new(
                PortErrorKind::Backend,
                "controlled delivery evidence failure",
            ));
        }
        Ok(self.fixed.evidence.clone())
    }

    async fn capture_fingerprint(
        &self,
        _request: WorkbenchDeliveryFingerprintRequest,
    ) -> PortResult<WorkbenchDeliveryFingerprint> {
        Ok(self.fixed.fingerprint.clone())
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

#[derive(Default)]
struct InMemoryInterruptionHistory {
    sessions: Mutex<Vec<HaloWorkbenchSessionSnapshot>>,
    writes: AtomicUsize,
}

impl InMemoryInterruptionHistory {
    fn write_count(&self) -> usize {
        self.writes.load(Ordering::Acquire)
    }
}

impl HaloWorkbenchInterruptionHistoryPort for InMemoryInterruptionHistory {
    fn load_interrupted_sessions(&self) -> PortResult<Vec<HaloWorkbenchSessionSnapshot>> {
        Ok(self
            .sessions
            .lock()
            .expect("interruption history lock")
            .clone())
    }

    fn replace_interrupted_sessions(
        &self,
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> PortResult<()> {
        *self.sessions.lock().expect("interruption history lock") = sessions;
        self.writes.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

#[derive(Default)]
struct RecordingManagedFacts {
    records: Mutex<Vec<ManagedEventFactRecord>>,
    fail: bool,
}

impl RecordingManagedFacts {
    fn failing() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    fn records(&self) -> Vec<ManagedEventFactRecord> {
        self.records.lock().expect("facts lock").clone()
    }
}

impl ManagedEventFactStorePort for RecordingManagedFacts {
    fn append(&self, fact: ManagedEventFactAppend) -> PortResult<ManagedEventFactRecord> {
        if self.fail {
            return Err(PortError::new(PortErrorKind::Backend, "facts unavailable"));
        }
        let mut records = self.records.lock().expect("facts lock");
        let record = ManagedEventFactRecord {
            task_id: fact.task_id,
            fact_id: fact.fact_id,
            sequence: records.len() as u64 + 1,
            recorded_at_ms: fact.recorded_at_ms,
            schema_version: fact.schema_version,
            kind: fact.kind,
            redacted_summary: fact.redacted_summary,
        };
        records.push(record.clone());
        Ok(record)
    }

    fn read_task(&self, task_id: &str) -> PortResult<Vec<ManagedEventFactRecord>> {
        Ok(self
            .records()
            .into_iter()
            .filter(|fact| fact.task_id == task_id)
            .collect())
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

fn build_runtime_with_delivery_evidence(
    adapter: Arc<DeterministicPiRpc>,
    delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
) -> HaloWorkbenchRuntime {
    HaloWorkbenchRuntime::new_with_delivery_evidence(
        adapter,
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        delivery_evidence,
        Arc::new(FixedClock),
    )
}

fn build_runtime_with_delivery_evidence_and_interruption_history(
    adapter: Arc<DeterministicPiRpc>,
    delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
    interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
) -> HaloWorkbenchRuntime {
    HaloWorkbenchRuntime::try_new_with_delivery_evidence_and_interruption_history(
        adapter,
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        delivery_evidence,
        interruption_history,
        Arc::new(FixedClock),
    )
    .expect("interruption history is restored")
}

fn build_runtime_with_interruption_history(
    adapter: Arc<DeterministicPiRpc>,
    interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
) -> HaloWorkbenchRuntime {
    build_runtime_with_delivery_evidence_and_interruption_history(
        adapter,
        Arc::new(FixedDeliveryEvidence::new(
            "history diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
        interruption_history,
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
                executor: None,
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

#[test]
fn facts_aware_recovery_projects_safe_facts_without_pi_replay() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let facts = Arc::new(RecordingManagedFacts::default());
    *facts.records.lock().expect("facts lock") = vec![
        ManagedEventFactRecord {
            task_id: "task-recovered".to_string(),
            fact_id: "fact-1".to_string(),
            sequence: 1,
            recorded_at_ms: 1_234,
            schema_version: 1,
            kind: ManagedEventFactKind::UserMessageSummary,
            redacted_summary: "safe user summary".to_string(),
        },
        ManagedEventFactRecord {
            task_id: "task-recovered".to_string(),
            fact_id: "fact-2".to_string(),
            sequence: 2,
            recorded_at_ms: 1_235,
            schema_version: 1,
            kind: ManagedEventFactKind::ToolActivity,
            redacted_summary: "safe tool summary".to_string(),
        },
    ];
    let history = Arc::new(InMemoryInterruptionHistory {
        sessions: Mutex::new(vec![HaloWorkbenchSessionSnapshot {
            workspace_id: "workspace-recovered".to_string(),
            task_id: "task-recovered".to_string(),
            session_id: "session-recovered".to_string(),
            mode: HaloWorkbenchSessionMode::Managed,
            phase: HaloWorkbenchSessionPhase::Interrupted,
            executor: ManagedExecutorKind::PiRpc,
            cancellation_mode: None,
            baseline: None,
            messages: Vec::new(),
            activities: Vec::new(),
            error: None,
            delivery_review: None,
        }]),
        writes: AtomicUsize::new(0),
    });
    let runtime = HaloWorkbenchRuntime::try_new_with_delivery_evidence_and_fact_store_and_interruption_history(
        adapter.clone(),
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        Arc::new(FixedDeliveryEvidence::new("unused", Vec::new(), Vec::new(), Vec::new())),
        facts,
        history,
        Arc::new(FixedClock),
    )
    .expect("facts-aware recovery succeeds");
    let session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == "session-recovered")
        .expect("recovered session is visible");
    assert_eq!(session.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(session.messages[0].role, HaloWorkbenchMessageRole::User);
    assert_eq!(session.messages[0].content, "safe user summary");
    assert_eq!(session.activities[0].label, "safe tool summary");
    assert!(adapter.commands().is_empty(), "recovery must not replay Pi");
}

#[tokio::test]
async fn managed_session_records_a_normalized_fact_before_pi_creation() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let facts = Arc::new(RecordingManagedFacts::default());
    let runtime = HaloWorkbenchRuntime::new_with_delivery_evidence_and_fact_store(
        adapter.clone(),
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        Arc::new(FixedDeliveryEvidence::new("diff", vec![], vec![], vec![])),
        facts.clone(),
        Arc::new(FixedClock),
    );
    open_ready(&runtime, &adapter, "facts-open", "facts-workspace").await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "facts-confirm".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "facts-workspace".to_string(),
                root_path: PathBuf::from("C:/work/facts-workspace"),
            },
        })
        .await
        .expect("managed workspace confirmation accepted");

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "facts-create".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "facts-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
                executor: None,
            },
        })
        .await
        .expect("managed create accepted");

    let records = facts.records();
    assert_eq!(records.len(), 2);
    let lifecycle = records
        .iter()
        .find(|record| record.kind == ManagedEventFactKind::TaskLifecycle)
        .expect("task lifecycle fact is recorded");
    assert_eq!(lifecycle.task_id, "facts-task");
    assert_eq!(adapter.count(CommandKind::CreateSession), 1);
}

#[tokio::test]
async fn managed_fact_failure_is_observable_and_prevents_session_and_pi_progress() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = HaloWorkbenchRuntime::new_with_delivery_evidence_and_fact_store(
        adapter.clone(),
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        Arc::new(FixedDeliveryEvidence::new("diff", vec![], vec![], vec![])),
        Arc::new(RecordingManagedFacts::failing()),
        Arc::new(FixedClock),
    );
    open_ready(
        &runtime,
        &adapter,
        "facts-fail-open",
        "facts-fail-workspace",
    )
    .await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "facts-fail-confirm".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "facts-fail-workspace".to_string(),
                root_path: PathBuf::from("C:/work/facts-fail-workspace"),
            },
        })
        .await
        .expect("managed workspace confirmation accepted");

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "facts-fail-create".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "facts-fail-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
                executor: None,
            },
        })
        .await
        .expect_err("facts failure must fail closed");

    assert_eq!(error.code, "managed_event_facts_unavailable");
    assert_eq!(
        runtime
            .snapshot()
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("managed_event_facts_unavailable")
    );
    assert!(runtime.snapshot().sessions.is_empty());
    assert_eq!(adapter.count(CommandKind::CreateSession), 0);
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
                executor: None,
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
                executor: None,
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
                mode: halo_runtime_ports::PiRpcSessionMode::Managed,
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
                executor: None,
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
                executor: None,
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
                executor: None,
            },
        })
        .await
        .expect_err("one active session is allowed per workspace/task binding");
    assert_eq!(error.code, "task_already_active");
    assert_eq!(adapter.count(CommandKind::CreateSession), 1);
}

#[tokio::test]
async fn abort_reaches_adapter_while_a_prompt_response_is_pending() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-abort-priority", "abort-priority").await;
    let session_id =
        create_idle_session(&runtime, &adapter, generation, "abort-priority-session").await;

    adapter.send_user_input_gate.block();
    let prompt_runtime = runtime.clone();
    let prompt_session_id = session_id.clone();
    let prompt = tokio::spawn(async move {
        prompt_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "blocked-prompt".to_string(),
                intent: HaloWorkbenchIntent::SendUserInput {
                    session_id: prompt_session_id,
                    content: "wait for the abort".to_string(),
                },
            })
            .await
    });
    adapter.send_user_input_gate.wait_until_started().await;
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    let abort_runtime = runtime.clone();
    let abort_session_id = session_id.clone();
    let abort = tokio::spawn(async move {
        abort_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "abort-before-prompt-response".to_string(),
                intent: HaloWorkbenchIntent::AbortSession {
                    session_id: abort_session_id,
                },
            })
            .await
    });
    let abort_dispatched = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if adapter.count(CommandKind::AbortSession) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    if !abort_dispatched {
        adapter.send_user_input_gate.release();
    }
    assert!(
        abort_dispatched,
        "abort must cross the Pi RPC port before an in-flight prompt response completes"
    );

    abort
        .await
        .expect("abort task")
        .expect("abort command is accepted");
    adapter.emit(PiRpcEvent::SessionStopped {
        generation,
        session_id: session_id.clone(),
        cancellation_mode: PiRpcCancellationMode::Native,
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    adapter.send_user_input_gate.release();
    let _ = prompt.await.expect("prompt task");
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);
    assert_eq!(adapter.count(CommandKind::AbortSession), 1);
}

#[tokio::test]
async fn shutdown_reclaims_a_managed_prompt_before_its_response_returns() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-shutdown-priority",
        "shutdown-priority",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-shutdown-priority".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "shutdown-priority".to_string(),
                root_path: PathBuf::from("C:/work/shutdown-priority"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "shutdown-priority-session",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;

    adapter.send_user_input_gate.block();
    let prompt_runtime = runtime.clone();
    let prompt_session_id = session_id.clone();
    let prompt = tokio::spawn(async move {
        prompt_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "blocked-managed-prompt".to_string(),
                intent: HaloWorkbenchIntent::SendUserInput {
                    session_id: prompt_session_id,
                    content: "shut down before this returns".to_string(),
                },
            })
            .await
    });
    adapter.send_user_input_gate.wait_until_started().await;
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    let shutdown_runtime = runtime.clone();
    let shutdown = tokio::spawn(async move { shutdown_runtime.shutdown().await });
    let shutdown_dispatched = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if adapter.count(CommandKind::Shutdown) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    if !shutdown_dispatched {
        adapter.send_user_input_gate.release();
    }
    assert!(
        shutdown_dispatched,
        "application shutdown must reclaim the Pi generation before an in-flight prompt response returns"
    );
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;

    adapter.send_user_input_gate.release();
    let _ = prompt.await.expect("prompt task");
    shutdown
        .await
        .expect("shutdown task")
        .expect("shutdown is accepted");
    assert_eq!(runtime.snapshot().phase, HaloWorkbenchPhase::Disconnected);
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
        cancellation_mode: PiRpcCancellationMode::Native,
    });
    wait_for_session_phase(
        &runtime,
        &abort_target,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    assert_eq!(
        runtime
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.session_id == abort_target)
            .and_then(|session| session.cancellation_mode),
        Some(HaloWorkbenchCancellationMode::Native)
    );

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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
                executor: None,
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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
async fn managed_transport_failure_is_interrupted_and_fences_late_events() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
    );
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-managed-interruption",
        "managed-interruption",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-managed-interruption".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "managed-interruption".to_string(),
                root_path: PathBuf::from("C:/work/managed-interruption"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-managed-interruption",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-managed-interruption".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "preserve this user input".to_string(),
            },
        })
        .await
        .expect("the managed session has an in-flight turn");
    assert_eq!(
        runtime
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .expect("managed session remains projected")
            .phase,
        HaloWorkbenchSessionPhase::Running
    );

    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    let interrupted_snapshot = runtime.snapshot();
    let interrupted_version = interrupted_snapshot.state_version;
    let interrupted = interrupted_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("interrupted managed session remains visible for review");
    assert_eq!(interrupted.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        interrupted.error.as_ref().map(|error| error.code.as_str()),
        Some("pi_transport_unavailable")
    );
    assert_eq!(interrupted.messages.len(), 1);
    assert_eq!(interrupted.messages[0].content, "preserve this user input");

    adapter.emit(PiRpcEvent::SessionIdle {
        generation,
        session_id: session_id.clone(),
    });
    adapter.emit(PiRpcEvent::SessionEnded {
        generation,
        session_id: session_id.clone(),
    });
    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Protocol,
    });
    adapter.emit(PiRpcEvent::MessageUpdated {
        generation,
        session_id: session_id.clone(),
        text: "late-message-must-not-append".to_string(),
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: session_id.clone(),
        operation_id: "late-operation-must-not-appear".to_string(),
        kind: PiRpcOperationKind::Permission,
        summary: PiRpcOperationSummary {
            tool_name: "late-tool".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
        redacted_tool_call_id: None,
    });
    adapter.emit(PiRpcEvent::AgentSettled {
        generation,
        session_id,
    });
    tokio::task::yield_now().await;
    let after_late_events = runtime.snapshot();
    assert_eq!(after_late_events.state_version, interrupted_version);
    let after_late_session = after_late_events
        .sessions
        .iter()
        .find(|session| session.task_id == "create-managed-interruption")
        .expect("interrupted managed session remains isolated from late events");
    assert_eq!(
        after_late_session.phase,
        HaloWorkbenchSessionPhase::Interrupted
    );
    assert_eq!(after_late_session.messages.len(), 1);
    assert!(after_late_events.pending_operations.is_empty());
    assert!(!serde_json::to_string(&after_late_events)
        .expect("interrupted snapshot serializes")
        .contains("late-message-must-not-append"));
}

#[tokio::test]
async fn managed_command_transport_failure_is_interrupted_before_a_late_adapter_failure_event() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-command-failure-interruption",
        "command-failure-interruption",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-command-failure-interruption".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "command-failure-interruption".to_string(),
                root_path: PathBuf::from("C:/work/command-failure-interruption"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-command-failure-interruption",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;

    adapter.push_reply(Err(PortError::new(
        PortErrorKind::Backend,
        "controlled transport closure",
    )));
    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-command-failure-interruption".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "keep this input as a non-replayed interruption".to_string(),
            },
        })
        .await
        .expect_err("the closed transport is reported to the request owner");
    assert_eq!(error.code, "adapter_unavailable");
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;

    let interrupted_snapshot = runtime.snapshot();
    let interrupted_version = interrupted_snapshot.state_version;
    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    tokio::task::yield_now().await;

    let after_late_event = runtime.snapshot();
    assert_eq!(after_late_event.state_version, interrupted_version);
    assert!(after_late_event.sessions.iter().any(|session| {
        session.session_id == session_id
            && session.phase == HaloWorkbenchSessionPhase::Interrupted
            && session.messages.len() == 1
    }));
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);
}

#[tokio::test]
async fn managed_adapter_failure_event_cannot_be_overwritten_by_the_inflight_command_result() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-event-first-interruption",
        "event-first-interruption",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-event-first-interruption".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "event-first-interruption".to_string(),
                root_path: PathBuf::from("C:/work/event-first-interruption"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-event-first-interruption",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;

    adapter.send_user_input_gate.block();
    let request_runtime = runtime.clone();
    let request_session_id = session_id.clone();
    let request = tokio::spawn(async move {
        request_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "send-event-first-interruption".to_string(),
                intent: HaloWorkbenchIntent::SendUserInput {
                    session_id: request_session_id,
                    content: "do not let the late command result rewrite interruption".to_string(),
                },
            })
            .await
    });
    adapter.send_user_input_gate.wait_until_started().await;

    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Protocol,
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    let interrupted_version = runtime.snapshot().state_version;

    adapter.push_reply(Err(PortError::new(
        PortErrorKind::Backend,
        "controlled protocol closure",
    )));
    adapter.send_user_input_gate.release();
    let error = request
        .await
        .expect("request task joins")
        .expect_err("the in-flight command observes its adapter failure");
    assert_eq!(error.code, "adapter_unavailable");

    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.state_version, interrupted_version);
    assert!(snapshot.sessions.iter().any(|session| {
        session.session_id == session_id
            && session.phase == HaloWorkbenchSessionPhase::Interrupted
            && session
                .error
                .as_ref()
                .is_some_and(|error| error.code == "pi_protocol_error")
    }));
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);
}

#[tokio::test]
async fn interrupted_managed_session_enters_delivery_review_without_replaying_pi() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "interrupted diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
    );
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-interrupted-review",
        "interrupted-review",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-interrupted-review".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "interrupted-review".to_string(),
                root_path: PathBuf::from("C:/work/interrupted-review"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-interrupted-review",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-interrupted-review".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "keep this interrupted work reviewable".to_string(),
            },
        })
        .await
        .expect("managed request is accepted");
    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    adapter.clear_commands();

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "review-interrupted-session".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("interrupted managed work can enter explicit delivery review");

    let review_snapshot = runtime.snapshot();
    let reviewed = review_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("interrupted session remains visible for review");
    assert_eq!(reviewed.phase, HaloWorkbenchSessionPhase::Reviewing);
    assert_eq!(
        reviewed.error.as_ref().map(|error| error.code.as_str()),
        Some("pi_transport_unavailable")
    );
    assert!(reviewed.delivery_review.is_some());
    assert!(adapter.commands().is_empty());

    let review_version = review_snapshot.state_version;
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: session_id.clone(),
        operation_id: "late-operation-after-review".to_string(),
        kind: PiRpcOperationKind::Permission,
        summary: PiRpcOperationSummary {
            tool_name: "late-write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
        redacted_tool_call_id: None,
    });
    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id,
        reason: PiRpcFailureKind::Protocol,
    });
    tokio::task::yield_now().await;
    let after_late_events = runtime.snapshot();
    assert_eq!(after_late_events.state_version, review_version);
    assert!(after_late_events.pending_operations.is_empty());
    assert_eq!(adapter.commands().len(), 0);
}

#[tokio::test]
async fn shutdown_preserves_managed_interruption_facts_after_pi_cleanup() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-shutdown-interruption",
        "shutdown-interruption",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-shutdown-interruption".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "shutdown-interruption".to_string(),
                root_path: PathBuf::from("C:/work/shutdown-interruption"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-shutdown-interruption",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-shutdown-interruption".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "preserve across shutdown".to_string(),
            },
        })
        .await
        .expect("managed prompt is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    runtime
        .shutdown()
        .await
        .expect("runtime shutdown is accepted");

    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.phase, HaloWorkbenchPhase::Disconnected);
    assert!(snapshot.pending_operations.is_empty());
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed interruption remains visible after shutdown");
    assert_eq!(session.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        session.error.as_ref().map(|error| error.code.as_str()),
        Some("runtime_shutdown")
    );
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].content, "preserve across shutdown");
    assert!(session.baseline.is_some());
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
}

#[tokio::test]
async fn restarted_runtime_does_not_replay_interrupted_managed_work_or_operations() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let interruption_history = Arc::new(InMemoryInterruptionHistory::default());
    let runtime =
        build_runtime_with_interruption_history(adapter.clone(), interruption_history.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-restart-non-replay",
        "restart-non-replay",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-restart-non-replay".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "restart-non-replay".to_string(),
                root_path: PathBuf::from("C:/work/restart-non-replay"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-restart-non-replay",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-restart-non-replay".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "do not replay this managed request".to_string(),
            },
        })
        .await
        .expect("managed request is accepted");
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: session_id.clone(),
        operation_id: "restart-non-replay-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
        redacted_tool_call_id: None,
    });
    wait_for_pending_operation(&runtime, "restart-non-replay-operation").await;

    runtime
        .shutdown()
        .await
        .expect("runtime shutdown is accepted");
    let shutdown_snapshot = runtime.snapshot();
    assert!(shutdown_snapshot.pending_operations.is_empty());
    assert!(shutdown_snapshot.sessions.iter().any(|session| {
        session.session_id == session_id && session.phase == HaloWorkbenchSessionPhase::Interrupted
    }));
    let commands_after_shutdown = adapter.commands().len();

    let restarted = build_runtime_with_interruption_history(adapter.clone(), interruption_history);
    tokio::task::yield_now().await;

    let restarted_snapshot = restarted.snapshot();
    assert_eq!(restarted_snapshot.phase, HaloWorkbenchPhase::Disconnected);
    assert!(restarted_snapshot.pending_operations.is_empty());
    let restored = restarted_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("restart restores only the sanitized interrupted Halo history");
    assert_eq!(restored.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(restored.mode, HaloWorkbenchSessionMode::Managed);
    assert_eq!(
        restored.error.as_ref().map(|error| error.code.as_str()),
        Some("runtime_shutdown")
    );
    assert!(restored.baseline.is_some());
    assert!(restored.messages.is_empty());
    assert!(restored.activities.is_empty());
    assert!(restored.delivery_review.is_none());
    assert_eq!(
        adapter.commands().len(),
        commands_after_shutdown,
        "restart must not reconnect, resend input, or resolve an interrupted operation"
    );
}

#[tokio::test]
async fn restart_retains_frozen_delivery_review_without_active_session_content() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let interruption_history = Arc::new(InMemoryInterruptionHistory::default());
    let runtime = build_runtime_with_delivery_evidence_and_interruption_history(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "reviewed diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
        interruption_history.clone(),
    );
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-restart-frozen-review",
        "restart-frozen-review",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-restart-frozen-review".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "restart-frozen-review".to_string(),
                root_path: PathBuf::from("C:/work/restart-frozen-review"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-restart-frozen-review",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-restart-frozen-review".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "do not retain this active request after restart".to_string(),
            },
        })
        .await
        .expect("managed input is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;
    adapter.emit(PiRpcEvent::ToolExecutionStarted {
        generation,
        session_id: session_id.clone(),
        redacted_tool_call_id: "restart-frozen-review-tool".to_string(),
        tool_name: "write".to_string(),
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let has_activity = runtime
                .snapshot()
                .sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .is_some_and(|session| !session.activities.is_empty());
            if has_activity {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("managed activity is projected before the interruption");
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
            request_id: "finish-restart-frozen-review".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("delivery review is accepted");
    let active_session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("active session remains visible before application loss");
    assert!(!active_session.messages.is_empty());
    assert!(!active_session.activities.is_empty());
    assert!(active_session.delivery_review.is_some());

    drop(runtime);

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if adapter.count(CommandKind::Shutdown) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dropping the old runtime completes its one cleanup command");
    let commands_before_restart = adapter.commands().len();
    let restarted = build_runtime_with_interruption_history(adapter.clone(), interruption_history);
    let restored = restarted
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("restart restores the interrupted delivery review fact");
    assert_eq!(restored.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert!(restored.baseline.is_some());
    assert!(restored.messages.is_empty());
    assert!(restored.activities.is_empty());
    let frozen_history = restored.clone();
    let review = restored
        .delivery_review
        .as_ref()
        .expect("frozen delivery evidence remains available for explicit review");
    assert_eq!(review.decision, None);
    assert_eq!(review.evidence.diff_preview, "reviewed diff");
    assert_eq!(review.evidence.changed_files, vec!["tracked.rs"]);

    let receipt = restarted
        .submit(HaloWorkbenchIntentRequest {
            request_id: "accept-restarted-frozen-review".to_string(),
            intent: HaloWorkbenchIntent::AcceptDelivery {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("a frozen interrupted review can be accepted without restarting Pi");
    assert_eq!(receipt.session_id.as_deref(), Some(session_id.as_str()));

    let resolved = restarted
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("accepted interruption history remains observable for this runtime");
    assert_eq!(resolved.phase, HaloWorkbenchSessionPhase::Ended);
    assert_eq!(
        resolved
            .delivery_review
            .as_ref()
            .and_then(|review| review.decision),
        Some(HaloWorkbenchDeliveryDecision::Accepted)
    );
    assert_eq!(restarted.snapshot().phase, HaloWorkbenchPhase::Disconnected);
    assert_eq!(
        adapter.commands().len(),
        commands_before_restart,
        "review disposition must not reconnect, replay, or issue a Pi command"
    );

    let duplicate_history = Arc::new(InMemoryInterruptionHistory {
        sessions: Mutex::new(vec![frozen_history]),
        writes: AtomicUsize::new(0),
    });
    let duplicate_runtime =
        build_runtime_with_interruption_history(adapter.clone(), duplicate_history);
    open_ready(
        &duplicate_runtime,
        &adapter,
        "reopen-restart-frozen-review",
        "restart-frozen-review",
    )
    .await;
    let commands_before_duplicate_review = adapter.commands().len();
    let duplicate_review = duplicate_runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "duplicate-restart-frozen-review".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect_err("an existing frozen review cannot be recaptured");
    assert_eq!(duplicate_review.code, "delivery_review_not_ready");
    let preserved = duplicate_runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("the interrupted review remains available after the rejected duplicate");
    assert_eq!(preserved.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        preserved
            .delivery_review
            .as_ref()
            .map(|review| review.evidence.diff_preview.as_str()),
        Some("reviewed diff")
    );
    assert_eq!(adapter.commands().len(), commands_before_duplicate_review);
    duplicate_runtime
        .shutdown()
        .await
        .expect("duplicate-review runtime is shut down");
}

#[tokio::test]
async fn forced_runtime_loss_restores_active_managed_work_as_interrupted_without_replay() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let interruption_history = Arc::new(InMemoryInterruptionHistory::default());
    let runtime =
        build_runtime_with_interruption_history(adapter.clone(), interruption_history.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-forced-runtime-loss",
        "forced-runtime-loss",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-forced-runtime-loss".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "forced-runtime-loss".to_string(),
                root_path: PathBuf::from("C:/work/forced-runtime-loss"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-forced-runtime-loss",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-forced-runtime-loss".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "preserve this work after abrupt application loss".to_string(),
            },
        })
        .await
        .expect("managed input is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id: session_id.clone(),
        operation_id: "forced-runtime-loss-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
        redacted_tool_call_id: None,
    });
    wait_for_pending_operation(&runtime, "forced-runtime-loss-operation").await;
    let commands_before_loss = adapter.commands().len();

    drop(runtime);

    let restarted = build_runtime_with_interruption_history(adapter.clone(), interruption_history);
    let restarted_snapshot = restarted.snapshot();
    assert_eq!(restarted_snapshot.phase, HaloWorkbenchPhase::Disconnected);
    assert!(restarted_snapshot.pending_operations.is_empty());
    let interrupted = restarted_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("restart projects an abrupt-loss managed checkpoint as interrupted");
    assert_eq!(interrupted.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(interrupted.mode, HaloWorkbenchSessionMode::Managed);
    assert_eq!(interrupted.cancellation_mode, None);
    assert_eq!(
        interrupted.error.as_ref().map(|error| error.code.as_str()),
        Some("application_interrupted")
    );
    assert!(interrupted.baseline.is_some());
    assert!(interrupted.messages.is_empty());
    assert!(interrupted.activities.is_empty());
    assert_eq!(adapter.commands().len(), commands_before_loss);
}

#[tokio::test]
async fn opening_a_new_workspace_interrupts_the_previous_managed_run_without_replay() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let first_generation = open_ready(
        &runtime,
        &adapter,
        "open-first-managed-interruption",
        "first-managed-interruption",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-first-managed-interruption".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "first-managed-interruption".to_string(),
                root_path: PathBuf::from("C:/work/first-managed-interruption"),
            },
        })
        .await
        .expect("first managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        first_generation,
        "create-first-managed-interruption",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-first-managed-interruption".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "must not replay in the next workspace".to_string(),
            },
        })
        .await
        .expect("first managed run is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;
    let sent_before_replacement = adapter.count(CommandKind::SendUserInput);

    open_ready(
        &runtime,
        &adapter,
        "open-second-managed-interruption",
        "second-managed-interruption",
    )
    .await;

    let interrupted_snapshot = runtime.snapshot();
    let interrupted = interrupted_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("the previous managed session remains available for explicit disposition");
    assert_eq!(interrupted.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        interrupted.error.as_ref().map(|error| error.code.as_str()),
        Some("workspace_closed")
    );
    assert_eq!(interrupted.messages.len(), 1);
    assert_eq!(
        interrupted.messages[0].content,
        "must not replay in the next workspace"
    );
    assert!(interrupted.baseline.is_some());
    assert!(adapter.commands().iter().any(|command| {
        matches!(
            command,
            PiRpcCommand::Shutdown { generation } if *generation == first_generation
        )
    }));
    assert_eq!(
        adapter.count(CommandKind::SendUserInput),
        sent_before_replacement
    );

    adapter.emit(PiRpcEvent::MessageUpdated {
        generation: first_generation,
        session_id: session_id.clone(),
        text: "late-old-generation-message".to_string(),
    });
    adapter.emit(PiRpcEvent::SessionFailed {
        generation: first_generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    tokio::task::yield_now().await;
    let after_late_events = runtime.snapshot();
    let after_late_session = after_late_events
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("late events do not discard the interrupted session");
    assert_eq!(
        after_late_session.phase,
        HaloWorkbenchSessionPhase::Interrupted
    );
    assert_eq!(after_late_session.messages.len(), 1);
    assert_eq!(
        adapter.count(CommandKind::SendUserInput),
        sent_before_replacement
    );
}

#[tokio::test]
async fn interrupted_session_from_replaced_workspace_cannot_enter_delivery_review() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "interrupted diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
    );
    let first_generation = open_ready(
        &runtime,
        &adapter,
        "open-old-interrupted-review",
        "old-interrupted-review",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-old-interrupted-review".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "old-interrupted-review".to_string(),
                root_path: PathBuf::from("C:/work/old-interrupted-review"),
            },
        })
        .await
        .expect("first managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        first_generation,
        "create-old-interrupted-review",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-old-interrupted-review".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "keep the old workspace isolated".to_string(),
            },
        })
        .await
        .expect("first managed request is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    open_ready(
        &runtime,
        &adapter,
        "open-current-interrupted-review",
        "current-interrupted-review",
    )
    .await;
    let commands_before_review = adapter.commands().len();

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "review-old-interrupted-session".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect_err("an old workspace session cannot enter the current delivery review");
    assert_eq!(error.code, "delivery_review_not_ready");

    let old_session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("old interrupted session remains a read-only fact");
    assert_eq!(old_session.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        old_session.error.as_ref().map(|error| error.code.as_str()),
        Some("workspace_closed")
    );
    assert!(old_session.delivery_review.is_none());
    assert_eq!(adapter.commands().len(), commands_before_review);
}

#[tokio::test]
async fn request_execution_panic_interrupts_the_active_managed_session_without_replay() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-join-panic-interruption",
        "join-panic-interruption",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-join-panic-interruption".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "join-panic-interruption".to_string(),
                root_path: PathBuf::from("C:/work/join-panic-interruption"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-join-panic-interruption",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    let request = HaloWorkbenchIntentRequest {
        request_id: "send-join-panic-interruption".to_string(),
        intent: HaloWorkbenchIntent::SendUserInput {
            session_id: session_id.clone(),
            content: "preserve the interrupted request without replay".to_string(),
        },
    };
    let sent_before_panic = adapter.count(CommandKind::SendUserInput);
    adapter.panic_on(CommandKind::SendUserInput);

    let error = runtime
        .submit(request.clone())
        .await
        .expect_err("the request owner reports the join panic");
    assert_eq!(error.code, "runtime_internal");
    wait_for_phase(&runtime, HaloWorkbenchPhase::Failed).await;

    let interrupted_snapshot = runtime.snapshot();
    let interrupted = interrupted_snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session remains available for explicit disposition");
    assert_eq!(interrupted.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        interrupted.error.as_ref().map(|error| error.code.as_str()),
        Some("runtime_internal")
    );
    assert_eq!(interrupted.messages.len(), 1);
    assert!(interrupted.baseline.is_some());
    assert_eq!(
        adapter.count(CommandKind::SendUserInput),
        sent_before_panic + 1
    );
    assert_eq!(
        adapter.count(CommandKind::Shutdown),
        1,
        "a runtime panic must reclaim the active Pi generation"
    );

    assert_eq!(
        runtime
            .submit(request)
            .await
            .expect_err("the failed request is not retried automatically")
            .code,
        "runtime_internal"
    );
    assert_eq!(
        adapter.count(CommandKind::SendUserInput),
        sent_before_panic + 1
    );
}

#[tokio::test]
async fn closed_adapter_event_stream_interrupts_active_managed_work_and_cleans_up_once() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-closed-event-stream",
        "closed-event-stream",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-closed-event-stream".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "closed-event-stream".to_string(),
                root_path: PathBuf::from("C:/work/closed-event-stream"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        "create-closed-event-stream",
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "send-closed-event-stream".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "preserve this interrupted work without replay".to_string(),
            },
        })
        .await
        .expect("managed input is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    adapter.close_events();
    wait_for_phase(&runtime, HaloWorkbenchPhase::Failed).await;
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if adapter.count(CommandKind::Shutdown) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("closed event stream triggers one cleanup");

    let snapshot = runtime.snapshot();
    let interrupted = snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session remains available for explicit disposition");
    assert_eq!(interrupted.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert_eq!(
        interrupted.error.as_ref().map(|error| error.code.as_str()),
        Some("adapter_event_stream_closed")
    );
    assert_eq!(adapter.count(CommandKind::SendUserInput), 1);
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
    tokio::task::yield_now().await;
    assert_eq!(adapter.count(CommandKind::Shutdown), 1);
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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
                executor: None,
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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
                executor: None,
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
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
        redacted_tool_call_id: None,
    });
    adapter.emit(PiRpcEvent::OperationRequested {
        generation,
        session_id,
        operation_id: "deny-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        summary: PiRpcOperationSummary {
            tool_name: "write".to_string(),
            arguments: "{}".to_string(),
            risk_level: PiRpcOperationRiskLevel::Standard,
        },
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
async fn pending_operation_projects_redacted_summary_and_risk_level() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(&runtime, &adapter, "open-summary-risk", "summary-risk").await;
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-summary-risk-session".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "summary-risk-task".to_string(),
                mode: HaloWorkbenchSessionMode::Standard,
                executor: None,
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
        operation_id: "high-risk-operation".to_string(),
        kind: PiRpcOperationKind::Permission,
        summary: PiRpcOperationSummary {
            tool_name: "browser".to_string(),
            arguments: r#"{"action":"[redacted]"}"#.to_string(),
            risk_level: PiRpcOperationRiskLevel::HighRisk,
        },
        redacted_tool_call_id: None,
    });
    wait_for_pending_operation(&runtime, "high-risk-operation").await;

    let pending = runtime.snapshot().pending_operations;
    let operation = pending
        .iter()
        .find(|operation| operation.operation_id == "high-risk-operation")
        .expect("pending high-risk operation");
    assert_eq!(operation.tool_name, "browser");
    assert_eq!(
        operation.risk_level,
        HaloWorkbenchOperationRiskLevel::HighRisk
    );
    assert!(operation.arguments.contains("[redacted]"));
    assert!(!operation.arguments.contains("https://example.test/submit"));
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

async fn settle_managed_session(
    runtime: &HaloWorkbenchRuntime,
    adapter: &Arc<DeterministicPiRpc>,
    workspace_id: &str,
    task_id: &str,
    request_prefix: &str,
) -> (u64, String) {
    let generation = open_ready(
        runtime,
        adapter,
        &format!("open-{request_prefix}"),
        workspace_id,
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: format!("confirm-{request_prefix}"),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: workspace_id.to_string(),
                root_path: PathBuf::from(format!("C:/work/{workspace_id}")),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");
    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: format!("create-{request_prefix}"),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: task_id.to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
                executor: None,
            },
        })
        .await
        .expect("managed session is created");
    let session_id = receipt.session_id.expect("managed session id");
    adapter.emit(PiRpcEvent::SessionCreated {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(runtime, &session_id, HaloWorkbenchSessionPhase::Idle).await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: format!("send-{request_prefix}"),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "Inspect the focused change".to_string(),
            },
        })
        .await
        .expect("managed prompt is accepted");
    wait_for_session_phase(runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;
    adapter.emit(PiRpcEvent::AgentSettled {
        generation,
        session_id: session_id.clone(),
    });
    wait_for_session_phase(
        runtime,
        &session_id,
        HaloWorkbenchSessionPhase::WaitingDeveloper,
    )
    .await;
    (generation, session_id)
}

fn delivery_evidence_for(
    session_id: &str,
    runtime: &HaloWorkbenchRuntime,
) -> Option<halo_agent_runtime::halo_workbench::HaloWorkbenchDeliveryReviewSnapshot> {
    runtime
        .snapshot()
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .and_then(|session| session.delivery_review.clone())
}

#[tokio::test]
async fn failed_interrupted_review_preserves_history_and_allows_explicit_retry() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let evidence = Arc::new(ControlledDeliveryEvidence::new(FixedDeliveryEvidence::new(
        "interrupted review diff",
        vec!["tracked.rs"],
        vec![(
            "tracked.rs".to_string(),
            WorkbenchDeliveryAttributionKind::TaskModification,
        )],
        vec!["tracked.rs"],
    )));
    evidence.fail_capture();
    let history = Arc::new(InMemoryInterruptionHistory::default());
    let runtime = build_runtime_with_delivery_evidence_and_interruption_history(
        adapter.clone(),
        evidence.clone(),
        history.clone(),
    );
    let (generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "interrupted-review-failure",
        "interrupted-review-failure-task",
        "interrupted-review-failure",
    )
    .await;
    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::Interrupted,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if history
                .sessions
                .lock()
                .expect("interruption history lock")
                .iter()
                .any(|session| {
                    session.session_id == session_id
                        && session.phase == HaloWorkbenchSessionPhase::Interrupted
                })
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the interrupted session is persisted before review");
    let writes_before_failed_review = history.write_count();
    let commands_before_review = adapter.commands().len();

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "failed-interrupted-review".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect_err("a failed read-only review is reported to the developer");
    assert_eq!(error.code, "delivery_evidence_unavailable");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if history.write_count() > writes_before_failed_review {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the failed review transition is persisted");

    let after_failed_review = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("interrupted session remains available after a failed review");
    assert_eq!(
        after_failed_review.phase,
        HaloWorkbenchSessionPhase::Interrupted
    );
    assert_eq!(
        after_failed_review
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("pi_transport_unavailable")
    );
    assert!(after_failed_review.baseline.is_some());
    assert!(after_failed_review.delivery_review.is_none());
    assert!(
        history
            .sessions
            .lock()
            .expect("interruption history lock")
            .iter()
            .any(|session| {
                session.session_id == session_id
                    && session.phase == HaloWorkbenchSessionPhase::Interrupted
            }),
        "a failed review must not erase the durable interruption fact"
    );

    let restarted_adapter = Arc::new(DeterministicPiRpc::new());
    let restarted = build_runtime_with_delivery_evidence_and_interruption_history(
        restarted_adapter.clone(),
        evidence.clone(),
        history,
    );
    let restored = restarted
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("restart restores the failed-review interruption fact");
    assert_eq!(restored.phase, HaloWorkbenchSessionPhase::Interrupted);
    assert!(restored.baseline.is_some());
    assert_eq!(
        restored.error.as_ref().map(|error| error.code.as_str()),
        Some("pi_transport_unavailable")
    );
    assert!(restarted_adapter.commands().is_empty());

    evidence.allow_capture();
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "retry-interrupted-review".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("an interrupted review may be retried explicitly");
    let retried = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("retried interruption remains projected");
    assert_eq!(retried.phase, HaloWorkbenchSessionPhase::Reviewing);
    assert!(retried.delivery_review.is_some());
    assert_eq!(adapter.commands().len(), commands_before_review);
}

#[tokio::test]
async fn pi_failure_during_unfrozen_delivery_review_interrupts_and_can_be_retried() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let evidence = Arc::new(ControlledDeliveryEvidence::new(FixedDeliveryEvidence::new(
        "unfrozen review diff",
        vec!["tracked.rs"],
        vec![(
            "tracked.rs".to_string(),
            WorkbenchDeliveryAttributionKind::TaskModification,
        )],
        vec!["tracked.rs"],
    )));
    evidence.block_capture();
    let runtime = build_runtime_with_delivery_evidence(adapter.clone(), evidence.clone());
    let (generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "unfrozen-review-failure",
        "unfrozen-review-failure-task",
        "unfrozen-review-failure",
    )
    .await;
    let commands_before_review = adapter.commands().len();
    let review_runtime = runtime.clone();
    let review_session_id = session_id.clone();
    let review = tokio::spawn(async move {
        review_runtime
            .submit(HaloWorkbenchIntentRequest {
                request_id: "review-while-pi-fails".to_string(),
                intent: HaloWorkbenchIntent::FinishAndReview {
                    session_id: review_session_id,
                },
            })
            .await
    });
    evidence.capture_gate.wait_until_started().await;
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Reviewing).await;

    adapter.emit(PiRpcEvent::SessionFailed {
        generation,
        session_id: session_id.clone(),
        reason: PiRpcFailureKind::Transport,
    });
    let interrupted = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime.snapshot().sessions.iter().any(|session| {
                session.session_id == session_id
                    && session.phase == HaloWorkbenchSessionPhase::Interrupted
            }) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    if !interrupted {
        evidence.capture_gate.release();
    }
    assert!(
        interrupted,
        "a Pi failure before evidence is frozen must be projected as interrupted"
    );

    let interrupted_session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("the interrupted session remains available for review");
    assert!(interrupted_session.baseline.is_some());
    assert!(interrupted_session.delivery_review.is_none());
    assert_eq!(
        interrupted_session
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("pi_transport_unavailable")
    );

    evidence.capture_gate.release();
    review
        .await
        .expect("review task")
        .expect_err("the unfrozen review cannot attach evidence after Pi fails");
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "retry-after-pi-failure".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("the interrupted session can be reviewed explicitly after the Pi failure");
    let retried = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("retried session remains projected");
    assert_eq!(retried.phase, HaloWorkbenchSessionPhase::Reviewing);
    assert!(retried.delivery_review.is_some());
    assert_eq!(adapter.commands().len(), commands_before_review);
}

#[tokio::test]
async fn finish_and_review_freezes_evidence_and_releases_adapter_session() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "diff --git a/tracked.rs b/tracked.rs\n+new content",
            vec!["tracked.rs", "new-file.rs"],
            vec![
                (
                    "already-tracked.rs".to_string(),
                    WorkbenchDeliveryAttributionKind::ExistingUserModification,
                ),
                (
                    "tracked.rs".to_string(),
                    WorkbenchDeliveryAttributionKind::TaskModification,
                ),
            ],
            vec!["tracked.rs"],
        )),
    );
    let (generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "finish-review-freeze",
        "finish-review-freeze-task",
        "finish-review-freeze",
    )
    .await;
    let _ = generation;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "finish-review-freeze-request".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("finish and review is accepted");

    let snapshot = runtime.snapshot();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session remains");
    assert_eq!(session.phase, HaloWorkbenchSessionPhase::Reviewing);
    let review = session
        .delivery_review
        .as_ref()
        .expect("delivery review is frozen");
    assert_eq!(review.decision, None);
    assert_eq!(
        review.evidence.changed_files,
        vec!["tracked.rs", "new-file.rs"]
    );
    assert_eq!(review.evidence.attribution.len(), 2);
    assert_eq!(adapter.count(CommandKind::EndSession), 1);
}

#[tokio::test]
async fn delivery_review_redacts_sensitive_diff_preview() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "Authorization: Basic diff-basic-canary Cookie: diff-cookie-canary password=diff-password-canary token=diff-token-canary sessionId=diff-session-id-canary",
            vec!["tracked.rs"],
            vec![("tracked.rs".to_string(), WorkbenchDeliveryAttributionKind::TaskModification)],
            vec!["tracked.rs"],
        )),
    );
    let (_generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "finish-review-redact",
        "finish-review-redact-task",
        "finish-review-redact",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "finish-review-redact-request".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("finish and review is accepted");

    let review = delivery_evidence_for(&session_id, &runtime).expect("review frozen");
    for canary in [
        "diff-basic-canary",
        "diff-cookie-canary",
        "diff-password-canary",
        "diff-token-canary",
        "diff-session-id-canary",
    ] {
        assert!(
            !review.evidence.diff_preview.contains(canary),
            "diff preview leaked canary: {canary}"
        );
    }
}

#[tokio::test]
async fn accept_delivery_records_conclusion_without_new_adapter_commands() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
    );
    let (_generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "accept-delivery",
        "accept-delivery-task",
        "accept-delivery",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "accept-delivery-finish".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("finish and review is accepted");
    let commands_after_finish = adapter.commands().len();

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "accept-delivery-decision".to_string(),
            intent: HaloWorkbenchIntent::AcceptDelivery {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("accept delivery is recorded");

    let snapshot = runtime.snapshot();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session remains");
    assert_eq!(session.phase, HaloWorkbenchSessionPhase::Ended);
    let review = session
        .delivery_review
        .as_ref()
        .expect("delivery review remains frozen");
    assert_eq!(
        review.decision,
        Some(HaloWorkbenchDeliveryDecision::Accepted)
    );
    assert_eq!(
        adapter.commands().len(),
        commands_after_finish,
        "accepting a delivery must not issue any adapter command"
    );
}

#[tokio::test]
async fn reject_delivery_records_conclusion_and_ends_session() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
    );
    let (_generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "reject-delivery",
        "reject-delivery-task",
        "reject-delivery",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "reject-delivery-finish".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("finish and review is accepted");

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "reject-delivery-decision".to_string(),
            intent: HaloWorkbenchIntent::RejectDelivery {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect("reject delivery is recorded");

    let session = runtime
        .snapshot()
        .sessions
        .iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session remains")
        .clone();
    assert_eq!(session.phase, HaloWorkbenchSessionPhase::Ended);
    assert_eq!(
        session.delivery_review.as_ref().unwrap().decision,
        Some(HaloWorkbenchDeliveryDecision::Rejected)
    );
}

#[tokio::test]
async fn finish_and_review_is_rejected_outside_waiting_developer() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime_with_delivery_evidence(
        adapter.clone(),
        Arc::new(FixedDeliveryEvidence::new(
            "diff",
            vec!["tracked.rs"],
            vec![(
                "tracked.rs".to_string(),
                WorkbenchDeliveryAttributionKind::TaskModification,
            )],
            vec!["tracked.rs"],
        )),
    );
    let (_generation, session_id) = settle_managed_session(
        &runtime,
        &adapter,
        "finish-gating",
        "finish-gating-task",
        "finish-gating",
    )
    .await;
    // Move the session into Running via a follow-up, then finish must fail.
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "finish-gating-follow-up".to_string(),
            intent: HaloWorkbenchIntent::FollowUp {
                session_id: session_id.clone(),
                content: "Keep going".to_string(),
            },
        })
        .await
        .expect("follow-up is accepted");
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Running).await;

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "finish-gating-request".to_string(),
            intent: HaloWorkbenchIntent::FinishAndReview {
                session_id: session_id.clone(),
            },
        })
        .await
        .expect_err("finish must be rejected while running");
    assert_eq!(error.code, "delivery_review_not_ready");
    assert_eq!(adapter.count(CommandKind::EndSession), 0);
}

async fn managed_session_with_facts(
    facts: &Arc<RecordingManagedFacts>,
    prefix: &str,
) -> (HaloWorkbenchRuntime, Arc<DeterministicPiRpc>, u64, String) {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = HaloWorkbenchRuntime::new_with_delivery_evidence_and_fact_store(
        adapter.clone(),
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        Arc::new(FixedDeliveryEvidence::new("unused", Vec::new(), Vec::new(), Vec::new())),
        facts.clone(),
        Arc::new(FixedClock),
    );
    let generation = open_ready(
        &runtime,
        &adapter,
        &format!("{prefix}-open"),
        &format!("{prefix}-workspace"),
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: format!("{prefix}-confirm"),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: format!("{prefix}-workspace"),
                root_path: PathBuf::from(format!("C:/work/{}-workspace", prefix)),
            },
        })
        .await
        .expect("managed workspace confirmation accepted");
    let session_id = create_session_with_mode(
        &runtime,
        &adapter,
        generation,
        &format!("{prefix}-create"),
        HaloWorkbenchSessionMode::Managed,
    )
    .await;
    (runtime, adapter, generation, session_id)
}

async fn wait_for_fact_count(facts: &RecordingManagedFacts, count: usize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if facts.records().len() >= count {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("fact log reaches expected size");
}

#[tokio::test]
async fn token_level_stream_frames_leave_the_fact_log_unchanged() {
    let facts = Arc::new(RecordingManagedFacts::default());
    let (runtime, adapter, generation, session_id) =
        managed_session_with_facts(&facts, "stream").await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "stream-prompt".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "stream prompt".to_string(),
            },
        })
        .await
        .expect("managed prompt accepted");
    wait_for_fact_count(&facts, 3).await;
    let before_frames = facts.records().len();

    adapter
        .emit(PiRpcEvent::MessageUpdated {
            generation,
            session_id: session_id.clone(),
            text: "Solving".to_string(),
        });
    adapter
        .emit(PiRpcEvent::MessageUpdated {
            generation,
            session_id: session_id.clone(),
            text: " the bug.".to_string(),
        });
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let reached = runtime
                .snapshot()
                .sessions
                .iter()
                .any(|session| {
                    session.session_id == session_id
                        && session
                            .messages
                            .last()
                            .is_some_and(|message| message.content == "Solving the bug.")
                });
            if reached {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("live record shows both streamed frames");

    // Token-level streaming frames only reach the activity session record.
    assert_eq!(facts.records().len(), before_frames);
}

#[tokio::test]
async fn settled_reply_lands_exactly_one_committed_reply_fact() {
    let facts = Arc::new(RecordingManagedFacts::default());
    let (runtime, adapter, generation, session_id) =
        managed_session_with_facts(&facts, "settle").await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "settle-prompt".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "settle prompt".to_string(),
            },
        })
        .await
        .expect("managed prompt accepted");
    wait_for_fact_count(&facts, 3).await;

    adapter
        .emit(PiRpcEvent::MessageUpdated {
            generation,
            session_id: session_id.clone(),
            text: "Solving".to_string(),
        });
    adapter
        .emit(PiRpcEvent::MessageUpdated {
            generation,
            session_id: session_id.clone(),
            text: " the bug.".to_string(),
        });
    adapter
        .emit(PiRpcEvent::AgentSettled {
            generation,
            session_id: session_id.clone(),
        });
    wait_for_session_phase(
        &runtime,
        &session_id,
        HaloWorkbenchSessionPhase::WaitingDeveloper,
    )
    .await;
    wait_for_fact_count(&facts, 4).await;

    let replies = facts
        .records()
        .into_iter()
        .filter(|record| record.kind == ManagedEventFactKind::AgentReplySummary)
        .collect::<Vec<_>>();
    assert_eq!(replies.len(), 1, "settlement commits exactly one reply fact");
}

#[tokio::test]
async fn cancellation_lands_delivered_prefix_and_interrupted_marker_without_completion() {
    let facts = Arc::new(RecordingManagedFacts::default());
    let (runtime, adapter, generation, session_id) =
        managed_session_with_facts(&facts, "cancel").await;

    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "cancel-prompt".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id: session_id.clone(),
                content: "cancel prompt".to_string(),
            },
        })
        .await
        .expect("managed prompt accepted");
    wait_for_fact_count(&facts, 3).await;
    adapter
        .emit(PiRpcEvent::MessageUpdated {
            generation,
            session_id: session_id.clone(),
            text: "partial reply".to_string(),
        });
    adapter
        .emit(PiRpcEvent::AgentSettled {
            generation,
            session_id: session_id.clone(),
        });
    wait_for_fact_count(&facts, 4).await;
    let before_cancel = facts.records();

    adapter
        .emit(PiRpcEvent::SessionStopped {
            generation,
            session_id: session_id.clone(),
            cancellation_mode: PiRpcCancellationMode::Forced,
        });
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Interrupted).await;
    wait_for_fact_count(&facts, 5).await;

    let records = facts.records();
    // The delivered prefix stays recorded and the cancellation appends the
    // interrupted lifecycle marker; nothing is rewritten into a completion.
    assert_eq!(records.len(), before_cancel.len() + 1);
    assert_eq!(
        records.last().expect("interrupted fact").kind,
        ManagedEventFactKind::TaskInterrupted
    );
    assert_eq!(
        records[..records.len() - 1],
        before_cancel,
        "the delivered prefix is preserved unchanged"
    );
    let kinds = records
        .iter()
        .map(|record| record.kind)
        .collect::<Vec<_>>();
    assert!(kinds.contains(&ManagedEventFactKind::UserMessageSummary));
    assert!(kinds.contains(&ManagedEventFactKind::AgentReplySummary));
    assert!(!kinds.contains(&
 ManagedEventFactKind::AttemptFailed));
}

#[tokio::test]
async fn failed_managed_session_records_an_independent_attempt_fact() {
    let facts = Arc::new(RecordingManagedFacts::default());
    let (runtime, adapter, generation, session_id) =
        managed_session_with_facts(&facts, "attempt").await;
    wait_for_fact_count(&facts, 2).await;
    let before_failure = facts.records();

    adapter
        .emit(PiRpcEvent::SessionFailed {
            generation,
            session_id: session_id.clone(),
            reason: PiRpcFailureKind::Protocol,
        });
    wait_for_session_phase(&runtime, &session_id, HaloWorkbenchSessionPhase::Interrupted).await;
    wait_for_fact_count(&facts, 3).await;

    let records = facts.records();
    assert_eq!(records.len(), before_failure.len() + 1);
    let attempt = records.last().expect("attempt fact");
    assert_eq!(attempt.kind, ManagedEventFactKind::AttemptFailed);
    assert!(
        attempt.redacted_summary.contains("protocol"),
        "the attempt fact records the failure reason"
    );
}

#[test]
fn attempt_and_interrupted_facts_stay_out_of_the_model_visible_rebuild() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let facts = Arc::new(RecordingManagedFacts::default());
    *facts.records.lock().expect("facts lock") = vec![
        ManagedEventFactRecord {
            task_id: "task-recovered".to_string(),
            fact_id: "fact-1".to_string(),
            sequence: 1,
            recorded_at_ms: 1_234,
            schema_version: 2,
            kind: ManagedEventFactKind::UserMessageSummary,
            redacted_summary: "safe user summary".to_string(),
        },
        ManagedEventFactRecord {
            task_id: "task-recovered".to_string(),
            fact_id: "fact-2".to_string(),
            sequence: 2,
            recorded_at_ms: 1_235,
            schema_version: 2,
            kind: ManagedEventFactKind::AgentReplySummary,
            redacted_summary: "safe reply summary".to_string(),
        },
        ManagedEventFactRecord {
            task_id: "task-recovered".to_string(),
            fact_id: "fact-3".to_string(),
            sequence: 3,
            recorded_at_ms: 1_236,
            schema_version: 2,
            kind: ManagedEventFactKind::AttemptFailed,
            redacted_summary: "Managed attempt 1 failed: protocol".to_string(),
        },
        ManagedEventFactRecord {
            task_id: "task-recovered".to_string(),
            fact_id: "fact-4".to_string(),
            sequence: 4,
            recorded_at_ms: 1_237,
            schema_version: 2,
            kind: ManagedEventFactKind::TaskInterrupted,
            redacted_summary: "Managed task interrupted; delivered prefix preserved".to_string(),
        },
    ];
    let history = Arc::new(InMemoryInterruptionHistory {
        sessions: Mutex::new(vec![HaloWorkbenchSessionSnapshot {
            workspace_id: "workspace-recovered".to_string(),
            task_id: "task-recovered".to_string(),
            session_id: "session-recovered".to_string(),
            mode: HaloWorkbenchSessionMode::Managed,
            phase: HaloWorkbenchSessionPhase::Interrupted,
            executor: ManagedExecutorKind::PiRpc,
            cancellation_mode: None,
            baseline: None,
            messages: Vec::new(),
            activities: Vec::new(),
            error: None,
            delivery_review: None,
        }]),
        writes: AtomicUsize::new(0),
    });
    let runtime = HaloWorkbenchRuntime::try_new_with_delivery_evidence_and_fact_store_and_interruption_history(
        adapter.clone(),
        Arc::new(TrustedWorkspaceFacts),
        Arc::new(AvailableProviderReadiness),
        Arc::new(FixedTaskBaseline),
        Arc::new(FixedDeliveryEvidence::new("unused", Vec::new(), Vec::new(), Vec::new())),
        facts,
        history,
        Arc::new(FixedClock),
    )
    .expect("facts-aware recovery succeeds");
    let session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == "session-recovered")
        .expect("recovered session is visible");

    // Attempt and interrupted facts never enter the model-visible rebuild.
    let message_contents = session
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        message_contents,
        vec!["safe user summary", "safe reply summary"]
    );
    assert!(session
        .messages
        .iter()
        .all(|message| !message.content.contains("attempt")
            && !message.content.contains("interrupted")));
    assert!(session
        .activities
        .iter()
        .all(|activity| !activity.label.contains("attempt")
            && !activity.label.contains("interrupted")));
    assert!(adapter.commands().is_empty(), "recovery must not replay Pi");
}

#[derive(Default)]
struct RecordingExecutor {
    prompts: Mutex<Vec<String>>,
    follow_ups: Mutex<Vec<String>>,
    aborts: Mutex<Vec<String>>,
}

impl RecordingExecutor {
    fn prompt_count(&self) -> usize {
        self.prompts.lock().expect("prompt lock").len()
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().expect("prompt lock").clone()
    }
}

#[async_trait]
impl ManagedExecutorPort for RecordingExecutor {
    fn capability_profile(&self) -> halo_runtime_ports::ManagedExecutorCapabilityProfile {
        halo_runtime_ports::ManagedExecutorCapabilityProfile {
            adapter_identity: "recording-fake".to_string(),
            compatibility_profile: "recording-fake-p0".to_string(),
            steer: false,
            queue_events: false,
            approval_channel: true,
            entry_read: false,
            native_sandbox_modes: false,
        }
    }

    fn sandbox_facts(&self) -> halo_runtime_ports::ManagedExecutorSandboxFacts {
        halo_runtime_ports::ManagedExecutorSandboxFacts {
            mode: halo_runtime_ports::ManagedExecutorSandboxMode::DangerFullAccess,
            enforcement: halo_runtime_ports::ManagedExecutorSandboxEnforcement::Partial,
        }
    }

    async fn prompt(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        self.prompts
            .lock()
            .expect("prompt lock")
            .push(request.content);
        Ok(())
    }

    async fn follow_up(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        self.follow_ups
            .lock()
            .expect("follow-up lock")
            .push(request.content);
        Ok(())
    }

    async fn abort(
        &self,
        target: ManagedExecutorTarget,
    ) -> PortResult<halo_runtime_ports::ManagedExecutorAbortOutcome> {
        self.aborts
            .lock()
            .expect("abort lock")
            .push(target.session_id);
        Ok(halo_runtime_ports::ManagedExecutorAbortOutcome::Cooperative)
    }

    async fn read_entries(
        &self,
        _target: ManagedExecutorTarget,
    ) -> PortResult<halo_runtime_ports::ManagedExecutorEntryPage> {
        Err(PortError::new(
            PortErrorKind::NotAvailable,
            "recording fake has no entry reads",
        ))
    }

    async fn resolve_approval(
        &self,
        _decision: halo_runtime_ports::ManagedExecutorApprovalDecision,
    ) -> PortResult<()> {
        Err(PortError::new(
            PortErrorKind::NotAvailable,
            "recording fake has no approval resolution",
        ))
    }

    fn subscribe(
        &self,
    ) -> tokio::sync::broadcast::Receiver<halo_runtime_ports::ManagedExecutorEvent> {
        let (_, receiver) = tokio::sync::broadcast::channel(1);
        receiver
    }
}

#[tokio::test]
async fn executor_selection_lists_installed_production_executors_and_defaults_to_pi() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());

    assert_eq!(
        runtime.workspace_default_executor(),
        ManagedExecutorKind::PiRpc
    );
    assert_eq!(
        runtime.available_managed_executors(),
        vec![ManagedExecutorKind::PiRpc],
        "only installed production executors are selectable"
    );

    let refused = runtime
        .set_workspace_default_executor(ManagedExecutorKind::Dsh)
        .expect_err("an uninstalled executor cannot become the workspace default");
    assert_eq!(refused.code, "executor_unavailable");

    runtime.install_managed_executor(
        ManagedExecutorKind::Dsh,
        Arc::new(RecordingExecutor::default()),
    );
    assert_eq!(
        runtime.available_managed_executors(),
        vec![ManagedExecutorKind::PiRpc, ManagedExecutorKind::Dsh]
    );
    runtime
        .set_workspace_default_executor(ManagedExecutorKind::Dsh)
        .expect("an installed executor becomes the workspace default");
    assert_eq!(
        runtime.workspace_default_executor(),
        ManagedExecutorKind::Dsh
    );
}

#[tokio::test]
async fn task_creation_override_binds_the_executor_into_session_and_baseline() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let dsh = Arc::new(RecordingExecutor::default());
    runtime.install_managed_executor(ManagedExecutorKind::Dsh, dsh.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-executor-binding",
        "executor-binding",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-executor-binding".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "executor-binding".to_string(),
                root_path: PathBuf::from("C:/work/executor-binding"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");

    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-executor-override-task".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "executor-override-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
                executor: Some(ManagedExecutorKind::Dsh),
            },
        })
        .await
        .expect("managed session with an executor override is created");
    let session_id = receipt.session_id.expect("managed session id");
    let session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session is projected");
    assert_eq!(session.executor, ManagedExecutorKind::Dsh);
    let baseline = session.baseline.expect("managed baseline is captured");
    assert_eq!(
        baseline.executor, ManagedExecutorKind::Dsh,
        "the task baseline records the selected executor"
    );
    assert_eq!(adapter.count(CommandKind::CreateSession), 1);

    // No in-session switch: the binding is fixed for the task lifetime and
    // the runtime exposes no intent that would rewrite it.
    adapter.emit(PiRpcEvent::SessionIdle {
        generation,
        session_id: session_id.clone(),
    });
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "prompt-executor-override-task".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id,
                content: "run through the bound executor".to_string(),
            },
        })
        .await
        .expect("managed prompt is accepted");
    assert_eq!(
        dsh.prompt_count(),
        1,
        "the bound executor receives the prompt through the port"
    );
    assert_eq!(
        adapter.count(CommandKind::SendUserInput),
        0,
        "a non-pi bound executor must not leak Pi commands"
    );
}

#[tokio::test]
async fn workspace_default_executor_receives_tasks_created_without_override() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let pi_recorder = Arc::new(RecordingExecutor::default());
    // Replacing the default pi binding with a recording executor proves the
    // runtime dispatches the managed execution face through the port.
    runtime.install_managed_executor(ManagedExecutorKind::PiRpc, pi_recorder.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-executor-default",
        "executor-default",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-executor-default".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "executor-default".to_string(),
                root_path: PathBuf::from("C:/work/executor-default"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");

    let receipt = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-executor-default-task".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "executor-default-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
                executor: None,
            },
        })
        .await
        .expect("managed session with the workspace default is created");
    let session_id = receipt.session_id.expect("managed session id");
    let session = runtime
        .snapshot()
        .sessions
        .into_iter()
        .find(|session| session.session_id == session_id)
        .expect("managed session is projected");
    assert_eq!(session.executor, ManagedExecutorKind::PiRpc);

    adapter.emit(PiRpcEvent::SessionIdle {
        generation,
        session_id: session_id.clone(),
    });
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "prompt-executor-default-task".to_string(),
            intent: HaloWorkbenchIntent::SendUserInput {
                session_id,
                content: "default executor prompt".to_string(),
            },
        })
        .await
        .expect("managed prompt is accepted");
    assert_eq!(
        pi_recorder.prompts(),
        vec!["default executor prompt".to_string()],
        "the workspace default executor receives the prompt"
    );
    assert_eq!(adapter.count(CommandKind::SendUserInput), 0);
}

#[tokio::test]
async fn an_override_naming_an_uninstalled_executor_fails_closed() {
    let adapter = Arc::new(DeterministicPiRpc::new());
    let runtime = build_runtime(adapter.clone());
    let generation = open_ready(
        &runtime,
        &adapter,
        "open-executor-refusal",
        "executor-refusal",
    )
    .await;
    runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "confirm-executor-refusal".to_string(),
            intent: HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id: "executor-refusal".to_string(),
                root_path: PathBuf::from("C:/work/executor-refusal"),
            },
        })
        .await
        .expect("managed workspace confirmation is accepted");

    let error = runtime
        .submit(HaloWorkbenchIntentRequest {
            request_id: "create-uninstalled-executor-task".to_string(),
            intent: HaloWorkbenchIntent::CreateSession {
                task_id: "uninstalled-executor-task".to_string(),
                mode: HaloWorkbenchSessionMode::Managed,
                executor: Some(ManagedExecutorKind::Dsh),
            },
        })
        .await
        .expect_err("an uninstalled executor override fails closed");
    assert_eq!(error.code, "executor_unavailable");
    assert_eq!(adapter.count(CommandKind::CreateSession), 0);
    assert!(
        runtime
            .snapshot()
            .sessions
            .iter()
            .all(|session| session.task_id != "uninstalled-executor-task"),
        "no session state may remain for the refused executor"
    );
}
