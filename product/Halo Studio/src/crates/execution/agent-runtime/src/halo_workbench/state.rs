//! Halo Workbench Runtime internal state: runtime state machine, adapter
//! bindings and the legacy Pi transport bridge.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::managed_event_facts::{
    normalize_summary, HaloFactId, HaloTaskId, ManagedEventFact,
    ManagedEventFactInput, ManagedEventFactKind, ManagedEventFacts, ManagedEventFactsPortAdapter,
};

use halo_runtime_ports::{
    ClockPort, ManagedEventFactStorePort, ManagedExecutorKind, ManagedExecutorPort,
    ManagedExecutorPromptRequest, ManagedExecutorTarget, PiProviderReadinessPort, PiRpcCommand, PiRpcEvent, PiRpcFailureKind, PiRpcPort, PiRpcReply,
    PortErrorKind, PortResult, WorkbenchDeliveryEvidence,
    WorkbenchDeliveryEvidencePort, WorkbenchDeliveryEvidenceRequest, WorkbenchDeliveryFingerprint,
    WorkbenchDeliveryFingerprintRequest, WorkbenchTaskBaseline, WorkbenchTaskBaselinePort,
    WorkbenchTaskBaselineRequest, WorkbenchWorkspaceFactsPort, PI_RPC_ADAPTER_IDENTITY,
};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, watch, OnceCell};
use super::vocabulary::*;
use super::redaction::*;

pub(super) type IntentResult = Result<HaloWorkbenchIntentReceipt, HaloWorkbenchError>;
pub(super) type CleanupResult = Result<(), HaloWorkbenchError>;

pub(super) enum RequestRecord {
    InFlight {
        fingerprint: [u8; 32],
        result: watch::Sender<Option<IntentResult>>,
    },
    Complete {
        fingerprint: [u8; 32],
        result: IntentResult,
    },
}

#[derive(Default)]
pub(super) struct RequestLedger {
    pub(super) records: HashMap<String, RequestRecord>,
}

impl RequestLedger {
    pub(super) fn record_complete(&mut self, request_id: String, fingerprint: [u8; 32], result: IntentResult) {
        self.records.insert(
            request_id,
            RequestRecord::Complete {
                fingerprint,
                result,
            },
        );
        while self
            .records
            .values()
            .filter(|record| matches!(record, RequestRecord::Complete { .. }))
            .count()
            > MAX_COMPLETED_REQUEST_RECORDS
        {
            let Some(request_id) = self.records.iter().find_map(|(request_id, record)| {
                matches!(record, RequestRecord::Complete { .. }).then_some(request_id.clone())
            }) else {
                break;
            };
            self.records.remove(&request_id);
        }
    }
}

pub(super) enum CleanupRecord {
    InFlight {
        result: watch::Sender<Option<CleanupResult>>,
    },
    Complete(CleanupResult),
}

pub(super) struct RuntimeState {
    pub(super) phase: HaloWorkbenchPhase,
    pub(super) adapter_available: bool,
    pub(super) adapter_readiness: Option<HaloWorkbenchAdapterReadiness>,
    pub(super) workspace: Option<HaloWorkbenchWorkspaceSnapshot>,
    pub(super) sessions: BTreeMap<String, HaloWorkbenchSessionSnapshot>,
    pub(super) pending_operations: BTreeMap<String, HaloWorkbenchPendingOperationSnapshot>,
    pub(super) settled_fingerprints: BTreeMap<String, watch::Receiver<Option<WorkbenchDeliveryFingerprint>>>,
    pub(super) error: Option<HaloWorkbenchError>,
    pub(super) sequence: u64,
    pub(super) state_version: u64,
    pub(super) generation: u64,
    pub(super) adapter_generation: Option<u64>,
    pub(super) managed_workspace_confirmation: Option<ManagedWorkspaceConfirmation>,
    pub(super) cleanup_started: HashSet<u64>,
    pub(super) terminated: bool,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            phase: HaloWorkbenchPhase::Disconnected,
            adapter_available: false,
            adapter_readiness: None,
            workspace: None,
            sessions: BTreeMap::new(),
            pending_operations: BTreeMap::new(),
            settled_fingerprints: BTreeMap::new(),
            error: None,
            sequence: 0,
            state_version: 0,
            generation: 0,
            adapter_generation: None,
            managed_workspace_confirmation: None,
            cleanup_started: HashSet::new(),
            terminated: false,
        }
    }
}

impl RuntimeState {
    pub(super) fn from_interruption_history(
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> Result<Self, HaloWorkbenchError> {
        let mut state = Self::default();
        for session in sessions {
            if session.workspace_id.is_empty()
                || session.task_id.is_empty()
                || session.session_id.is_empty()
                || session.mode != HaloWorkbenchSessionMode::Managed
                || session.phase != HaloWorkbenchSessionPhase::Interrupted
                || !session.messages.is_empty()
                || !session.activities.is_empty()
            {
                return Err(HaloWorkbenchError::interruption_history_unavailable());
            }
            if state
                .sessions
                .insert(session.session_id.clone(), session)
                .is_some()
            {
                return Err(HaloWorkbenchError::interruption_history_unavailable());
            }
        }
        Ok(state)
    }

    pub(super) fn from_fact_history(
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
        facts_by_task: BTreeMap<String, Vec<ManagedEventFact>>,
    ) -> Result<Self, HaloWorkbenchError> {
        let mut state = Self::default();
        for session in sessions {
            let facts = facts_by_task
                .get(&session.task_id)
                .ok_or_else(HaloWorkbenchError::managed_event_facts_unavailable)?;
            let mut session = session;
            session.phase = HaloWorkbenchSessionPhase::Interrupted;
            session.messages = facts
                .iter()
                .filter_map(|fact| match fact.kind {
                    ManagedEventFactKind::UserMessageSummary => {
                        Some(HaloWorkbenchMessageSnapshot {
                            role: HaloWorkbenchMessageRole::User,
                            content: fact.redacted_summary.as_str().to_string(),
                        })
                    }
                    ManagedEventFactKind::AgentReplySummary => Some(HaloWorkbenchMessageSnapshot {
                        role: HaloWorkbenchMessageRole::Assistant,
                        content: fact.redacted_summary.as_str().to_string(),
                    }),
                    _ => None,
                })
                .collect();
            session.activities = facts
                .iter()
                .filter_map(|fact| match fact.kind {
                    ManagedEventFactKind::ToolActivity => {
                        let summary = fact.redacted_summary.as_str();
                        let (status, is_error) = if summary.contains("failed") {
                            (HaloWorkbenchActivityStatus::Failed, true)
                        } else if summary.contains("started") {
                            (HaloWorkbenchActivityStatus::Started, false)
                        } else if summary.contains("updated") {
                            (HaloWorkbenchActivityStatus::Updated, false)
                        } else {
                            (HaloWorkbenchActivityStatus::Completed, false)
                        };
                        Some(HaloWorkbenchActivitySnapshot {
                            activity_id: format!("fact-{}", fact.sequence),
                            kind: HaloWorkbenchActivityKind::Tool,
                            label: summary.to_string(),
                            status,
                            is_error,
                        })
                    }
                    _ => None,
                })
                .collect();
            if session.workspace_id.is_empty()
                || session.task_id.is_empty()
                || session.session_id.is_empty()
                || session.mode != HaloWorkbenchSessionMode::Managed
                || session.phase != HaloWorkbenchSessionPhase::Interrupted
            {
                return Err(HaloWorkbenchError::interruption_history_unavailable());
            }
            if state
                .sessions
                .insert(session.session_id.clone(), session)
                .is_some()
            {
                return Err(HaloWorkbenchError::interruption_history_unavailable());
            }
        }
        Ok(state)
    }
}

pub(super) struct InterruptionHistoryState {
    pub(super) persisted_sessions: Vec<HaloWorkbenchSessionSnapshot>,
    // State is snapshotted before persistence so adapters cannot block the
    // runtime lock. This high-water mark prevents a delayed old snapshot from
    // overwriting a later interruption fact.
    pub(super) last_observed_state_version: u64,
}

impl InterruptionHistoryState {
    pub(super) fn new(persisted_sessions: Vec<HaloWorkbenchSessionSnapshot>) -> Self {
        Self {
            persisted_sessions,
            last_observed_state_version: 0,
        }
    }

