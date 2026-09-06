//! Halo Workbench Runtime public dispatch: intent handling, session and
//! delivery operations, adapter snapshot projection.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use crate::managed_event_facts::{
    HaloTaskId, InMemoryManagedEventFacts, ManagedEventFactKind, ManagedEventFacts, ManagedEventFactsPortAdapter,
};

use halo_runtime_ports::{
    ClockPort, ManagedEventFactStorePort, ManagedExecutorKind, ManagedExecutorPort,
    ManagedExecutorPromptRequest, ManagedExecutorTarget, PiProviderReadinessPort,
    PiRpcAvailabilitySummary, PiRpcCapability, PiRpcCommand, PiRpcFailureKind, PiRpcPort, PiRpcReply, PiRpcVersionEvidenceSource, PiRpcWorkspace, WorkbenchDeliveryEvidence,
    WorkbenchDeliveryEvidencePort, WorkbenchDeliveryEvidenceRequest, WorkbenchDeliveryFingerprint, WorkbenchTaskBaseline, WorkbenchTaskBaselinePort,
    WorkbenchTaskBaselineRequest, WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest,
    WorkbenchWorkspaceTrustRequest,
};

use tokio::sync::{broadcast, watch, OnceCell};
use uuid::Uuid;
use super::state::*;
use super::vocabulary::*;
use super::redaction::*;


impl HaloWorkbenchRuntime {
    pub fn new(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self::new_with_task_baseline(
            adapter,
            workspace_facts,
            provider_readiness,
            Arc::new(UnavailableTaskBaselinePort),
            clock,
        )
    }

    /// Constructs the runtime with the read-only Git baseline provider used
    /// by managed task creation. The compatibility `new` constructor remains
    /// available for standard-only callers and contract fakes.
    pub fn new_with_task_baseline(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self::new_with_delivery_evidence(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            Arc::new(UnavailableDeliveryEvidencePort),
            clock,
        )
    }

    /// Constructs the runtime with both the Git baseline provider and the
    /// read-only delivery evidence provider used by managed tasks.
    pub fn new_with_delivery_evidence(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self::try_new_with_delivery_evidence_and_interruption_history(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            delivery_evidence,
            Arc::new(EmptyInterruptionHistoryPort),
            clock,
        )
        .expect("the empty interruption history port is infallible")
    }

    /// Constructs the runtime with an injected durable managed-facts store.
    pub fn new_with_delivery_evidence_and_fact_store(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        fact_store: Arc<dyn ManagedEventFactStorePort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        let runtime = Self::new_with_delivery_evidence(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            delivery_evidence,
            clock,
        );
        runtime.inner.install_managed_event_fact_store(fact_store);
        runtime
    }