    pub(super) fn should_persist(
        &mut self,
        state_version: u64,
        sessions: &[HaloWorkbenchSessionSnapshot],
    ) -> bool {
        if state_version < self.last_observed_state_version {
            return false;
        }
        self.last_observed_state_version = state_version;
        self.persisted_sessions.as_slice() != sessions
    }

    pub(super) fn mark_persisted(&mut self, sessions: Vec<HaloWorkbenchSessionSnapshot>) {
        self.persisted_sessions = sessions;
    }
}

pub(super) struct HaloWorkbenchRuntimeInner {
    pub(super) adapter: Arc<dyn PiRpcPort>,
    pub(super) workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
    pub(super) task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
    pub(super) delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
    pub(super) interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
    pub(super) provider_readiness: Arc<dyn PiProviderReadinessPort>,
    pub(super) clock: Arc<dyn ClockPort>,
    /// Managed executor bindings installed behind the unified port
    /// (ADR-0078 M3). The default constructor binds the injected Pi RPC
    /// port through the runtime bridge; the composition root replaces it
    /// with the production adapter wrapper and may add further executors.
    pub(super) managed_executors: Mutex<HashMap<ManagedExecutorKind, Arc<dyn ManagedExecutorPort>>>,
    /// The workspace default executor used when task creation passes no
    /// override. Only installed executors are selectable.
    pub(super) workspace_default_executor: Mutex<ManagedExecutorKind>,
    pub(super) managed_event_facts: Mutex<Option<Arc<dyn ManagedEventFacts>>>,
    pub(super) state: Mutex<RuntimeState>,
    pub(super) interruption_history_state: Mutex<InterruptionHistoryState>,
    pub(super) requests: tokio::sync::Mutex<RequestLedger>,
    pub(super) cleanups: tokio::sync::Mutex<HashMap<u64, CleanupRecord>>,
    pub(super) lifecycle_actions: tokio::sync::Mutex<()>,
    pub(super) adapter_actions: tokio::sync::Mutex<()>,
    pub(super) prompt_actions: tokio::sync::Mutex<()>,
    pub(super) events: broadcast::Sender<HaloWorkbenchEvent>,
    pub(super) adapter_events_started: AtomicBool,
    pub(super) shutdown_result: OnceCell<Result<(), HaloWorkbenchError>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ManagedWorkspaceConfirmation {
    pub(super) generation: u64,
    pub(super) workspace_id: String,
    pub(super) canonical_root: PathBuf,
}

pub(super) struct UnavailableTaskBaselinePort;

/// The runtime-internal default pi binding: it exposes the injected
/// `PiRpcPort` behind the unified `ManagedExecutorPort` (ADR-0078). The
/// composition root replaces this bridge with the production adapter
/// wrapper; the bridge exists so managed dispatch always crosses the
/// executor-neutral seam, including in contract fakes.
pub(super) struct PiRpcPortExecutorBridge {
    pub(super) adapter: Arc<dyn PiRpcPort>,
    /// Reads the runtime's active adapter generation at dispatch time.
    pub(super) generation: Arc<dyn Fn() -> Option<u64> + Send + Sync>,
}

impl PiRpcPortExecutorBridge {
    pub(super) fn current_generation(&self) -> halo_runtime_ports::PortResult<u64> {
        (self.generation)().ok_or_else(|| {
            halo_runtime_ports::PortError::new(
                PortErrorKind::NotAvailable,
                "The Workbench adapter generation is not active",
            )
        })
    }
}

#[async_trait::async_trait]
impl ManagedExecutorPort for PiRpcPortExecutorBridge {
    fn capability_profile(&self) -> halo_runtime_ports::ManagedExecutorCapabilityProfile {
        halo_runtime_ports::ManagedExecutorCapabilityProfile {
            adapter_identity: PI_RPC_ADAPTER_IDENTITY.to_string(),
            compatibility_profile: self
                .adapter
                .readiness()
                .map(|summary| summary.version.profile.as_str().to_string())
                .unwrap_or_else(|| "unprobed".to_string()),
            // The bridge is a pass-through for the execution face; steering
            // and queue-event adoption stay with the production adapter
            // wrapper.
            steer: false,
            queue_events: false,
            approval_channel: true,
            entry_read: true,
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
        let generation = self.current_generation()?;
        self.adapter
            .execute(PiRpcCommand::SendUserInput {
                generation,
                task_id: request.target.task_id,
                session_id: request.target.session_id,
                content: request.content,
            })
            .await
            .map(|_| ())
    }

    async fn follow_up(&self, request: ManagedExecutorPromptRequest) -> PortResult<()> {
        let generation = self.current_generation()?;
        self.adapter
            .execute(PiRpcCommand::FollowUp {
                generation,
                task_id: request.target.task_id,
                session_id: request.target.session_id,
                content: request.content,
            })
            .await
            .map(|_| ())
    }

    async fn abort(
        &self,
        target: ManagedExecutorTarget,
    ) -> PortResult<halo_runtime_ports::ManagedExecutorAbortOutcome> {
        let generation = self.current_generation()?;
        self.adapter
            .execute(PiRpcCommand::AbortSession {
                generation,
                task_id: target.task_id,
                session_id: target.session_id,
            })
            .await
            .map(|_| halo_runtime_ports::ManagedExecutorAbortOutcome::Cooperative)
    }

    async fn read_entries(
        &self,
        _target: ManagedExecutorTarget,
    ) -> PortResult<halo_runtime_ports::ManagedExecutorEntryPage> {
        // The bridge exposes only the execution face; entry reads keep their
        // dedicated adapter path in the Workbench Runtime.
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "the runtime bridge does not expose entry reads",
        ))
    }

    async fn resolve_approval(
        &self,
        _decision: halo_runtime_ports::ManagedExecutorApprovalDecision,
    ) -> PortResult<()> {
        // The bridge exposes only the execution face; approval resolution
        // keeps its dedicated adapter path in the Workbench Runtime.
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "the runtime bridge does not expose approval resolution",
        ))
    }

    fn subscribe(
        &self,
    ) -> tokio::sync::broadcast::Receiver<halo_runtime_ports::ManagedExecutorEvent> {
        // The runtime derives its facts from the adapter event stream
        // directly; the bridge never forwards unified events. The dropped
        // sender turns receivers into clean `Closed` states.
        let (_, receiver) = tokio::sync::broadcast::channel(1);
        receiver
    }
}

#[async_trait::async_trait]
impl WorkbenchTaskBaselinePort for UnavailableTaskBaselinePort {
    async fn capture(
        &self,
        _request: WorkbenchTaskBaselineRequest,
    ) -> halo_runtime_ports::PortResult<WorkbenchTaskBaseline> {
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "managed task baseline provider is unavailable",
        ))
    }
}

pub(super) struct UnavailableDeliveryEvidencePort;

#[async_trait::async_trait]
impl WorkbenchDeliveryEvidencePort for UnavailableDeliveryEvidencePort {
    async fn capture(
        &self,
        _request: WorkbenchDeliveryEvidenceRequest,
    ) -> halo_runtime_ports::PortResult<WorkbenchDeliveryEvidence> {
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "managed delivery evidence provider is unavailable",
        ))
    }

    async fn capture_fingerprint(
        &self,
        _request: WorkbenchDeliveryFingerprintRequest,
    ) -> halo_runtime_ports::PortResult<WorkbenchDeliveryFingerprint> {
        Err(halo_runtime_ports::PortError::new(
            PortErrorKind::NotAvailable,
            "managed delivery evidence provider is unavailable",
        ))
    }
}

pub(super) struct EmptyInterruptionHistoryPort;

impl HaloWorkbenchInterruptionHistoryPort for EmptyInterruptionHistoryPort {
    fn load_interrupted_sessions(&self) -> PortResult<Vec<HaloWorkbenchSessionSnapshot>> {
        Ok(Vec::new())
    }

    fn replace_interrupted_sessions(
        &self,
        _sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> PortResult<()> {
        Ok(())
    }
}

impl HaloWorkbenchRuntimeInner {
    pub(super) fn persist_interruption_history(
        &self,
        state_version: u64,
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) {
        let mut history = self
            .interruption_history_state
            .lock()
            .expect("Halo Workbench interruption history lock");
        if !history.should_persist(state_version, &sessions) {
            return;
        }
        if let Err(error) = self
            .interruption_history
            .replace_interrupted_sessions(sessions.clone())
        {
            log::warn!(
                "Halo Workbench interruption history persistence failed: operation=replace_interrupted_sessions session_count={} error={error}",
                sessions.len()
            );
            return;
        }
        history.mark_persisted(sessions);
    }

    pub(super) fn install_managed_event_fact_store(&self, port: Arc<dyn ManagedEventFactStorePort>) {
        let mut store = self
            .managed_event_facts
            .lock()
            .expect("Halo Workbench managed event facts lock");
        *store = Some(Arc::new(ManagedEventFactsPortAdapter::new(port)));
    }

    pub(super) fn append_managed_task_fact(
        &self,
        task_id: &str,
        kind: ManagedEventFactKind,
        summary: &str,
        identity: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let store = self
            .managed_event_facts
            .lock()
            .expect("Halo Workbench managed event facts lock")
            .clone();
        let Some(store) = store else {
            return Ok(());
        };
        store
            .append(ManagedEventFactInput {
                fact_id: HaloFactId::from_runtime({
                    let mut digest = Sha256::new();
                    digest.update(task_id.as_bytes());
                    digest.update([0]);
                    digest.update(identity.as_bytes());
                    digest.update([0]);
                    digest.update(format!("{kind:?}").as_bytes());
                    format!("halo-fact-{}", hex::encode(digest.finalize()))
                }),
                task_id: HaloTaskId::from_runtime(task_id.to_string()),
                recorded_at_ms: self.clock.now_unix_millis(),
                schema_version: crate::managed_event_facts::MANAGED_EVENT_FACT_SCHEMA_VERSION,
                kind,
                redacted_summary: normalize_summary(summary)
                    .map_err(|_| HaloWorkbenchError::managed_event_facts_unavailable())?,
            })
            .map(|_| ())
            .map_err(|_| HaloWorkbenchError::managed_event_facts_unavailable())
    }

    pub(super) fn append_managed_session_fact(
        &self,
        generation: u64,
        session_id: &str,
        kind: ManagedEventFactKind,
        summary: &str,
        identity: &str,
    ) -> bool {
        let task_id = self
            .state
            .lock()
            .expect("Halo Workbench state lock")
            .sessions
            .get(session_id)
            .filter(|session| session.mode == HaloWorkbenchSessionMode::Managed)
            .map(|session| session.task_id.clone());
        let Some(task_id) = task_id else {
            return true;
        };
        match self.append_managed_task_fact(&task_id, kind, summary, identity) {
            Ok(()) => true,
            Err(error) => {
                self.expose_error(error);
                let _ = generation;
                false
            }
        }
    }