    /// Restores the safe Halo snapshot while attaching the durable facts store.
    /// Facts are read for schema/record validation only; they are never replayed
    /// into Pi or treated as executable operations during recovery.
    pub fn try_new_with_delivery_evidence_and_fact_store_and_interruption_history(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        fact_store: Arc<dyn ManagedEventFactStorePort>,
        interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, HaloWorkbenchError> {
        let restored_sessions = interruption_history
            .load_interrupted_sessions()
            .map_err(|_| HaloWorkbenchError::interruption_history_unavailable())?;
        let fact_adapter = ManagedEventFactsPortAdapter::new(fact_store.clone());
        let mut facts_by_task = BTreeMap::new();
        for session in &restored_sessions {
            let facts = fact_adapter
                .read_task(&HaloTaskId::from_runtime(session.task_id.clone()))
                .map_err(|_| HaloWorkbenchError::managed_event_facts_unavailable())?;
            facts_by_task.insert(session.task_id.clone(), facts);
        }
        let runtime = Self::try_new_with_delivery_evidence_and_interruption_history(
            adapter,
            workspace_facts,
            provider_readiness,
            task_baseline,
            delivery_evidence,
            interruption_history,
            clock,
        )?;
        let state = RuntimeState::from_fact_history(restored_sessions, facts_by_task)?;
        *runtime
            .inner
            .state
            .lock()
            .expect("Halo Workbench state lock") = state;
        runtime.inner.install_managed_event_fact_store(fact_store);
        Ok(runtime)
    }

    /// Constructs the runtime with the durable, redacted interruption facts
    /// that are safe to surface after an application restart. This boundary
    /// deliberately excludes native Pi session state and pending operations.
    pub fn try_new_with_delivery_evidence_and_interruption_history(
        adapter: Arc<dyn PiRpcPort>,
        workspace_facts: Arc<dyn WorkbenchWorkspaceFactsPort>,
        provider_readiness: Arc<dyn PiProviderReadinessPort>,
        task_baseline: Arc<dyn WorkbenchTaskBaselinePort>,
        delivery_evidence: Arc<dyn WorkbenchDeliveryEvidencePort>,
        interruption_history: Arc<dyn HaloWorkbenchInterruptionHistoryPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Result<Self, HaloWorkbenchError> {
        let restored_interruption_history = interruption_history
            .load_interrupted_sessions()
            .map_err(|_| HaloWorkbenchError::interruption_history_unavailable())?;
        let state = RuntimeState::from_interruption_history(restored_interruption_history.clone())?;
        let (events, _) = broadcast::channel(256);
        let inner = Arc::new(HaloWorkbenchRuntimeInner {
                adapter,
                workspace_facts,
                task_baseline,
                delivery_evidence,
                interruption_history,
                provider_readiness,
                clock,
                managed_executors: Mutex::new(HashMap::new()),
                workspace_default_executor: Mutex::new(ManagedExecutorKind::PiRpc),
                managed_event_facts: Mutex::new(Some(Arc::new(
                    InMemoryManagedEventFacts::default(),
                ))),
                state: Mutex::new(state),
                interruption_history_state: Mutex::new(InterruptionHistoryState::new(
                    restored_interruption_history,
                )),
                requests: tokio::sync::Mutex::new(RequestLedger::default()),
                cleanups: tokio::sync::Mutex::new(HashMap::new()),
                lifecycle_actions: tokio::sync::Mutex::new(()),
                adapter_actions: tokio::sync::Mutex::new(()),
                prompt_actions: tokio::sync::Mutex::new(()),
                events,
                adapter_events_started: AtomicBool::new(false),
                executor_fact_pumps_started: Mutex::new(std::collections::HashSet::new()),
                shutdown_result: OnceCell::new(),
            });
        // M3 executor binding default: the injected Pi RPC port is exposed
        // behind the unified ManagedExecutorPort through the runtime bridge.
        // The composition root replaces it with the production adapter
        // wrapper via `install_managed_executor`.
        let inner_for_bridge = Arc::downgrade(&inner);
        inner.managed_executors.lock().expect("executor registry").insert(
            ManagedExecutorKind::PiRpc,
            Arc::new(PiRpcPortExecutorBridge {
                adapter: inner.adapter.clone(),
                generation: Arc::new(move || {
                    let inner = inner_for_bridge.upgrade()?;
                    let state = inner.state.lock().expect("Halo Workbench state lock");
                    if state.terminated {
                        None
                    } else {
                        state.adapter_generation
                    }
                }),
            }),
        );
        Ok(Self { inner })
    }

    pub fn snapshot(&self) -> HaloWorkbenchSnapshot {
        self.inner.snapshot()
    }

    /// Installs one production managed executor behind the unified port
    /// (ADR-0078 M3). Called by the composition root only; re-installation
    /// for an already-bound executor replaces the binding, but running
    /// sessions keep the executor fixed at their task creation.
    pub fn install_managed_executor(
        &self,
        kind: ManagedExecutorKind,
        executor: Arc<dyn ManagedExecutorPort>,
    ) {
        self.inner
            .managed_executors
            .lock()
            .expect("executor registry")
            .insert(kind, executor);
        if tokio::runtime::Handle::try_current().is_ok() {
            self.inner.ensure_executor_fact_pumps();
        }
    }

    /// The executors a task-creation selector may offer: only real
    /// production adapters that are actually installed in this runtime.
    pub fn available_managed_executors(&self) -> Vec<ManagedExecutorKind> {
        let registry = self.inner.managed_executors.lock().expect("executor registry");
        ManagedExecutorKind::production_executors()
            .iter()
            .copied()
            .filter(|kind| registry.contains_key(kind))
            .collect()
    }

    /// The installed managed executors with their honest capability profiles
    /// (ADR-0078). The task-creation selector renders exactly these entries
    /// and degrades against the flags as-is; an executor without a production
    /// port never crosses the seam.
    pub fn managed_executor_profiles(&self) -> Vec<ManagedExecutorProfileSummary> {
        let registry = self.inner.managed_executors.lock().expect("executor registry");
        ManagedExecutorKind::production_executors()
            .iter()
            .copied()
            .filter_map(|kind| {
                let executor = registry.get(&kind)?;
                let profile = executor.capability_profile();
                Some(ManagedExecutorProfileSummary {
                    kind,
                    adapter_identity: profile.adapter_identity,
                    compatibility_profile: profile.compatibility_profile,
                    steer: profile.steer,
                    queue_events: profile.queue_events,
                    approval_channel: profile.approval_channel,
                    entry_read: profile.entry_read,
                    native_sandbox_modes: profile.native_sandbox_modes,
                })
            })
            .collect()
    }

    /// The workspace default executor for tasks created without an override.
    pub fn workspace_default_executor(&self) -> ManagedExecutorKind {
        *self
            .inner
            .workspace_default_executor
            .lock()
            .expect("workspace default executor")
    }

    /// Sets the workspace default executor. Fails closed when the executor
    /// is not installed; already running sessions never switch (ADR-0078 M3).
    pub fn set_workspace_default_executor(
        &self,
        kind: ManagedExecutorKind,
    ) -> Result<(), HaloWorkbenchError> {
        if !self.available_managed_executors().contains(&kind) {
            return Err(HaloWorkbenchError::new(
                "executor_unavailable",
                "The requested managed executor is not installed",
                "select_installed_executor",
            ));
        }
        *self
            .inner
            .workspace_default_executor
            .lock()
            .expect("workspace default executor") = kind;
        Ok(())
    }

    fn resolve_managed_executor(
        &self,
        kind: ManagedExecutorKind,
    ) -> Result<Arc<dyn ManagedExecutorPort>, HaloWorkbenchError> {
        self.inner
            .managed_executors
            .lock()
            .expect("executor registry")
            .get(&kind)
            .cloned()
            .ok_or_else(|| {
                HaloWorkbenchError::new(
                    "executor_unavailable",
                    "The requested managed executor is not installed",
                    "select_installed_executor",
                )
            })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HaloWorkbenchEvent> {
        self.inner.events.subscribe()
    }

    pub async fn submit(&self, request: HaloWorkbenchIntentRequest) -> IntentResult {
        if request.request_id.trim().is_empty() {
            return Err(HaloWorkbenchError::invalid_request(
                "A non-empty request identifier is required",
            ));
        }
        if self
            .inner
            .state
            .lock()
            .expect("Halo Workbench state lock")
            .terminated
        {
            return Err(HaloWorkbenchError::runtime_shutdown());
        }
        self.ensure_adapter_event_loop();

        let fingerprint = request_fingerprint(&request.intent)?;
        let (owner_sender, mut waiter) = {
            let mut ledger = self.inner.requests.lock().await;
            match ledger.records.get(&request.request_id) {
                Some(RequestRecord::Complete {
                    fingerprint: existing,
                    result,
                }) => {
                    return if existing == &fingerprint {
                        result.clone()
                    } else {
                        Err(HaloWorkbenchError::request_id_conflict())
                    };
                }
                Some(RequestRecord::InFlight {
                    fingerprint: existing,
                    result,
                }) => {
                    if existing != &fingerprint {
                        return Err(HaloWorkbenchError::request_id_conflict());
                    }
                    (None, Some(result.subscribe()))
                }
                None => {
                    let (sender, receiver) = watch::channel(None);
                    ledger.records.insert(
                        request.request_id.clone(),
                        RequestRecord::InFlight {
                            fingerprint,
                            result: sender.clone(),
                        },
                    );
                    (Some(sender), Some(receiver))
                }
            }
        };

        if owner_sender.is_none() {
            let waiter = waiter.as_mut().expect("duplicate request waiter");
            loop {
                if let Some(result) = waiter.borrow().clone() {
                    return result;
                }
                if waiter.changed().await.is_err() {
                    return Err(HaloWorkbenchError::new(
                        "runtime_internal",
                        "The Workbench request owner stopped unexpectedly",
                        "retry",
                    ));
                }
            }
        }

        let sender = owner_sender.expect("request owner sender");
        let runtime = self.clone();
        let request_id = request.request_id;
        let intent = request.intent;
        tokio::spawn(async move {
            let execution_runtime = runtime.clone();
            let execution_request_id = request_id.clone();
            let execution = tokio::spawn(async move {
                execution_runtime
                    .execute_intent(&execution_request_id, intent)
                    .await
            });
            let result = match execution.await {
                Ok(result) => result,
                Err(_) => {
                    let error = HaloWorkbenchError::new(
                        "runtime_internal",
                        "The Workbench request execution stopped unexpectedly",
                        "restart_application",
                    );
                    runtime
                        .inner
                        .fail_active_runtime(
                            error.clone(),
                            "Workbench Runtime request execution stopped unexpectedly",
                        )
                        .await;
                    Err(error)
                }
            };
            if let Err(error) = &result {
                runtime.inner.expose_error(error.clone());
            }
            sender.send_replace(Some(result.clone()));
            let mut ledger = runtime.inner.requests.lock().await;
            ledger.record_complete(request_id, fingerprint, result);
        });

        let waiter = waiter.as_mut().expect("request owner waiter");
        loop {
            if let Some(result) = waiter.borrow().clone() {
                return result;
            }
            if waiter.changed().await.is_err() {
                return Err(HaloWorkbenchError::new(
                    "runtime_internal",
                    "The Workbench request owner stopped unexpectedly",
                    "retry",
                ));
            }
        }
    }

    pub async fn shutdown(&self) -> Result<(), HaloWorkbenchError> {
        let runtime = self.clone();
        self.inner
            .shutdown_result
            .get_or_init(|| async move { runtime.shutdown_inner().await })
            .await
            .clone()
    }

    fn ensure_adapter_event_loop(&self) {
        if self
            .inner
            .adapter_events_started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let mut events = self.inner.adapter.subscribe();
        let inner: Weak<HaloWorkbenchRuntimeInner> = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.apply_adapter_event(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.fail_adapter_event_gap().await;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        let Some(inner) = inner.upgrade() else {
                            break;
                        };
                        inner.fail_adapter_event_stream_closed().await;
                        break;
                    }
                }
            }
        });
        self.inner.ensure_executor_fact_pumps();
    }

    async fn execute_intent(&self, request_id: &str, intent: HaloWorkbenchIntent) -> IntentResult {
        match intent {
            HaloWorkbenchIntent::OpenWorkspace { workspace } => {
                self.open_workspace(request_id, workspace).await
            }
            HaloWorkbenchIntent::CloseWorkspace => {
                self.close_workspace(Some(request_id), false).await?;
                Ok(self.inner.receipt(request_id, None))
            }
            HaloWorkbenchIntent::ConfirmManagedWorkspace {
                workspace_id,
                root_path,
            } => {
                self.confirm_managed_workspace(request_id, workspace_id, root_path)
                    .await
            }
            HaloWorkbenchIntent::CreateSession {
                task_id,
                mode,
                executor,
            } => self.create_session(request_id, task_id, mode, executor).await,
            HaloWorkbenchIntent::SendUserInput {
                session_id,
                content,
            } => {
                self.session_command(request_id, &session_id, SessionIntent::Prompt(content))
                    .await
            }
            HaloWorkbenchIntent::FollowUp {
                session_id,
                content,
            } => {
                self.session_command(request_id, &session_id, SessionIntent::FollowUp(content))
                    .await
            }
            HaloWorkbenchIntent::StopSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::Abort)
                    .await
            }
            HaloWorkbenchIntent::AbortSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::Abort)
                    .await
            }
            HaloWorkbenchIntent::EndSession { session_id } => {
                self.session_command(request_id, &session_id, SessionIntent::End)
                    .await
            }
            HaloWorkbenchIntent::ResolveOperation {
                operation_id,
                decision,
            } => {
                self.resolve_operation(request_id, &operation_id, decision)
                    .await
            }
            HaloWorkbenchIntent::FinishAndReview { session_id } => {
                self.finish_and_review(request_id, &session_id).await
            }
            HaloWorkbenchIntent::AcceptDelivery { session_id } => {
                self.resolve_delivery(
                    request_id,
                    &session_id,
                    HaloWorkbenchDeliveryDecision::Accepted,
                )
                .await
            }
            HaloWorkbenchIntent::RejectDelivery { session_id } => {
                self.resolve_delivery(
                    request_id,
                    &session_id,
                    HaloWorkbenchDeliveryDecision::Rejected,
                )
                .await
            }
        }
    }

    async fn open_workspace(
        &self,
        request_id: &str,
        workspace: HaloWorkbenchWorkspaceInput,
    ) -> IntentResult {
        validate_workspace_input(&workspace)?;
        let (cleanup_generation, generation) = {
            let _lifecycle = self.inner.lifecycle_actions.lock().await;
            let mut state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.terminated {
                return Err(HaloWorkbenchError::runtime_shutdown());
            }
            let cleanup_generation = state.adapter_generation;
            interrupt_managed_sessions(&mut state, &HaloWorkbenchError::workspace_closed());
            state.generation = state.generation.saturating_add(1);
            state.cleanup_started.clear();
            if cleanup_generation.is_some() || state.phase != HaloWorkbenchPhase::Disconnected {
                state.phase = HaloWorkbenchPhase::Stopping;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.error = None;
            }
            (cleanup_generation, state.generation)
        };

        if let Some(cleanup_generation) = cleanup_generation {
            self.cleanup_generation(cleanup_generation, generation, Some(request_id))
                .await?;
        }
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }

        let facts = self
            .inner
            .workspace_facts
            .inspect(WorkbenchWorkspaceFactsRequest {
                workspace_id: workspace.workspace_id.clone(),
                root: workspace.root_path.clone(),
            })
            .await;
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        let facts = match facts {
            Ok(facts) => facts,
            Err(_) => {
                let error = HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace facts could not be verified",
                    "retry",
                );
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
        };
        if facts.workspace_id != workspace.workspace_id {
            let error = HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "Workspace identity verification failed",
                "refresh_workspace",
            );
            self.inner
                .fail_generation(generation, Some(request_id), error.clone());
            return Err(error);
        }
        let adapter_workspace = PiRpcWorkspace {
            workspace_id: facts.workspace_id.clone(),
            canonical_root: facts.canonical_root.clone(),
        };
        let public_workspace = HaloWorkbenchWorkspaceSnapshot {
            workspace_id: facts.workspace_id,
            display_name: workspace.display_name,
            root_path: facts.canonical_root,
            trusted: facts.trusted,
            git_repository: facts.git_repository,
        };
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::WorkspaceChanged,
            "Workbench workspace is being probed",
            None,
            None,
            move |state| {
                if state.generation != generation || state.terminated {
                    return false;
                }
                state.workspace = Some(public_workspace);
                state.adapter_generation = Some(generation);
                state.managed_workspace_confirmation = None;
                retain_managed_interruption_facts(state);
                state.pending_operations.clear();
                state.phase = HaloWorkbenchPhase::Probing;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.error = None;
                true
            },
        );

        let probe = self
            .inner
            .adapter
            .execute(PiRpcCommand::Probe {
                generation,
                workspace: adapter_workspace.clone(),
            })
            .await
            .map_err(|error| port_failure(error.kind));
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        let adapter_readiness = match probe {
            Ok(PiRpcReply::Available { summary }) => {
                if !valid_adapter_profile_summary(&summary) {
                    let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                    self.inner
                        .fail_generation(generation, Some(request_id), error.clone());
                    return Err(error);
                }
                summary
            }
            Ok(PiRpcReply::Accepted) | Ok(PiRpcReply::Entries { .. }) => {
                let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
            Ok(PiRpcReply::Ready { .. }) => {
                let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
            Ok(PiRpcReply::Unavailable { reason }) => {
                let error = adapter_failure(reason);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
            Err(error) => {
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
        };
        let public_profile_readiness = HaloWorkbenchAdapterReadiness::from(&adapter_readiness);
        let public_profile_readiness_for_event = public_profile_readiness.clone();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime adapter profile was verified",
            None,
            None,
            move |state| {
                if state.generation != generation
                    || state.phase != HaloWorkbenchPhase::Probing
                    || state.terminated
                {
                    return false;
                }
                state.adapter_readiness = Some(public_profile_readiness_for_event);
                true
            },
        );

        let provider_readiness = self.inner.provider_readiness.check().await;
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        let provider_readiness = match provider_readiness {
            Ok(provider_readiness) => provider_readiness,
            Err(_) => {
                let error = HaloWorkbenchError::new(
                    "provider_readiness_unavailable",
                    "Pi provider readiness could not be verified",
                    "retry",
                );
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                return Err(error);
            }
        };
        if !provider_readiness.available {
            let error = HaloWorkbenchError::new(
                "provider_unavailable",
                "Pi provider/model readiness is not available",
                "configure_provider",
            );
            self.inner
                .fail_generation(generation, Some(request_id), error.clone());
            return Err(error);
        }

        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime is starting",
            None,
            None,
            move |state| {
                if state.generation != generation || state.terminated {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Starting;
                state.adapter_available = true;
                state.adapter_readiness = Some(public_profile_readiness);
                state.error = None;
                true
            },
        );
        let start = {
            let _action = self.inner.adapter_actions.lock().await;
            if !self.is_current_generation(generation) {
                return Ok(self.inner.receipt(request_id, None));
            }
            self.inner
                .adapter
                .execute(PiRpcCommand::Start {
                    generation,
                    workspace: adapter_workspace,
                })
                .await
                .map_err(|error| port_failure(error.kind))
        };
        if !self.is_current_generation(generation) {
            return Ok(self.inner.receipt(request_id, None));
        }
        match start {
            Ok(PiRpcReply::Ready { summary }) => {
                if !valid_adapter_ready_summary(&summary) {
                    let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                    self.inner
                        .fail_generation(generation, Some(request_id), error.clone());
                    return Err(error);
                }
                let public_adapter_readiness = HaloWorkbenchAdapterReadiness::from(&summary);
                self.inner.publish_transition(
                    Some(request_id),
                    HaloWorkbenchEventKind::RuntimeStateChanged,
                    "Workbench Runtime adapter readiness handshake was verified",
                    None,
                    None,
                    move |state| {
                        if state.generation != generation
                            || state.phase != HaloWorkbenchPhase::Starting
                            || state.terminated
                        {
                            return false;
                        }
                        state.adapter_readiness = Some(public_adapter_readiness);
                        true
                    },
                );
                Ok(self.inner.receipt(request_id, None))
            }
            Ok(PiRpcReply::Accepted) | Ok(PiRpcReply::Available { .. }) | Ok(PiRpcReply::Entries { .. }) => {
                let error = adapter_failure(PiRpcFailureKind::CapabilityMismatch);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                Err(error)
            }
            Ok(PiRpcReply::Unavailable { reason }) => {
                let error = adapter_failure(reason);
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                Err(error)
            }
            Err(error) => {
                self.inner
                    .fail_generation(generation, Some(request_id), error.clone());
                Err(error)
            }
        }
    }

    async fn close_workspace(
        &self,
        correlation_id: Option<&str>,
        terminate: bool,
    ) -> Result<(), HaloWorkbenchError> {
        let (cleanup_generation, generation) = {
            let _lifecycle = self.inner.lifecycle_actions.lock().await;
            let mut state = self.inner.state.lock().expect("Halo Workbench state lock");
            if terminate {
                state.terminated = true;
            }
            let cleanup_generation = state.adapter_generation;
            state.generation = state.generation.saturating_add(1);
            state.cleanup_started.clear();
            if cleanup_generation.is_some() || state.phase != HaloWorkbenchPhase::Disconnected {
                state.phase = HaloWorkbenchPhase::Stopping;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.error = None;
            }
            let close_error = if terminate {
                HaloWorkbenchError::runtime_shutdown()
            } else {
                HaloWorkbenchError::workspace_closed()
            };
            interrupt_managed_sessions(&mut state, &close_error);
            (cleanup_generation, state.generation)
        };
        if let Some(cleanup_generation) = cleanup_generation {
            self.cleanup_generation(cleanup_generation, generation, correlation_id)
                .await?;
        } else {
            self.inner.publish_transition(
                correlation_id,
                HaloWorkbenchEventKind::WorkspaceChanged,
                "Workbench workspace was closed",
                None,
                None,
                |state| {
                    if state.generation != generation
                        || (state.phase == HaloWorkbenchPhase::Disconnected
                            && state.workspace.is_none()
                            && state.pending_operations.is_empty()
                            && state.error.is_none())
                    {
                        return false;
                    }
                    state.phase = HaloWorkbenchPhase::Disconnected;
                    state.adapter_available = false;
                    state.adapter_readiness = None;
                    state.managed_workspace_confirmation = None;
                    state.workspace = None;
                    retain_managed_interruption_facts(state);
                    state.pending_operations.clear();
                    state.error = None;
                    true
                },
            );
        }
        Ok(())
    }

    async fn cleanup_generation(
        &self,
        cleanup_generation: u64,
        fence_generation: u64,
        correlation_id: Option<&str>,
    ) -> Result<(), HaloWorkbenchError> {
        self.inner.publish_transition(
            correlation_id,
            HaloWorkbenchEventKind::RuntimeStateChanged,
            "Workbench Runtime is stopping",
            None,
            None,
            |state| {
                if state.generation != fence_generation {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Stopping;
                true
            },
        );
        let result = self.inner.execute_cleanup_once(cleanup_generation).await;
        if !self.is_current_generation(fence_generation) {
            return Ok(());
        }
        if result.is_err() {
            let error = HaloWorkbenchError::new(
                "cleanup_failed",
                "Workbench Runtime cleanup did not complete",
                "restart_application",
            );
            self.inner
                .fail_generation(fence_generation, correlation_id, error.clone());
            return Err(error);
        }
        self.inner.publish_transition(
            correlation_id,
            HaloWorkbenchEventKind::WorkspaceChanged,
            "Workbench workspace was closed",
            None,
            None,
            |state| {
                if state.generation != fence_generation {
                    return false;
                }
                state.phase = HaloWorkbenchPhase::Disconnected;
                state.adapter_available = false;
                state.adapter_readiness = None;
                state.managed_workspace_confirmation = None;
                if state.adapter_generation == Some(cleanup_generation) {
                    state.adapter_generation = None;
                }
                state.workspace = None;
                retain_managed_interruption_facts(state);
                state.pending_operations.clear();
                state.error = None;
                true
            },
        );
        Ok(())
    }

    async fn confirm_managed_workspace(
        &self,
        request_id: &str,
        workspace_id: String,
        root_path: PathBuf,
    ) -> IntentResult {
        validate_workspace_confirmation(&workspace_id, &root_path)?;
        let generation = self.ready_generation()?;
        let expected_root = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            if workspace.workspace_id != workspace_id || workspace.root_path != root_path {
                return Err(HaloWorkbenchError::new(
                    "workspace_identity_mismatch",
                    "The confirmed workspace does not match the active canonical workspace",
                    "refresh_workspace",
                ));
            }
            workspace.root_path.clone()
        };

        let facts = self
            .inner
            .workspace_facts
            .confirm_managed_trust(WorkbenchWorkspaceTrustRequest {
                workspace_id: workspace_id.clone(),
                root: root_path,
            })
            .await
            .map_err(|_| {
                HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace trust could not be confirmed",
                    "retry",
                )
            })?;
        if facts.workspace_id != workspace_id || facts.canonical_root != expected_root {
            return Err(HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "Workspace identity verification failed",
                "refresh_workspace",
            ));
        }
        if !facts.git_repository {
            return Err(HaloWorkbenchError::managed_workspace_not_git());
        }
        if !facts.trusted {
            return Err(HaloWorkbenchError::new(
                "workspace_untrusted",
                "The workspace owner did not confirm managed execution",
                "confirm_managed_workspace",
            ));
        }

        let confirmation = ManagedWorkspaceConfirmation {
            generation,
            workspace_id: workspace_id.clone(),
            canonical_root: expected_root,
        };
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::WorkspaceChanged,
            "Workspace trust was explicitly confirmed for managed execution",
            None,
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(workspace) = state.workspace.as_mut() else {
                    return false;
                };
                if workspace.workspace_id != workspace_id
                    || workspace.root_path != confirmation.canonical_root
                {
                    return false;
                }
                workspace.trusted = true;
                state.managed_workspace_confirmation = Some(confirmation);
                true
            },
        );
        Ok(self.inner.receipt(request_id, None))
    }

    async fn create_session(
        &self,
        request_id: &str,
        task_id: String,
        mode: HaloWorkbenchSessionMode,
        executor: Option<ManagedExecutorKind>,
    ) -> IntentResult {
        validate_task_id(&task_id)?;
        let session_id = Uuid::new_v4().to_string();
        let generation = self.ready_generation()?;
        // ADR-0078 M3: the executor is resolved once, at task creation. The
        // override must name an installed production executor; otherwise the
        // workspace default is used. The session and its baseline record the
        // resolved executor for the whole task lifetime.
        let executor_kind = match executor {
            Some(kind) => {
                self.resolve_managed_executor(kind)?;
                kind
            }
            None => {
                let default_kind = self.workspace_default_executor();
                self.resolve_managed_executor(default_kind)?;
                default_kind
            }
        };
        let workspace_id = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state
                .workspace
                .as_ref()
                .map(|workspace| workspace.workspace_id.clone())
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?
        };
        if mode == HaloWorkbenchSessionMode::Managed {
            self.ensure_managed_workspace_confirmed(generation).await?;
        }
        let event_session_id = session_id.clone();
        let state_session_id = session_id.clone();
        let state_task_id = task_id.clone();
        let state_workspace_id = workspace_id.clone();
        if mode == HaloWorkbenchSessionMode::Managed {
            self.inner.append_managed_task_fact(
                &task_id,
                ManagedEventFactKind::TaskLifecycle,
                "Managed task session is being created",
                request_id,
            )?;
        }
        if !self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is being created",
            Some(event_session_id),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                if state.sessions.values().any(|session| {
                    session.workspace_id == state_workspace_id
                        && session.task_id == state_task_id
                        && !session.phase.is_terminal()
                }) {
                    return false;
                }
                state.sessions.insert(
                    state_session_id.clone(),
                    HaloWorkbenchSessionSnapshot {
                        workspace_id: state_workspace_id,
                        task_id: state_task_id,
                        session_id: state_session_id,
                        mode,
                        phase: HaloWorkbenchSessionPhase::Creating,
                        executor: executor_kind,
                        cancellation_mode: None,
                        baseline: None,
                        messages: Vec::new(),
                        activities: Vec::new(),
                        error: None,
                        delivery_review: None,
                    },
                );
                true
            },
        ) {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation == generation
                && state.sessions.values().any(|session| {
                    session.workspace_id == workspace_id
                        && session.task_id == task_id
                        && !session.phase.is_terminal()
                })
            {
                return Err(HaloWorkbenchError::task_already_active());
            }
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        if mode == HaloWorkbenchSessionMode::Managed {
            let baseline = match self.capture_managed_task_baseline(generation).await {
                Ok(baseline) => baseline,
                Err(error) => {
                    self.fail_session_before_adapter(
                        generation,
                        request_id,
                        &session_id,
                        error.clone(),
                    );
                    return Err(error);
                }
            };
            let baseline = HaloWorkbenchTaskBaselineSnapshot {
                executor: executor_kind,
                ..baseline
            };
            if !self.attach_session_baseline(generation, &session_id, baseline) {
                return Err(HaloWorkbenchError::session_not_found());
            }
            if !self.inner.append_managed_session_fact(
                generation,
                &session_id,
                ManagedEventFactKind::TaskBaselineLinked,
                "Managed task baseline linked",
                request_id,
            ) {
                return Err(HaloWorkbenchError::managed_event_facts_unavailable());
            }
        }
        let result = self
            .execute_session_adapter_action(
                generation,
                &task_id,
                &session_id,
                PiRpcCommand::CreateSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                    mode: mode.into(),
                },
                false,
            )
            .await;
        self.finish_session_command(
            generation,
            request_id,
            &session_id,
            result,
            HaloWorkbenchSessionPhase::Failed,
        )?;
        Ok(self.inner.receipt(request_id, Some(session_id)))
    }

    /// Routes the managed execution face through the session's bound
    /// executor. Returns `Ok(None)` for commands that have no executor-port
    /// face (session teardown, entry reads, approval resolution) so they
    /// keep their adapter path.
    async fn dispatch_managed_executor_action(
        &self,
        generation: u64,
        task_id: &str,
        session_id: &str,
        command: &PiRpcCommand,
    ) -> Result<Option<PiRpcReply>, HaloWorkbenchError> {
        let kind = self.session_executor_kind(generation, session_id);
        let executor = self.resolve_managed_executor(kind)?;
        let target = ManagedExecutorTarget {
            task_id: task_id.to_string(),
            session_id: session_id.to_string(),
        };
        let dispatched = match command {
            PiRpcCommand::SendUserInput { content, .. } => executor
                .prompt(ManagedExecutorPromptRequest {
                    target,
                    content: content.clone(),
                })
                .await
                .map(|_| PiRpcReply::Accepted),
            PiRpcCommand::FollowUp { content, .. } => executor
                .follow_up(ManagedExecutorPromptRequest {
                    target,
                    content: content.clone(),
                })
                .await
                .map(|_| PiRpcReply::Accepted),
            PiRpcCommand::AbortSession { .. } => {
                executor.abort(target).await.map(|_| PiRpcReply::Accepted)
            }
            _ => return Ok(None),
        };
        dispatched
            .map(Some)
            .map_err(|error| port_failure(error.kind))
    }

    fn session_executor_kind(&self, generation: u64, session_id: &str) -> ManagedExecutorKind {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation {
            return ManagedExecutorKind::PiRpc;
        }
        state
            .sessions
            .get(session_id)
            .map(|session| session.executor)
            .unwrap_or(ManagedExecutorKind::PiRpc)
    }

    async fn ensure_managed_workspace_confirmed(
        &self,
        generation: u64,
    ) -> Result<(), HaloWorkbenchError> {
        let confirmation = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state.managed_workspace_confirmation.clone()
        };
        let Some(confirmation) = confirmation else {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        };
        if confirmation.generation != generation {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        }
        let request = WorkbenchWorkspaceFactsRequest {
            workspace_id: confirmation.workspace_id.clone(),
            root: confirmation.canonical_root.clone(),
        };
        let facts = self
            .inner
            .workspace_facts
            .inspect(request.clone())
            .await
            .map_err(|_| {
                HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace trust could not be revalidated",
                    "retry",
                )
            })?;
        if facts.workspace_id != request.workspace_id
            || facts.canonical_root != request.root
            || !facts.git_repository
        {
            return Err(HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "The managed workspace changed after confirmation",
                "refresh_workspace",
            ));
        }
        if !facts.trusted {
            return Err(HaloWorkbenchError::new(
                "workspace_untrusted",
                "Managed workspace trust is no longer active",
                "confirm_managed_workspace",
            ));
        }
        Ok(())
    }

    async fn capture_managed_task_baseline(
        &self,
        generation: u64,
    ) -> Result<HaloWorkbenchTaskBaselineSnapshot, HaloWorkbenchError> {
        self.ensure_managed_workspace_confirmed(generation).await?;
        let request = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            WorkbenchTaskBaselineRequest {
                workspace_id: workspace.workspace_id.clone(),
                canonical_root: workspace.root_path.clone(),
            }
        };
        let baseline = self
            .inner
            .task_baseline
            .capture(request.clone())
            .await
            .map_err(|_| HaloWorkbenchError::task_baseline_unavailable())?;
        validate_task_baseline(&baseline)
            .map_err(|_| HaloWorkbenchError::task_baseline_unavailable())?;
        if baseline.canonical_root != request.canonical_root {
            return Err(HaloWorkbenchError::task_baseline_unavailable());
        }
        Ok(HaloWorkbenchTaskBaselineSnapshot {
            head: baseline.head,
            canonical_root: baseline.canonical_root,
            existing_changed_files: baseline.existing_changed_files,
            working_tree_fingerprint: baseline.working_tree_fingerprint,
            captured_at_ms: baseline.captured_at_ms,
            // The session resolves the real executor binding right after the
            // capture and stamps it onto this snapshot before attaching it.
            executor: ManagedExecutorKind::default(),
        })
    }

    fn attach_session_baseline(
        &self,
        generation: u64,
        session_id: &str,
        baseline: HaloWorkbenchTaskBaselineSnapshot,
    ) -> bool {
        let session_id = session_id.to_string();
        self.inner.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionStateChanged,
            "Managed task Git baseline was captured",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.mode != HaloWorkbenchSessionMode::Managed
                    || session.phase != HaloWorkbenchSessionPhase::Creating
                {
                    return false;
                }
                session.baseline = Some(baseline);
                true
            },
        )
    }

    fn fail_session_before_adapter(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        error: HaloWorkbenchError,
    ) {
        let session_id = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session command failed",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                session.phase = HaloWorkbenchSessionPhase::Failed;
                session.error = Some(error);
                true
            },
        );
    }

    async fn session_command(
        &self,
        request_id: &str,
        session_id: &str,
        intent: SessionIntent,
    ) -> IntentResult {
        if let SessionIntent::Prompt(content) | SessionIntent::FollowUp(content) = &intent {
            validate_user_input(content)?;
        }
        let generation = self.ready_generation()?;
        self.ensure_session_action_allowed(generation, session_id, &intent)?;
        let task_id = self.session_task_id(generation, session_id)?;
        let facts_managed = self.session_requires_managed_trust(generation, session_id)?;
        let allow_session_removal = matches!(&intent, SessionIntent::End);
        let command = match intent {
            SessionIntent::Prompt(content) => {
                if facts_managed {
                    self.inner.append_managed_task_fact(
                        &task_id,
                        ManagedEventFactKind::UserMessageSummary,
                        "Managed user message received",
                        request_id,
                    )?;
                }
                self.append_user_message(generation, session_id, &content)?;
                self.mark_session_running(
                    generation,
                    request_id,
                    session_id,
                    HaloWorkbenchSessionPhase::Idle,
                )?;
                PiRpcCommand::SendUserInput {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                    content,
                }
            }
            SessionIntent::FollowUp(content) => {
                if facts_managed {
                    self.inner.append_managed_task_fact(
                        &task_id,
                        ManagedEventFactKind::UserMessageSummary,
                        "Managed follow-up message received",
                        request_id,
                    )?;
                }
                self.append_user_message(generation, session_id, &content)?;
                self.mark_session_running(
                    generation,
                    request_id,
                    session_id,
                    HaloWorkbenchSessionPhase::WaitingDeveloper,
                )?;
                PiRpcCommand::FollowUp {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                    content,
                }
            }
            SessionIntent::Abort => {
                self.mark_session_stopping(
                    generation,
                    request_id,
                    session_id,
                    SessionIntent::Abort,
                )?;
                PiRpcCommand::AbortSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                }
            }
            SessionIntent::End => {
                self.mark_session_stopping(generation, request_id, session_id, SessionIntent::End)?;
                PiRpcCommand::EndSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                }
            }
        };
        let result = self
            .execute_session_adapter_action(
                generation,
                &task_id,
                session_id,
                command,
                allow_session_removal,
            )
            .await;
        self.finish_session_command(
            generation,
            request_id,
            session_id,
            result,
            HaloWorkbenchSessionPhase::Failed,
        )?;
        Ok(self.inner.receipt(request_id, Some(session_id.to_string())))
    }

    fn append_user_message(
        &self,
        generation: u64,
        session_id: &str,
        content: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let session_id = session_id.to_string();
        let content = redact_halo_text(content, MAX_PUBLIC_MESSAGE_BYTES);
        if content.trim().is_empty() {
            return Err(HaloWorkbenchError::invalid_request(
                "Non-empty user input is required",
            ));
        }
        if !self.inner.publish_transition(
            None,
            HaloWorkbenchEventKind::SessionMessageUpdated,
            "Workbench user message was recorded",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase.is_terminal() {
                    return false;
                }
                append_message(
                    &mut session.messages,
                    HaloWorkbenchMessageRole::User,
                    content,
                );
                true
            },
        ) {
            return Err(HaloWorkbenchError::session_not_found());
        }
        Ok(())
    }

    async fn execute_session_adapter_action(
        &self,
        generation: u64,
        task_id: &str,
        session_id: &str,
        command: PiRpcCommand,
        allow_session_removal: bool,
    ) -> Result<PiRpcReply, HaloWorkbenchError> {
        self.ensure_workspace_available(generation).await?;
        let managed = self.session_requires_managed_trust(generation, session_id)?;
        if managed {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        self.ensure_session_transport_allowed(generation, task_id, session_id)?;
        // ADR-0078 M3: managed sessions dispatch the execution face
        // (prompt / follow-up / abort) through the unified
        // ManagedExecutorPort bound to the session's fixed executor.
        if managed {
            if let Some(dispatched) = self
                .dispatch_managed_executor_action(generation, task_id, session_id, &command)
                .await?
            {
                return Ok(dispatched);
            }
        }
        let result = if matches!(&command, PiRpcCommand::AbortSession { .. }) {
            // A running prompt can legitimately wait for a Pi response. Abort
            // must still reach that session before the bounded response wait
            // completes; PiRpcAdapter serializes JSONL writes itself.
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        } else if matches!(
            &command,
            PiRpcCommand::SendUserInput { .. } | PiRpcCommand::FollowUp { .. }
        ) {
            // Prompts retain their existing cross-session serialization without
            // blocking a shutdown from fencing a running decision action.
            let _prompt = self.inner.prompt_actions.lock().await;
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        } else {
            let _action = self.inner.adapter_actions.lock().await;
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
            self.inner
                .adapter
                .execute(command)
                .await
                .map_err(|error| port_failure(error.kind))
        };
        self.ensure_workspace_available(generation).await?;
        if managed {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        if !allow_session_removal {
            self.ensure_session_transport_allowed(generation, task_id, session_id)?;
        }
        result
    }

    fn session_task_id(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Result<String, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        state
            .sessions
            .get(session_id)
            .map(|session| session.task_id.clone())
            .ok_or_else(HaloWorkbenchError::session_not_found)
    }

    async fn ensure_workspace_available(&self, generation: u64) -> Result<(), HaloWorkbenchError> {
        let request = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return Err(HaloWorkbenchError::runtime_not_ready());
            }
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            WorkbenchWorkspaceFactsRequest {
                workspace_id: workspace.workspace_id.clone(),
                root: workspace.root_path.clone(),
            }
        };

        let facts = self
            .inner
            .workspace_facts
            .inspect(request.clone())
            .await
            .map_err(|_| {
                HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace facts could not be revalidated",
                    "retry",
                )
            })?;
        if facts.workspace_id == request.workspace_id && facts.canonical_root == request.root {
            return Ok(());
        }

        let error = HaloWorkbenchError::new(
            "workspace_identity_mismatch",
            "The active workspace changed while the session was running",
            "refresh_workspace",
        );
        let _ = self.close_workspace(None, false).await;
        Err(error)
    }

    async fn ensure_managed_workspace_trusted(
        &self,
        generation: u64,
    ) -> Result<(), HaloWorkbenchError> {
        let confirmation = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state.managed_workspace_confirmation.clone()
        };
        let Some(confirmation) = confirmation else {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        };
        if confirmation.generation != generation {
            return Err(HaloWorkbenchError::managed_workspace_confirmation_required());
        }
        let request = WorkbenchWorkspaceFactsRequest {
            workspace_id: confirmation.workspace_id.clone(),
            root: confirmation.canonical_root.clone(),
        };
        let facts = self
            .inner
            .workspace_facts
            .inspect(request.clone())
            .await
            .map_err(|_| {
                HaloWorkbenchError::new(
                    "workspace_facts_unavailable",
                    "Workspace trust could not be revalidated",
                    "retry",
                )
            })?;
        if facts.workspace_id != request.workspace_id || facts.canonical_root != request.root {
            let error = HaloWorkbenchError::new(
                "workspace_identity_mismatch",
                "The managed workspace changed while the task was active",
                "refresh_workspace",
            );
            let _ = self.close_workspace(None, false).await;
            return Err(error);
        }
        if facts.git_repository && facts.trusted {
            return Ok(());
        }
        let error = if facts.git_repository {
            HaloWorkbenchError::new(
                "workspace_untrusted",
                "Workspace trust was revoked while the managed task was active",
                "confirm_managed_workspace",
            )
        } else {
            HaloWorkbenchError::managed_workspace_not_git()
        };
        let _ = self.close_workspace(None, false).await;
        Err(error)
    }

    fn session_requires_managed_trust(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Result<bool, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        state
            .sessions
            .get(session_id)
            .map(|session| session.mode == HaloWorkbenchSessionMode::Managed)
            .ok_or_else(HaloWorkbenchError::session_not_found)
    }

    fn ensure_session_action_allowed(
        &self,
        generation: u64,
        session_id: &str,
        intent: &SessionIntent,
    ) -> Result<(), HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.terminated
            || state.generation != generation
            || state.phase != HaloWorkbenchPhase::Ready
        {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(HaloWorkbenchError::session_not_found)?;
        if session.phase.is_terminal() {
            return Err(HaloWorkbenchError::session_terminal());
        }
        let allowed = match intent {
            SessionIntent::Prompt(_) => matches!(session.phase, HaloWorkbenchSessionPhase::Idle),
            SessionIntent::FollowUp(_) => {
                matches!(session.phase, HaloWorkbenchSessionPhase::WaitingDeveloper)
            }
            SessionIntent::Abort => matches!(session.phase, HaloWorkbenchSessionPhase::Running),
            SessionIntent::End => matches!(
                session.phase,
                HaloWorkbenchSessionPhase::Idle
                    | HaloWorkbenchSessionPhase::Running
                    | HaloWorkbenchSessionPhase::WaitingDeveloper
                    | HaloWorkbenchSessionPhase::Interrupted
            ),
        };
        if allowed {
            return Ok(());
        }
        if session.phase == HaloWorkbenchSessionPhase::Stopping
            || (session.phase == HaloWorkbenchSessionPhase::Running
                && matches!(
                    intent,
                    SessionIntent::Prompt(_) | SessionIntent::FollowUp(_)
                ))
        {
            Err(HaloWorkbenchError::session_busy())
        } else {
            Err(HaloWorkbenchError::session_not_ready())
        }
    }

    fn ensure_session_transport_allowed(
        &self,
        generation: u64,
        task_id: &str,
        session_id: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.terminated
            || state.generation != generation
            || state.phase != HaloWorkbenchPhase::Ready
        {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(HaloWorkbenchError::session_not_found)?;
        if session.task_id != task_id {
            return Err(HaloWorkbenchError::session_not_found());
        }
        if session.phase.is_terminal() {
            return Err(HaloWorkbenchError::session_terminal());
        }
        Ok(())
    }

    fn mark_session_running(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        expected_phase: HaloWorkbenchSessionPhase,
    ) -> Result<(), HaloWorkbenchError> {
        let session_id = session_id.to_string();
        if self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is running",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase != expected_phase {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Running;
                true
            },
        ) {
            Ok(())
        } else {
            Err(HaloWorkbenchError::session_busy())
        }
    }

    fn mark_session_stopping(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        intent: SessionIntent,
    ) -> Result<(), HaloWorkbenchError> {
        let session_id = session_id.to_string();
        if self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is stopping",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                let allowed = match intent {
                    SessionIntent::Abort => session.phase == HaloWorkbenchSessionPhase::Running,
                    SessionIntent::End => matches!(
                        session.phase,
                        HaloWorkbenchSessionPhase::Idle
                            | HaloWorkbenchSessionPhase::Running
                            | HaloWorkbenchSessionPhase::WaitingDeveloper
                            | HaloWorkbenchSessionPhase::Interrupted
                    ),
                    SessionIntent::Prompt(_) | SessionIntent::FollowUp(_) => false,
                };
                if !allowed {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Stopping;
                true
            },
        ) {
            Ok(())
        } else {
            Err(HaloWorkbenchError::session_busy())
        }
    }

    fn finish_session_command(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        result: Result<PiRpcReply, HaloWorkbenchError>,
        failure_phase: HaloWorkbenchSessionPhase,
    ) -> Result<(), HaloWorkbenchError> {
        let error = match result {
            Ok(PiRpcReply::Accepted)
            | Ok(PiRpcReply::Available { .. })
            | Ok(PiRpcReply::Ready { .. })
            | Ok(PiRpcReply::Entries { .. }) => return Ok(()),
            Ok(PiRpcReply::Unavailable { reason }) => adapter_failure(reason),
            Err(error) => error,
        };
        let session_id = session_id.to_string();
        let session_error = error.clone();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session command failed",
            Some(session_id.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id) else {
                    return false;
                };
                if session.phase.rejects_adapter_events() {
                    return false;
                }
                let projected_phase = if session.mode == HaloWorkbenchSessionMode::Managed
                    && failure_phase == HaloWorkbenchSessionPhase::Failed
                {
                    HaloWorkbenchSessionPhase::Interrupted
                } else {
                    failure_phase
                };
                if !valid_session_transition(session.phase, projected_phase) {
                    return false;
                }
                session.phase = projected_phase;
                session.error = Some(session_error);
                session.cancellation_mode = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != session_id);
                true
            },
        );
        Err(error)
    }

    async fn resolve_operation(
        &self,
        request_id: &str,
        operation_id: &str,
        decision: HaloWorkbenchOperationDecision,
    ) -> IntentResult {
        let generation = self.ready_generation()?;
        let (task_id, session_id) = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            state
                .pending_operations
                .get(operation_id)
                .map(|operation| (operation.task_id.clone(), operation.session_id.clone()))
                .ok_or_else(HaloWorkbenchError::operation_not_found)?
        };
        self.ensure_workspace_available(generation).await?;
        if self.session_requires_managed_trust(generation, &session_id)? {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        self.ensure_session_transport_allowed(generation, &task_id, &session_id)?;
        validate_operation_decision(&decision)?;
        let owned_operation_id = operation_id.to_string();
        let claimed = self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::OperationRequested,
            "Workbench operation decision was submitted",
            Some(session_id.clone()),
            Some(owned_operation_id.clone()),
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(operation) = state.pending_operations.get_mut(&owned_operation_id) else {
                    return false;
                };
                if operation.phase != HaloWorkbenchPendingOperationPhase::AwaitingDecision {
                    return false;
                }
                operation.phase = HaloWorkbenchPendingOperationPhase::DecisionSubmitted;
                true
            },
        );
        if !claimed {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return Err(HaloWorkbenchError::runtime_not_ready());
            }
            return if state.pending_operations.contains_key(operation_id) {
                Err(HaloWorkbenchError::operation_decision_in_progress())
            } else {
                Err(HaloWorkbenchError::operation_not_found())
            };
        }
        let result = {
            let _action = self.inner.adapter_actions.lock().await;
            self.ensure_session_transport_allowed(generation, &task_id, &session_id)?;
            let operation_is_claimed = self
                .inner
                .state
                .lock()
                .expect("Halo Workbench state lock")
                .pending_operations
                .get(operation_id)
                .is_some_and(|operation| {
                    operation.session_id == session_id
                        && operation.phase == HaloWorkbenchPendingOperationPhase::DecisionSubmitted
                });
            if !operation_is_claimed {
                return Err(HaloWorkbenchError::operation_not_found());
            }
            let result = self
                .inner
                .adapter
                .execute(PiRpcCommand::ResolveOperation {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                    operation_id: operation_id.to_string(),
                    decision: decision.into(),
                })
                .await
                .map_err(|error| port_failure(error.kind));
            result
        };
        self.ensure_workspace_available(generation).await?;
        if self.session_requires_managed_trust(generation, &session_id)? {
            self.ensure_managed_workspace_trusted(generation).await?;
        }
        self.ensure_session_transport_allowed(generation, &task_id, &session_id)?;
        match result {
            Ok(PiRpcReply::Accepted)
            | Ok(PiRpcReply::Available { .. })
            | Ok(PiRpcReply::Ready { .. })
            | Ok(PiRpcReply::Entries { .. }) => Ok(self.inner.receipt(request_id, Some(session_id))),
            Ok(PiRpcReply::Unavailable { reason }) => {
                let error = adapter_failure(reason);
                self.restore_operation(generation, request_id, operation_id, &session_id);
                Err(error)
            }
            Err(error) => {
                self.restore_operation(generation, request_id, operation_id, &session_id);
                Err(error)
            }
        }
    }

    /// Explicitly closes the logical session for delivery review. A settled
    /// session releases its adapter session after freezing bounded/redacted
    /// evidence. An interrupted session is already transport-isolated, so its
    /// explicit review path must not contact Pi again.
    async fn finish_and_review(&self, request_id: &str, session_id: &str) -> IntentResult {
        let generation = self.ready_generation()?;
        let Some(entry) = self.enter_delivery_review(generation, request_id, session_id) else {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            return if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                Err(HaloWorkbenchError::runtime_not_ready())
            } else if !state.sessions.contains_key(session_id) {
                Err(HaloWorkbenchError::session_not_found())
            } else {
                Err(HaloWorkbenchError::delivery_review_not_ready())
            };
        };

        let settled = match entry {
            DeliveryReviewEntry::Settled => {
                self.await_settled_fingerprint(generation, session_id).await
            }
            DeliveryReviewEntry::Interrupted => None,
        };
        let evidence = match self
            .capture_delivery_evidence(generation, session_id, settled)
            .await
        {
            Ok(evidence) => evidence,
            Err(error) => {
                self.handle_delivery_review_failure(
                    entry,
                    generation,
                    request_id,
                    session_id,
                    error.clone(),
                );
                return Err(error);
            }
        };
        let review = match self.build_delivery_review(generation, session_id, evidence) {
            Ok(review) => review,
            Err(error) => {
                self.handle_delivery_review_failure(
                    entry,
                    generation,
                    request_id,
                    session_id,
                    error.clone(),
                );
                return Err(error);
            }
        };
        if !self.attach_delivery_review(generation, request_id, session_id, review) {
            let error = HaloWorkbenchError::session_not_found();
            self.handle_delivery_review_failure(
                entry,
                generation,
                request_id,
                session_id,
                error.clone(),
            );
            return Err(error);
        }

        if entry == DeliveryReviewEntry::Settled {
            self.release_adapter_session(generation, request_id, session_id)
                .await?;
        }
        Ok(self.inner.receipt(request_id, Some(session_id.to_string())))
    }

    /// Records the developer's accept/reject conclusion. No Git write, commit,
    /// push, rollback, file deletion, branch creation or history rewrite is
    /// performed here.
    async fn resolve_delivery(
        &self,
        request_id: &str,
        session_id: &str,
        decision: HaloWorkbenchDeliveryDecision,
    ) -> IntentResult {
        let session_id_owned = session_id.to_string();
        if self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench delivery was resolved",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.terminated {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                let active_review = state.phase == HaloWorkbenchPhase::Ready
                    && session.phase == HaloWorkbenchSessionPhase::Reviewing;
                let interrupted_history = state.phase != HaloWorkbenchPhase::Stopping
                    && session.phase == HaloWorkbenchSessionPhase::Interrupted
                    && session.delivery_review.is_some();
                if session.mode != HaloWorkbenchSessionMode::Managed
                    || (!active_review && !interrupted_history)
                {
                    return false;
                }
                let Some(review) = session.delivery_review.as_mut() else {
                    return false;
                };
                if review.decision.is_some() {
                    return false;
                }
                review.decision = Some(decision);
                session.phase = HaloWorkbenchSessionPhase::Ended;
                session.error = None;
                state
                    .pending_operations
                    .retain(|_, operation| operation.session_id != session_id_owned);
                true
            },
        ) {
            Ok(self.inner.receipt(request_id, Some(session_id.to_string())))
        } else {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.terminated {
                Err(HaloWorkbenchError::runtime_shutdown())
            } else if !state.sessions.contains_key(session_id) {
                Err(HaloWorkbenchError::session_not_found())
            } else if state.phase != HaloWorkbenchPhase::Ready
                && state
                    .sessions
                    .get(session_id)
                    .is_some_and(|session| session.phase != HaloWorkbenchSessionPhase::Interrupted)
            {
                Err(HaloWorkbenchError::runtime_not_ready())
            } else {
                Err(HaloWorkbenchError::delivery_decision_not_ready())
            }
        }
    }

    fn enter_delivery_review(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
    ) -> Option<DeliveryReviewEntry> {
        let session_id_owned = session_id.to_string();
        let mut entry = None;
        let transitioned = self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session is in delivery review",
            Some(session_id_owned.clone()),
            None,
            |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let active_workspace_id = state
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.workspace_id.clone());
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.mode != HaloWorkbenchSessionMode::Managed
                    || active_workspace_id.as_deref() != Some(session.workspace_id.as_str())
                {
                    return false;
                }
                let review_entry = match session.phase {
                    HaloWorkbenchSessionPhase::WaitingDeveloper
                        if session.delivery_review.is_none() =>
                    {
                        DeliveryReviewEntry::Settled
                    }
                    HaloWorkbenchSessionPhase::Interrupted if session.delivery_review.is_none() => {
                        DeliveryReviewEntry::Interrupted
                    }
                    _ => return false,
                };
                session.phase = HaloWorkbenchSessionPhase::Reviewing;
                entry = Some(review_entry);
                true
            },
        );
        transitioned.then_some(entry).flatten()
    }

    fn attach_delivery_review(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        review: HaloWorkbenchDeliveryReviewSnapshot,
    ) -> bool {
        let session_id_owned = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench delivery evidence was frozen",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Reviewing {
                    return false;
                }
                session.delivery_review = Some(review);
                true
            },
        )
    }

    fn fail_session_phase(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
        error: HaloWorkbenchError,
    ) {
        let session_id_owned = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Workbench session command failed",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.phase.is_terminal()
                    || session.phase == HaloWorkbenchSessionPhase::Interrupted
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Failed;
                session.error = Some(error);
                true
            },
        );
    }

    fn handle_delivery_review_failure(
        &self,
        entry: DeliveryReviewEntry,
        generation: u64,
        request_id: &str,
        session_id: &str,
        error: HaloWorkbenchError,
    ) {
        match entry {
            DeliveryReviewEntry::Settled => {
                self.fail_session_phase(generation, request_id, session_id, error);
            }
            DeliveryReviewEntry::Interrupted => {
                self.restore_interrupted_delivery_review(generation, request_id, session_id);
            }
        }
    }

    fn restore_interrupted_delivery_review(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
    ) {
        let session_id_owned = session_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::SessionStateChanged,
            "Interrupted delivery review remains available",
            Some(session_id_owned.clone()),
            None,
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(session) = state.sessions.get_mut(&session_id_owned) else {
                    return false;
                };
                if session.phase != HaloWorkbenchSessionPhase::Reviewing
                    || session.delivery_review.is_some()
                {
                    return false;
                }
                session.phase = HaloWorkbenchSessionPhase::Interrupted;
                true
            },
        );
    }

    async fn await_settled_fingerprint(
        &self,
        generation: u64,
        session_id: &str,
    ) -> Option<WorkbenchDeliveryFingerprint> {
        let mut receiver = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation {
                return None;
            }
            state.settled_fingerprints.get(session_id).cloned()?
        };
        let current = receiver.borrow().clone();
        if current.is_some() {
            return current;
        }
        if tokio::time::timeout(Duration::from_secs(5), receiver.changed())
            .await
            .is_err()
        {
            return None;
        }
        let result = receiver.borrow().clone();
        result
    }

    async fn capture_delivery_evidence(
        &self,
        generation: u64,
        session_id: &str,
        settled: Option<WorkbenchDeliveryFingerprint>,
    ) -> Result<WorkbenchDeliveryEvidence, HaloWorkbenchError> {
        let request = {
            let state = self.inner.state.lock().expect("Halo Workbench state lock");
            if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
                return Err(HaloWorkbenchError::runtime_not_ready());
            }
            let session = state
                .sessions
                .get(session_id)
                .ok_or_else(HaloWorkbenchError::session_not_found)?;
            let baseline = session
                .baseline
                .as_ref()
                .ok_or_else(HaloWorkbenchError::task_baseline_unavailable)?;
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(HaloWorkbenchError::runtime_not_ready)?;
            WorkbenchDeliveryEvidenceRequest {
                workspace_id: workspace.workspace_id.clone(),
                canonical_root: workspace.root_path.clone(),
                baseline: WorkbenchTaskBaseline {
                    head: baseline.head.clone(),
                    canonical_root: baseline.canonical_root.clone(),
                    existing_changed_files: baseline.existing_changed_files.clone(),
                    working_tree_fingerprint: baseline.working_tree_fingerprint.clone(),
                    captured_at_ms: baseline.captured_at_ms,
                },
                settled,
            }
        };
        self.inner
            .delivery_evidence
            .capture(request)
            .await
            .map_err(|_| HaloWorkbenchError::delivery_evidence_unavailable())
    }

    fn build_delivery_review(
        &self,
        generation: u64,
        session_id: &str,
        evidence: WorkbenchDeliveryEvidence,
    ) -> Result<HaloWorkbenchDeliveryReviewSnapshot, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.generation != generation || state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(HaloWorkbenchError::session_not_found)?;
        Ok(HaloWorkbenchDeliveryReviewSnapshot {
            evidence: HaloWorkbenchDeliveryEvidenceSnapshot {
                captured_at_ms: evidence.captured_at_ms,
                head: evidence.head,
                working_tree_fingerprint: evidence.working_tree_fingerprint,
                changed_files: evidence.changed_files,
                diff_preview: redact_halo_text(&evidence.diff_preview, MAX_DELIVERY_DIFF_BYTES),
                attribution: evidence
                    .attribution
                    .into_iter()
                    .map(|item| HaloWorkbenchDeliveryAttributionSnapshot {
                        path: item.path,
                        kind: item.kind.into(),
                    })
                    .collect(),
            },
            summary: summarize_delivery_messages(&session.messages),
            verification_results: summarize_delivery_activities(&session.activities),
            run_conclusion: session
                .messages
                .iter()
                .rev()
                .find(|message| message.role == HaloWorkbenchMessageRole::Assistant)
                .map(|message| redact_halo_text(&message.content, MAX_DELIVERY_SUMMARY_BYTES))
                .unwrap_or_default(),
            decision: None,
        })
    }

    async fn release_adapter_session(
        &self,
        generation: u64,
        request_id: &str,
        session_id: &str,
    ) -> Result<(), HaloWorkbenchError> {
        let task_id = self.session_task_id(generation, session_id)?;
        let result = self
            .execute_session_adapter_action(
                generation,
                &task_id,
                session_id,
                PiRpcCommand::EndSession {
                    generation,
                    task_id: task_id.clone(),
                    session_id: session_id.to_string(),
                },
                true,
            )
            .await;
        self.finish_session_command(
            generation,
            request_id,
            session_id,
            result,
            HaloWorkbenchSessionPhase::Failed,
        )?;
        Ok(())
    }

    fn restore_operation(
        &self,
        generation: u64,
        request_id: &str,
        operation_id: &str,
        session_id: &str,
    ) {
        let operation_id = operation_id.to_string();
        self.inner.publish_transition(
            Some(request_id),
            HaloWorkbenchEventKind::OperationRequested,
            "Workbench operation decision was not accepted",
            Some(session_id.to_string()),
            Some(operation_id.clone()),
            move |state| {
                if state.generation != generation {
                    return false;
                }
                let Some(operation) = state.pending_operations.get_mut(&operation_id) else {
                    return false;
                };
                operation.phase = HaloWorkbenchPendingOperationPhase::AwaitingDecision;
                true
            },
        );
    }

    fn ready_generation(&self) -> Result<u64, HaloWorkbenchError> {
        let state = self.inner.state.lock().expect("Halo Workbench state lock");
        if state.terminated {
            return Err(HaloWorkbenchError::runtime_shutdown());
        }
        if state.phase != HaloWorkbenchPhase::Ready {
            return Err(HaloWorkbenchError::runtime_not_ready());
        }
        Ok(state.generation)
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        self.inner
            .state
            .lock()
            .expect("Halo Workbench state lock")
            .generation
            == generation
    }

    async fn shutdown_inner(&self) -> Result<(), HaloWorkbenchError> {
        self.close_workspace(None, true).await
    }
}

enum SessionIntent {
    Prompt(String),
    FollowUp(String),
    Abort,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryReviewEntry {
    Settled,
    Interrupted,
}

pub(super) fn valid_adapter_profile_summary(summary: &PiRpcAvailabilitySummary) -> bool {
    summary.version.profile == summary.version.version.compatibility_profile()
        && summary.version.evidence_source == PiRpcVersionEvidenceSource::LocalVersionProbe
        && summary.capabilities.required.as_slice() == PiRpcCapability::required_p0()
        && summary.capabilities.verified.is_empty()
}

pub(super) fn valid_adapter_ready_summary(summary: &PiRpcAvailabilitySummary) -> bool {
    summary.version.profile == summary.version.version.compatibility_profile()
        && summary.version.evidence_source == PiRpcVersionEvidenceSource::LocalVersionProbe
        && summary.capabilities.required.as_slice() == PiRpcCapability::required_p0()
        && summary.capabilities.verified.as_slice()
            == PiRpcCapability::verified_by_readiness_handshake()
}