    pub(super) fn expose_error(&self, error: HaloWorkbenchError) {
        self.state.lock().expect("Halo Workbench state lock").error = Some(error);
    }

    /// Appends the committed reply summary fact at settlement. The settled
    /// turn's accumulated assistant text seeds the fact identity so each
    /// committed reply is exactly one fact; the summary itself stays a
    /// Halo-owned constant.
    pub(super) fn append_settled_reply_fact(&self, generation: u64, session_id: &str) -> bool {
        let reply = {
            let state = self.state.lock().expect("Halo Workbench state lock");
            state
                .sessions
                .get(session_id)
                .filter(|session| session.mode == HaloWorkbenchSessionMode::Managed)
                .and_then(|session| {
                    session
                        .messages
                        .iter()
                        .rev()
                        .find(|message| message.role == HaloWorkbenchMessageRole::Assistant)
                })
                .map(|message| message.content.clone())
        };
        let Some(reply) = reply else {
            return true;
        };
        if reply.is_empty() {
            return true;
        }
        self.append_managed_session_fact(
            generation,
            session_id,
            ManagedEventFactKind::AgentReplySummary,
            "Managed agent reply summary received",
            &reply,
        )
    }

    /// Whether a managed session would currently accept the interrupted
    /// terminal transition. Read-only pre-check so the interrupted fact only
    /// lands when the cancellation actually does.
    pub(super) fn managed_session_accepts_interruption(&self, generation: u64, session_id: &str) -> bool {
        let state = self.state.lock().expect("Halo Workbench state lock");
        state.generation == generation
            && state
                .sessions
                .get(session_id)
                .is_some_and(|session| {
                    session.mode == HaloWorkbenchSessionMode::Managed
                        && session.accepts_terminal_adapter_event()
                        && valid_session_transition(
                            session.phase,
                            HaloWorkbenchSessionPhase::Interrupted,
                        )
                })
    }

    pub(super) fn snapshot(&self) -> HaloWorkbenchSnapshot {
        let state = self.state.lock().expect("Halo Workbench state lock");
        HaloWorkbenchSnapshot {
            schema_version: HALO_WORKBENCH_SCHEMA_VERSION,
            phase: state.phase,
            adapter: HaloWorkbenchAdapterSnapshot {
                identity: PI_RPC_ADAPTER_IDENTITY.to_string(),
                available: state.adapter_available,
                readiness: state.adapter_readiness.clone(),
            },
            workspace: state.workspace.clone(),
            sessions: state.sessions.values().cloned().collect(),
            pending_operations: state.pending_operations.values().cloned().collect(),
            last_sequence: state.sequence,
            state_version: state.state_version,
            error: state.error.clone(),
        }
    }

    pub(super) fn receipt(&self, request_id: &str, session_id: Option<String>) -> HaloWorkbenchIntentReceipt {
        HaloWorkbenchIntentReceipt {
            request_id: request_id.to_string(),
            state_version: self
                .state
                .lock()
                .expect("Halo Workbench state lock")
                .state_version,
            session_id,
        }
    }

    pub(super) fn publish_transition(
        &self,
        correlation_id: Option<&str>,
        kind: HaloWorkbenchEventKind,
        summary: &'static str,
        session_id: Option<String>,
        operation_id: Option<String>,
        mutate: impl FnOnce(&mut RuntimeState) -> bool,
    ) -> bool {
        let (event, interrupted_sessions) = {
            let mut state = self.state.lock().expect("Halo Workbench state lock");
            if !mutate(&mut state) {
                return false;
            }
            state.sequence = state
                .sequence
                .checked_add(1)
                .expect("Halo Workbench event sequence exhausted");
            state.state_version = state
                .state_version
                .checked_add(1)
                .expect("Halo Workbench state version exhausted");
            let event = HaloWorkbenchEvent {
                sequence: state.sequence,
                state_version: state.state_version,
                correlation_id: correlation_id.map(str::to_string),
                kind,
                summary: summary.to_string(),
                session_id,
                operation_id,
                occurred_at_ms: self.clock.now_unix_millis(),
            };
            let interrupted_sessions = interruption_history_snapshots(&state);
            (event, interrupted_sessions)
        };
        self.persist_interruption_history(event.state_version, interrupted_sessions);
        let _ = self.events.send(event);
        true
    }

    pub(super) fn apply_adapter_event(&self, event: PiRpcEvent) {
        let generation = event.generation();
        match event {
            PiRpcEvent::Ready { .. } => {
                self.publish_transition(
                    None,
                    HaloWorkbenchEventKind::RuntimeStateChanged,
                    "Workbench Runtime is ready",
                    None,
                    None,
                    |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Starting
                            || state.terminated
                        {
                            return false;
                        }
                        state.phase = HaloWorkbenchPhase::Ready;
                        state.adapter_available = true;
                        state.error = None;
                        true
                    },
                );
            }
            PiRpcEvent::Failed { reason, .. } => {
                let error = adapter_failure(reason);
                self.fail_generation(generation, None, error);
            }
            PiRpcEvent::SessionCreated { session_id, .. }
            | PiRpcEvent::SessionIdle { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Idle,
                    "Workbench session is idle",
                );
            }
            // Live queue bookkeeping stays in the adapter; Halo owns
            // queueing and derives settlement from `agent_settled`.
            PiRpcEvent::QueueUpdated { .. } => {}
            PiRpcEvent::AgentSettled { session_id, .. } => {
                if !self.append_settled_reply_fact(generation, &session_id) {
                    return;
                }
                self.capture_settled_fingerprint(generation, &session_id);
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::WaitingDeveloper,
                    "Workbench session is waiting for developer",
                );
            }
            PiRpcEvent::SessionStopped {
                session_id,
                cancellation_mode,
                ..
            } => {
                if self.managed_session_accepts_interruption(generation, &session_id) {
                    // Cancellation lands as the delivered prefix plus the
                    // interrupted marker; no completion fact follows
                    // (ADR-0080).
                    self.append_managed_session_fact(
                        generation,
                        &session_id,
                        ManagedEventFactKind::TaskInterrupted,
                        "Managed task interrupted; delivered prefix preserved",
                        &format!("{session_id}\u{1}interrupted"),
                    );
                }
                self.set_session_interrupted(generation, &session_id, cancellation_mode.into());
            }
            PiRpcEvent::SessionRunning { session_id, .. } => {
                self.set_session_phase(
                    generation,
                    &session_id,
                    HaloWorkbenchSessionPhase::Running,
                    "Workbench session is running",
                );
            }
            PiRpcEvent::SessionEnded { session_id, .. } => {
                self.set_adapter_session_ended(generation, &session_id);
            }
            PiRpcEvent::SessionFailed {
                session_id, reason, ..
            } => {
                let phase = self
                    .state
                    .lock()
                    .expect("Halo Workbench state lock")
                    .sessions
                    .get(&session_id)
                    .filter(|session| session.mode == HaloWorkbenchSessionMode::Managed)
                    .map(|_| HaloWorkbenchSessionPhase::Interrupted)
                    .unwrap_or(HaloWorkbenchSessionPhase::Failed);
                if phase == HaloWorkbenchSessionPhase::Interrupted {
                    // A failed executor attempt is recorded as its own fact,
                    // never merged back into a continuous history (ADR-0080).
                    self.append_managed_session_fact(
                        generation,
                        &session_id,
                        ManagedEventFactKind::AttemptFailed,
                        &format!(
                            "Managed attempt failed: {}",
                            managed_executor_failure_label(reason)
                        ),
                        &format!(
                            "{session_id}\u{1}attempt\u{1}{}",
                            managed_executor_failure_label(reason)
                        ),
                    );
                }
                self.set_session_failure(generation, &session_id, adapter_failure(reason), phase);
            }
            PiRpcEvent::MessageUpdated {
                session_id, text, ..
            } => {
                // Token-level streaming frames are live activity only
                // (ADR-0080): they update the activity session record and
                // never append facts. The committed reply fact lands at
                // settlement.
                self.append_assistant_message(generation, &session_id, text);
            }
            PiRpcEvent::ToolExecutionStarted {
                session_id,
                redacted_tool_call_id,
                tool_name,
                ..
            } => {
                if !self.append_managed_session_fact(
                    generation,
                    &session_id,
                    ManagedEventFactKind::ToolActivity,
                    "Managed tool activity started",
                    &format!("{}:started", redacted_tool_call_id),
                ) {
                    return;
                }
                self.update_tool_activity(
                    generation,
                    &session_id,
                    redacted_tool_call_id,
                    tool_name,
                    HaloWorkbenchActivityStatus::Started,
                    false,
                );
            }
            PiRpcEvent::ToolExecutionUpdated {
                session_id,
                redacted_tool_call_id,
                tool_name,
                ..
            } => {
                if !self.append_managed_session_fact(
                    generation,
                    &session_id,
                    ManagedEventFactKind::ToolActivity,
                    "Managed tool activity updated",
                    &format!("{}:updated", redacted_tool_call_id),
                ) {
                    return;
                }
                self.update_tool_activity(
                    generation,
                    &session_id,
                    redacted_tool_call_id,
                    tool_name,
                    HaloWorkbenchActivityStatus::Updated,
                    false,
                );
            }
            PiRpcEvent::ToolExecutionEnded {
                session_id,
                redacted_tool_call_id,
                tool_name,
                is_error,
                ..
            } => {
                if !self.append_managed_session_fact(
                    generation,
                    &session_id,
                    ManagedEventFactKind::ToolActivity,
                    "Managed tool activity ended",
                    &format!("{}:ended", redacted_tool_call_id),
                ) {
                    return;
                }
                self.update_tool_activity(
                    generation,
                    &session_id,
                    redacted_tool_call_id,
                    tool_name,
                    if is_error {
                        HaloWorkbenchActivityStatus::Failed
                    } else {
                        HaloWorkbenchActivityStatus::Completed
                    },
                    is_error,
                );
            }
            PiRpcEvent::OperationRequested {
                session_id,
                operation_id,
                kind,
                summary,
                ..
            } => {
                if !self.append_managed_session_fact(
                    generation,
                    &session_id,
                    ManagedEventFactKind::AgentOperationRequest,
                    "Managed operation request received",
                    &operation_id,
                ) {
                    return;
                }
                let event_session_id = session_id.clone();
                let event_operation_id = operation_id.clone();
                self.publish_transition(
                    None,
                    HaloWorkbenchEventKind::OperationRequested,
                    "A Workbench operation requires a decision",
                    Some(event_session_id),
                    Some(event_operation_id),
                    move |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Ready
                            || state
                                .sessions
                                .get(&session_id)
                                .is_none_or(|session| session.phase.rejects_adapter_events())
                            || state.pending_operations.contains_key(&operation_id)
                        {
                            return false;
                        }
                        let Some(session) = state.sessions.get(&session_id) else {
                            return false;
                        };
                        state.pending_operations.insert(
                            operation_id.clone(),
                            HaloWorkbenchPendingOperationSnapshot {
                                operation_id,
                                task_id: session.task_id.clone(),
                                session_id,
                                kind: kind.into(),
                                phase: HaloWorkbenchPendingOperationPhase::AwaitingDecision,
                                tool_name: summary.tool_name,
                                arguments: summary.arguments,
                                risk_level: summary.risk_level.into(),
                            },
                        );
                        true
                    },
                );
            }
            PiRpcEvent::OperationResolved {
                session_id,
                operation_id,
                ..
            } => {
                if !self.append_managed_session_fact(
                    generation,
                    &session_id,
                    ManagedEventFactKind::AgentOperationDecision,
                    "Managed operation decision received",
                    &operation_id,
                ) {
                    return;
                }
                let event_session_id = session_id.clone();
                let event_operation_id = operation_id.clone();
                self.publish_transition(
                    None,
                    HaloWorkbenchEventKind::OperationResolved,
                    "Workbench operation was resolved",
                    Some(event_session_id),
                    Some(event_operation_id),
                    move |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Ready
                        {
                            return false;
                        }
                        let belongs_to_session = state
                            .pending_operations
                            .get(&operation_id)
                            .is_some_and(|operation| operation.session_id == session_id);
                        belongs_to_session
                            && state.pending_operations.remove(&operation_id).is_some()
                    },
                );
            }
        }
    }

    pub(super) fn append_assistant_message(&self, generation: u64, session_id: &str, text: String) {
        let text = redact_halo_text(&text, MAX_PUBLIC_MESSAGE_BYTES);
        if text.is_empty() {
            return;
        }
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionMessageUpdated,
            "Workbench assistant message was updated",
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Running {
                    return false;
                }
                append_message(
                    &mut session.messages,
                    HaloWorkbenchMessageRole::Assistant,
                    text,
                );
                true
            },
        );
    }

    pub(super) fn update_tool_activity(
        &self,
        generation: u64,
        session_id: &str,
        activity_id: String,
        label: String,
        status: HaloWorkbenchActivityStatus,
        is_error: bool,
    ) {
        let Some(activity_id) = opaque_public_activity_id(&activity_id) else {
            return;
        };
        let label = redact_halo_text(&label, MAX_PUBLIC_LABEL_BYTES);
        let Some(label) = bounded_public_label(&label, MAX_PUBLIC_LABEL_BYTES) else {
            return;
        };
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionActivityUpdated,
            "Workbench tool activity was updated",
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Running {
                    return false;
                }
                if let Some(activity) = session
                    .activities
                    .iter_mut()
                    .find(|activity| activity.activity_id == activity_id)
                {
                    activity.label = label;
                    activity.status = status;
                    activity.is_error = is_error;
                    return true;
                }
                if session.activities.len() >= MAX_SESSION_ACTIVITIES {
                    session.activities.remove(0);
                }
                session.activities.push(HaloWorkbenchActivitySnapshot {
                    activity_id,
                    kind: HaloWorkbenchActivityKind::Tool,
                    label,
                    status,
                    is_error,
                });
                true
            },
        );
    }

    pub(super) fn set_session_phase(
        &self,
        generation: u64,
        session_id: &str,
        phase: HaloWorkbenchSessionPhase,
        summary: &'static str,
    ) {
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            summary,
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if session.phase == phase
                    || session.phase.is_terminal()
                    || !valid_session_transition(session.phase, phase)
                {
                    return false;
                }
                session.phase = phase;
                session.error = None;
                if phase != HaloWorkbenchSessionPhase::Interrupted {
                    session.cancellation_mode = None;
                }
                if phase.is_terminal() {
                    state
                        .pending_operations
                        .retain(|_, operation| operation.session_id != owned_session_id);
                }
                true
            },
        );
    }

    pub(super) fn set_session_interrupted(
        &self,
        generation: u64,
        session_id: &str,
        cancellation_mode: HaloWorkbenchCancellationMode,
    ) {
        let owned_session_id = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session was interrupted",
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if !session.accepts_terminal_adapter_event()
                    || !valid_session_transition(
                        session.phase,
                        HaloWorkbenchSessionPhase::Interrupted,
                    )
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Interrupted;
                session.error = None;
                session.cancellation_mode = Some(cancellation_mode);
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != owned_session_id);
                true
            },
        );
    }

    pub(super) fn set_session_failure(
        &self,
        generation: u64,
        session_id: &str,
        error: HaloWorkbenchError,
        phase: HaloWorkbenchSessionPhase,
    ) {
        let owned_session_id = session_id.to_string();
        let summary = match phase {
            HaloWorkbenchSessionPhase::Interrupted => "Workbench session was interrupted",
            _ => "Workbench session failed",
        };
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            summary,
            Some(owned_session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&owned_session_id) else {
                    return false;
                };
                if !session.accepts_terminal_adapter_event()
                    || !valid_session_transition(session.phase, phase)
                {
                    return false;
                }
                session.phase = phase;
                session.error = Some(error);
                session.cancellation_mode = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != owned_session_id);
                true
            },
        );
    }

    pub(super) fn fail_generation(
        &self,
        generation: u64,
        correlation_id: Option<&str>,
        error: HaloWorkbenchError,
    ) -> bool {
        self.publish_transition(
            correlation_id,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime failed",
            None,
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                interrupt_managed_sessions(state, &error);
                state.phase = HaloWorkbenchPhase::Failed;
                state.adapter_available = false;
                state.error = Some(error);
                true
            },
        )
    }

    pub(super) async fn fail_adapter_event_gap(self: &Arc<Self>) {
        self.fail_active_runtime(
            HaloWorkbenchError::new(
                "adapter_event_gap",
                "The Workbench execution event stream has a gap",
                "restart_runtime",
            ),
            "Workbench Runtime event stream failed",
        )
        .await;
    }

    pub(super) async fn fail_adapter_event_stream_closed(self: &Arc<Self>) {
        self.fail_active_runtime(
            HaloWorkbenchError::new(
                "adapter_event_stream_closed",
                "The Workbench execution event stream closed unexpectedly",
                "restart_runtime",
            ),
            "Workbench Runtime event stream failed",
        )
        .await;
    }

    pub(super) async fn fail_active_runtime(
        self: &Arc<Self>,
        error: HaloWorkbenchError,
        summary: &'static str,
    ) {
        let transitioned = self.publish_transition(
            None,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            summary,
            None,
            None,
            move |state| {
                if state.terminated
                    || !matches!(
                        state.phase,
                        HaloWorkbenchPhase::Probing
                            | HaloWorkbenchPhase::Starting
                            | HaloWorkbenchPhase::Ready
                    )
                {
                    return false;
                }
                interrupt_managed_sessions(state, &error);
                state.phase = HaloWorkbenchPhase::Failed;
                state.adapter_available = false;
                state.error = Some(error);
                true
            },
        );
        let cleanup_generation = transitioned.then(|| {
            self.state
                .lock()
                .expect("Halo Workbench state lock")
                .adapter_generation
        });
        if let Some(generation) = cleanup_generation.flatten() {
            if let Err(error) = self.execute_cleanup_once(generation).await {
                log::warn!(
                    "Halo Workbench Runtime cleanup failed: operation=shutdown generation={generation} error={error}"
                );
            }
        }
    }

    pub(super) fn capture_settled_fingerprint(&self, generation: u64, session_id: &str) {
        let session_id_owned = session_id.to_string();
        let request = {
            let state = self.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return;
            }
            let Some(session) = state.sessions.get(&session_id_owned) else {
                return;
            };
            if session.mode != HaloWorkbenchSessionMode::Managed {
                return;
            }
            let Some(workspace) = state.workspace.as_ref() else {
                return;
            };
            WorkbenchDeliveryFingerprintRequest {
                workspace_id: workspace.workspace_id.clone(),
                canonical_root: workspace.root_path.clone(),
            }
        };
        let (sender, receiver) = watch::channel(None);
        {
            let mut state = self.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation {
                return;
            }
            state
                .settled_fingerprints
                .insert(session_id_owned, receiver);
        }
        let port = self.delivery_evidence.clone();
        tokio::spawn(async move {
            let fingerprint = port.capture_fingerprint(request).await.ok();
            sender.send_replace(fingerprint);
        });
    }

    pub(super) fn set_adapter_session_ended(&self, generation: u64, session_id: &str) {
        let session_id_owned = session_id.to_string();
        self.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session ended",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                // A finished managed task remains in read-only delivery review
                // until the developer accepts or rejects the result.
                if session.phase == HaloWorkbenchSessionPhase::Reviewing
                    || session.phase == HaloWorkbenchSessionPhase::Interrupted
                    || session.phase.is_terminal()
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Ended;
                session.error = None;
                session.cancellation_mode = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != session_id_owned);
                true
            },
        );
    }

    pub(super) async fn execute_cleanup_once(self: &Arc<Self>, generation: u64) -> CleanupResult {
        let mut result = {
            let mut cleanups = self.cleanups.lock().await;
            match cleanups.get(&generation) {
                Some(CleanupRecord::Complete(result)) => return result.clone(),
                Some(CleanupRecord::InFlight { result }) => result.subscribe(),
                None => {
                    let (sender, receiver) = watch::channel(None);
                    cleanups.insert(
                        generation,
                        CleanupRecord::InFlight {
                            result: sender.clone(),
                        },
                    );
                    self.state
                        .lock()
                        .expect("Halo Workbench state lock")
                        .cleanup_started
                        .insert(generation);
                    let inner = Arc::clone(self);
                    tokio::spawn(async move {
                        let cleanup_result = {
                            let _action = inner.adapter_actions.lock().await;
                            match inner
                                .adapter
                                .execute(PiRpcCommand::Shutdown { generation })
                                .await
                            {
                                Ok(PiRpcReply::Accepted)
                                | Ok(PiRpcReply::Available { .. })
                                | Ok(PiRpcReply::Ready { .. })
                                | Ok(PiRpcReply::Entries { .. }) => Ok(()),
                                Ok(PiRpcReply::Unavailable { .. }) => Err(HaloWorkbenchError::new(
                                    "cleanup_failed",
                                    "Workbench Runtime cleanup did not complete",
                                    "restart_application",
                                )),
                                Err(error) => Err(port_failure(error.kind)),
                            }
                        };
                        sender.send_replace(Some(cleanup_result.clone()));
                        let mut cleanups = inner.cleanups.lock().await;
                        cleanups.insert(generation, CleanupRecord::Complete(cleanup_result));
                        while cleanups
                            .values()
                            .filter(|record| matches!(record, CleanupRecord::Complete(_)))
                            .count()
                            > MAX_COMPLETED_CLEANUP_RECORDS
                        {
                            let Some(generation) =
                                cleanups.iter().find_map(|(generation, record)| {
                                    matches!(record, CleanupRecord::Complete(_))
                                        .then_some(*generation)
                                })
                            else {
                                break;
                            };
                            cleanups.remove(&generation);
                        }
                    });
                    receiver
                }
            }
        };

        loop {
            if let Some(cleanup_result) = result.borrow().clone() {
                return cleanup_result;
            }
            if result.changed().await.is_err() {
                return Err(HaloWorkbenchError::new(
                    "cleanup_failed",
                    "Workbench Runtime cleanup did not complete",
                    "restart_application",
                ));
            }
        }
    }
}

impl Drop for HaloWorkbenchRuntimeInner {
    fn drop(&mut self) {
        let state = match self.state.get_mut() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(generation) = state.adapter_generation else {
            return;
        };
        if !state.cleanup_started.insert(generation) {
            return;
        }
        let adapter = self.adapter.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = adapter.execute(PiRpcCommand::Shutdown { generation }).await;
            });
        }
    }
}

#[derive(Clone)]
pub struct HaloWorkbenchRuntime {
    pub(super) inner: Arc<HaloWorkbenchRuntimeInner>,
}

pub(super) fn interrupt_managed_sessions(state: &mut RuntimeState, error: &HaloWorkbenchError) {
    let mut interrupted_session_ids = HashSet::new();
    for session in state.sessions.values_mut() {
        if session.mode != HaloWorkbenchSessionMode::Managed
            || session.phase.is_terminal()
            || !valid_session_transition(session.phase, HaloWorkbenchSessionPhase::Interrupted)
        {
            continue;
        }
        session.phase = HaloWorkbenchSessionPhase::Interrupted;
        session.cancellation_mode = None;
        session.error = Some(error.clone());
        interrupted_session_ids.insert(session.session_id.clone());
    }
    state
        .pending_operations
        .retain(|_, operation| !interrupted_session_ids.contains(&operation.session_id));
}

pub(super) fn retain_managed_interruption_facts(state: &mut RuntimeState) {
    state.sessions.retain(|_, session| {
        session.mode == HaloWorkbenchSessionMode::Managed
            && session.phase == HaloWorkbenchSessionPhase::Interrupted
    });
}

pub(super) fn interruption_history_snapshots(state: &RuntimeState) -> Vec<HaloWorkbenchSessionSnapshot> {
    state
        .sessions
        .values()
        .filter_map(|session| {
            if session.mode != HaloWorkbenchSessionMode::Managed || session.phase.is_terminal() {
                return None;
            }
            // The durable record is deliberately a post-crash projection, not
            // a resumable transport checkpoint. It can only return as an
            // explicit Interrupted disposition after process loss. Frozen
            // delivery evidence and the task baseline remain reviewable, but
            // active session content never crosses this persistence boundary.
            let mut checkpoint = session.clone();
            if checkpoint.phase.needs_interruption_checkpoint() {
                checkpoint.phase = HaloWorkbenchSessionPhase::Interrupted;
                checkpoint.cancellation_mode = None;
                checkpoint.error = Some(HaloWorkbenchError::application_interrupted());
            }
            checkpoint.messages.clear();
            checkpoint.activities.clear();
            Some(checkpoint)
        })
        .collect()
}

pub(super) fn valid_session_transition(
    from: HaloWorkbenchSessionPhase,
    to: HaloWorkbenchSessionPhase,
) -> bool {
    use HaloWorkbenchSessionPhase::*;

    matches!(
        (from, to),
        (
            Creating,
            Idle | Running | Interrupted | Stopping | Ended | Failed
        ) | (Idle, Running | Interrupted | Stopping | Ended | Failed)
            | (
                Running,
                WaitingDeveloper | Interrupted | Stopping | Ended | Failed
            )
            | (
                WaitingDeveloper,
                Reviewing | Interrupted | Stopping | Ended | Failed
            )
            | (Reviewing, Interrupted | Failed)
            | (Interrupted, Reviewing | Ended | Failed)
            | (Stopping, Interrupted | Ended | Failed)
    )
}


#[cfg(test)]
mod interruption_history_tests {
    use super::*;

    fn interrupted_session(session_id: &str) -> HaloWorkbenchSessionSnapshot {
        HaloWorkbenchSessionSnapshot {
            workspace_id: "workspace-1".to_string(),
            task_id: "task-1".to_string(),
            session_id: session_id.to_string(),
            mode: HaloWorkbenchSessionMode::Managed,
            phase: HaloWorkbenchSessionPhase::Interrupted,
            executor: ManagedExecutorKind::PiRpc,
            cancellation_mode: None,
            baseline: None,
            messages: Vec::new(),
            activities: Vec::new(),
            error: None,
            delivery_review: None,
        }
    }

    #[test]
    fn an_older_checkpoint_cannot_replace_newer_interruption_history() {
        let newer = vec![interrupted_session("newer-session")];
        let older = vec![interrupted_session("older-session")];
        let mut history = InterruptionHistoryState::new(Vec::new());

        assert!(history.should_persist(2, &newer));
        history.mark_persisted(newer.clone());

        assert!(!history.should_persist(1, &older));
        assert_eq!(history.persisted_sessions, newer);
    }
}

pub(super) fn summarize_delivery_messages(messages: &[HaloWorkbenchMessageSnapshot]) -> String {
    let joined = messages
        .iter()
        .filter(|message| message.role == HaloWorkbenchMessageRole::Assistant)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    redact_halo_text(&joined, MAX_DELIVERY_SUMMARY_BYTES)
}

pub(super) fn summarize_delivery_activities(activities: &[HaloWorkbenchActivitySnapshot]) -> String {
    let joined = activities
        .iter()
        .map(|activity| activity.label.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    redact_halo_text(&joined, MAX_DELIVERY_SUMMARY_BYTES)
}

pub(super) fn append_message(
    messages: &mut Vec<HaloWorkbenchMessageSnapshot>,
    role: HaloWorkbenchMessageRole,
    content: String,
) {
    if content.is_empty() {
        return;
    }
    if role == HaloWorkbenchMessageRole::Assistant
        && messages
            .last()
            .is_some_and(|message| message.role == HaloWorkbenchMessageRole::Assistant)
    {
        if let Some(message) = messages.last_mut() {
            let remaining = MAX_PUBLIC_MESSAGE_BYTES.saturating_sub(message.content.len());
            if remaining > 0 {
                message
                    .content
                    .push_str(&truncate_utf8(&content, remaining));
            }
        }
        return;
    }
    if messages.len() >= MAX_SESSION_MESSAGES {
        messages.remove(0);
    }
    messages.push(HaloWorkbenchMessageSnapshot {
        role,
        content: truncate_utf8(&content, MAX_PUBLIC_MESSAGE_BYTES),
    });
}

pub(super) fn bounded_public_label(value: &str, max_bytes: usize) -> Option<String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(truncate_utf8(value, max_bytes))
}

/// Tool-call identifiers stay adapter-private even if a malformed adapter
/// event reaches the Runtime. The public snapshot uses an opaque local key.
pub(super) fn opaque_public_activity_id(value: &str) -> Option<String> {
    bounded_public_label(value, MAX_PUBLIC_LABEL_BYTES)?;
    let digest = Sha256::digest(value.as_bytes());
    Some(format!("activity-{}", hex::encode(&digest[..8])))
}
pub(super) fn port_failure(kind: PortErrorKind) -> HaloWorkbenchError {
    match kind {
        PortErrorKind::Cancelled => HaloWorkbenchError::new(
            "adapter_cancelled",
            "The Workbench execution request was cancelled",
            "retry",
        ),
        PortErrorKind::Timeout => HaloWorkbenchError::new(
            "adapter_timeout",
            "The Workbench execution adapter timed out",
            "retry",
        ),
        PortErrorKind::PermissionDenied => HaloWorkbenchError::new(
            "adapter_access_denied",
            "The Workbench execution adapter was denied access",
            "review_system_permissions",
        ),
        _ => HaloWorkbenchError::new(
            "adapter_unavailable",
            "The Workbench execution adapter is unavailable",
            "retry",
        ),
    }
}

/// Executor-neutral failure label used by attempt facts. The wording mirrors
/// the unified `ManagedExecutorFailureKind` vocabulary so facts stay
/// executor-neutral even though this seam still speaks Pi events.
pub(super) fn managed_executor_failure_label(reason: PiRpcFailureKind) -> &'static str {
    match reason {
        PiRpcFailureKind::NotInstalled => "not_installed",
        PiRpcFailureKind::UnsupportedVersion => "unsupported_version",
        PiRpcFailureKind::CapabilityMismatch => "capability_mismatch",
        PiRpcFailureKind::Authentication => "authentication",
        PiRpcFailureKind::Transport => "transport",
        PiRpcFailureKind::Protocol => "protocol",
        PiRpcFailureKind::Internal => "internal",
    }
}

pub(super) fn adapter_failure(reason: PiRpcFailureKind) -> HaloWorkbenchError {
    match reason {
        PiRpcFailureKind::NotInstalled => HaloWorkbenchError::new(
            "pi_not_installed",
            "Pi is not installed or cannot be resolved on PATH",
            "install_pi",
        ),
        PiRpcFailureKind::UnsupportedVersion => HaloWorkbenchError::new(
            "pi_version_unsupported",
            "The installed Pi version is not supported",
            "upgrade_pi",
        ),
        PiRpcFailureKind::CapabilityMismatch => HaloWorkbenchError::new(
            "pi_capability_mismatch",
            "The installed Pi RPC process lacks required capabilities",
            "upgrade_pi",
        ),
        PiRpcFailureKind::Authentication => HaloWorkbenchError::new(
            "pi_authentication_failed",
            "Pi provider authentication is unavailable",
            "configure_provider",
        ),
        PiRpcFailureKind::Transport => HaloWorkbenchError::new(
            "pi_transport_unavailable",
            "The Pi RPC child process is unavailable",
            "restart_runtime",
        ),
        PiRpcFailureKind::Protocol => HaloWorkbenchError::new(
            "pi_protocol_error",
            "The Pi RPC protocol is incompatible or malformed",
            "upgrade_pi",
        ),
        PiRpcFailureKind::Internal => HaloWorkbenchError::new(
            "pi_internal_error",
            "The Pi RPC adapter reported an internal failure",
            "restart_runtime",
        ),
    }
}
