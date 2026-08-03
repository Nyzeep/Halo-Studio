import { createStore, type StoreApi } from 'zustand/vanilla';

import {
  createTauriWorkbenchRuntimeTransport,
  createWorkbenchRuntimeClient,
  type WorkbenchRuntimeClient,
  type WorkbenchRuntimeUnlisten,
} from './client';
import {
  HALO_WORKBENCH_SCHEMA_VERSION,
  PI_RPC_ADAPTER_IDENTITY,
  type WorkbenchPiRpcCapability,
  type WorkbenchPiRpcCompatibilityProfile,
  type WorkbenchPiRpcVersion,
  type WorkbenchPiRpcVersionEvidenceSource,
  type WorkbenchRuntimeEvent,
  type WorkbenchRuntimeIntentReceipt,
  type WorkbenchRuntimeIntentRequest,
  type WorkbenchRuntimeSnapshot,
} from './types';

const EVENT_BUFFER_LIMIT = 256;
const MAX_RESYNC_READS_WITHOUT_PROGRESS = 2;
const RUNTIME_PHASES = new Set([
  'disconnected',
  'probing',
  'starting',
  'ready',
  'failed',
  'stopping',
]);
const SESSION_MODES = new Set(['standard', 'managed']);
const SESSION_PHASES = new Set([
  'creating',
  'idle',
  'running',
  'waitingDeveloper',
  'interrupted',
  'stopping',
  'ended',
  'failed',
]);
const OPERATION_KINDS = new Set(['permission']);
const OPERATION_PHASES = new Set(['awaitingDecision', 'decisionSubmitted']);
const PI_RPC_VERSIONS = new Set(['0.81.1', '0.83.0']);
const PI_RPC_COMPATIBILITY_PROFILES = new Set([
  'pi-rpc-0.81.1-p0',
  'pi-rpc-0.83.0-p0',
]);
const PI_RPC_VERSION_EVIDENCE_SOURCES = new Set(['local_version_probe']);
const PI_RPC_REQUIRED_CAPABILITIES = [
  'prompt',
  'follow_up',
  'abort',
  'get_state',
  'get_entries',
  'get_entries.entries',
  'get_entries.leaf_id',
  'get_entries.since',
  'message_update',
  'tool_execution_start',
  'tool_execution_update',
  'tool_execution_end',
  'agent_settled',
  'extension_ui_request',
  'extension_ui_response',
] as const;
const PI_RPC_CAPABILITIES = new Set<string>(PI_RPC_REQUIRED_CAPABILITIES);
const EVENT_KINDS = new Set([
  'runtimeStateChanged',
  'workspaceChanged',
  'sessionStateChanged',
  'operationRequested',
  'operationResolved',
]);
const EVENT_SUMMARIES = new Set([
  'Workbench Runtime is ready',
  'Workbench Runtime adapter profile was verified',
  'Workbench Runtime is starting',
  'Workbench Runtime is stopping',
  'Workbench Runtime failed',
  'Workbench Runtime event stream failed',
  'Workbench workspace is being probed',
  'Workbench workspace was closed',
  'Workbench session is being created',
  'Workbench session is idle',
  'Workbench session is running',
  'Workbench session is waiting for developer',
  'Workbench session was interrupted',
  'Workbench session is stopping',
  'Workbench session command failed',
  'Workbench session ended',
  'Workbench session failed',
  'A Workbench operation requires a decision',
  'Workbench operation was resolved',
  'Workbench operation decision was submitted',
  'Workbench operation decision was not accepted',
]);
const WORKBENCH_ERROR_CODES = new Set([
  'adapter_access_denied',
  'adapter_cancelled',
  'adapter_event_gap',
  'adapter_event_stream_closed',
  'adapter_timeout',
  'adapter_unavailable',
  'cleanup_failed',
  'invalid_request',
  'operation_decision_in_progress',
  'operation_not_found',
  'pi_authentication_failed',
  'pi_capability_mismatch',
  'pi_internal_error',
  'pi_not_installed',
  'pi_protocol_error',
  'pi_transport_unavailable',
  'pi_version_unsupported',
  'provider_readiness_unavailable',
  'provider_unavailable',
  'request_id_conflict',
  'runtime_internal',
  'runtime_not_ready',
  'runtime_shutdown',
  'session_not_found',
  'session_terminal',
  'workspace_facts_unavailable',
  'workspace_identity_mismatch',
  'workspace_untrusted',
]);
const WORKBENCH_ERROR_RECOVERY_ACTIONS = new Set([
  'choose_trusted_workspace',
  'configure_provider',
  'correct_request',
  'create_new_request',
  'create_new_session',
  'install_pi',
  'refresh_runtime_snapshot',
  'refresh_workspace',
  'restart_application',
  'restart_runtime',
  'retry',
  'retry_after_runtime_ready',
  'review_system_permissions',
  'upgrade_pi',
  'wait_for_operation_confirmation',
]);
const SAFE_RUNTIME_ERROR_SUMMARY = 'The Halo Workbench Runtime reported an error';

export type WorkbenchRuntimeSyncStatus =
  | 'idle'
  | 'bootstrapping'
  | 'ready'
  | 'resyncing'
  | 'failed';

export interface WorkbenchRuntimeStoreState {
  syncStatus: WorkbenchRuntimeSyncStatus;
  snapshot: WorkbenchRuntimeSnapshot | null;
  lastEvent: WorkbenchRuntimeEvent | null;
  stableErrorCode: string | null;
  start: () => Promise<void>;
  stop: () => void;
  submitIntent: (
    request: WorkbenchRuntimeIntentRequest,
  ) => Promise<WorkbenchRuntimeIntentReceipt>;
}

const isValidCounter = (value: number): boolean =>
  Number.isSafeInteger(value) && value >= 0;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const contractMismatch = (): never => {
  throw new Error('runtime_contract_mismatch');
};

const nullableString = (value: unknown): string | null => {
  if (value === null) return null;
  if (typeof value !== 'string') return contractMismatch();
  return value;
};

const sanitizeRuntimeError = (input: unknown): WorkbenchRuntimeSnapshot['error'] => {
  if (!isRecord(input)
    || typeof input.code !== 'string'
    || !WORKBENCH_ERROR_CODES.has(input.code)
    || typeof input.recoveryAction !== 'string'
    || typeof input.summary !== 'string') {
    return contractMismatch();
  }

  return {
    code: input.code,
    recoveryAction: WORKBENCH_ERROR_RECOVERY_ACTIONS.has(input.recoveryAction)
      ? input.recoveryAction
      : 'retry',
    summary: SAFE_RUNTIME_ERROR_SUMMARY,
  };
};

const sanitizeAdapterReadiness = (
  input: unknown,
): WorkbenchRuntimeSnapshot['adapter']['readiness'] => {
  if (input === null || input === undefined) return null;
  if (
    !isRecord(input)
    || !isRecord(input.version)
    || !isRecord(input.capabilities)
    || typeof input.version.version !== 'string'
    || !PI_RPC_VERSIONS.has(input.version.version)
    || typeof input.version.profile !== 'string'
    || !PI_RPC_COMPATIBILITY_PROFILES.has(input.version.profile)
    || typeof input.version.evidenceSource !== 'string'
    || !PI_RPC_VERSION_EVIDENCE_SOURCES.has(input.version.evidenceSource)
    || !Array.isArray(input.capabilities.required)
    || input.capabilities.required.length !== PI_RPC_REQUIRED_CAPABILITIES.length
  ) {
    return contractMismatch();
  }

  const required = input.capabilities.required.map(capability => {
    if (typeof capability !== 'string' || !PI_RPC_CAPABILITIES.has(capability)) {
      return contractMismatch();
    }
    return capability as WorkbenchPiRpcCapability;
  });
  if (required.some((capability, index) => capability !== PI_RPC_REQUIRED_CAPABILITIES[index])) {
    return contractMismatch();
  }

  return {
    version: {
      version: input.version.version as WorkbenchPiRpcVersion,
      profile: input.version.profile as WorkbenchPiRpcCompatibilityProfile,
      evidenceSource: input.version.evidenceSource as WorkbenchPiRpcVersionEvidenceSource,
    },
    capabilities: { required },
  };
};

const sanitizeSnapshot = (input: unknown): WorkbenchRuntimeSnapshot => {
  if (!isRecord(input) || !isRecord(input.adapter)) return contractMismatch();
  if (
    input.schemaVersion !== HALO_WORKBENCH_SCHEMA_VERSION
    || !RUNTIME_PHASES.has(String(input.phase))
    || input.adapter.identity !== PI_RPC_ADAPTER_IDENTITY
    || typeof input.adapter.available !== 'boolean'
    || !Array.isArray(input.sessions)
    || !Array.isArray(input.pendingOperations)
    || typeof input.lastSequence !== 'number'
    || !isValidCounter(input.lastSequence)
    || typeof input.stateVersion !== 'number'
    || !isValidCounter(input.stateVersion)
  ) {
    return contractMismatch();
  }

  const workspace = input.workspace === null
    ? null
    : (() => {
        if (
          !isRecord(input.workspace)
          || typeof input.workspace.workspaceId !== 'string'
          || typeof input.workspace.displayName !== 'string'
          || typeof input.workspace.rootPath !== 'string'
          || typeof input.workspace.trusted !== 'boolean'
          || typeof input.workspace.gitRepository !== 'boolean'
        ) {
          return contractMismatch();
        }
        return {
          workspaceId: input.workspace.workspaceId,
          displayName: input.workspace.displayName,
          rootPath: input.workspace.rootPath,
          trusted: input.workspace.trusted,
          gitRepository: input.workspace.gitRepository,
        };
      })();

  const sessions = input.sessions.map(session => {
    if (
      !isRecord(session)
      || typeof session.sessionId !== 'string'
      || !SESSION_MODES.has(String(session.mode))
      || !SESSION_PHASES.has(String(session.phase))
    ) {
      return contractMismatch();
    }
    return {
      sessionId: session.sessionId,
      mode: session.mode as WorkbenchRuntimeSnapshot['sessions'][number]['mode'],
      phase: session.phase as WorkbenchRuntimeSnapshot['sessions'][number]['phase'],
    };
  });

  const pendingOperations = input.pendingOperations.map(operation => {
    if (
      !isRecord(operation)
      || typeof operation.operationId !== 'string'
      || typeof operation.sessionId !== 'string'
      || !OPERATION_KINDS.has(String(operation.kind))
      || !OPERATION_PHASES.has(String(operation.phase))
    ) {
      return contractMismatch();
    }
    return {
      operationId: operation.operationId,
      sessionId: operation.sessionId,
      kind: operation.kind as WorkbenchRuntimeSnapshot['pendingOperations'][number]['kind'],
      phase: operation.phase as WorkbenchRuntimeSnapshot['pendingOperations'][number]['phase'],
    };
  });

  const runtimeError = input.error === null ? null : sanitizeRuntimeError(input.error);

  return {
    schemaVersion: HALO_WORKBENCH_SCHEMA_VERSION,
    phase: input.phase as WorkbenchRuntimeSnapshot['phase'],
    adapter: {
      identity: PI_RPC_ADAPTER_IDENTITY,
      available: input.adapter.available,
      readiness: sanitizeAdapterReadiness(input.adapter.readiness),
    },
    workspace,
    sessions,
    pendingOperations,
    lastSequence: input.lastSequence,
    stateVersion: input.stateVersion,
    error: runtimeError,
  };
};

const sanitizeEvent = (input: unknown): WorkbenchRuntimeEvent | null => {
  if (
    !isRecord(input)
    || typeof input.sequence !== 'number'
    || !isValidCounter(input.sequence)
    || input.sequence === 0
    || typeof input.stateVersion !== 'number'
    || !isValidCounter(input.stateVersion)
    || !EVENT_KINDS.has(String(input.kind))
    || typeof input.summary !== 'string'
    || !EVENT_SUMMARIES.has(input.summary)
    || typeof input.occurredAtMs !== 'number'
    || !isValidCounter(input.occurredAtMs)
  ) {
    return null;
  }

  try {
    return {
      sequence: input.sequence,
      stateVersion: input.stateVersion,
      correlationId: nullableString(input.correlationId),
      kind: input.kind as WorkbenchRuntimeEvent['kind'],
      summary: input.summary,
      sessionId: nullableString(input.sessionId),
      operationId: nullableString(input.operationId),
      occurredAtMs: input.occurredAtMs,
    };
  } catch {
    return null;
  }
};

const once = (unlisten: WorkbenchRuntimeUnlisten): WorkbenchRuntimeUnlisten => {
  let called = false;
  return () => {
    if (called) return;
    called = true;
    unlisten();
  };
};

export const createWorkbenchRuntimeStore = (
  client: WorkbenchRuntimeClient,
): StoreApi<WorkbenchRuntimeStoreState> => {
  let generation = 0;
  let cursor = 0;
  let activeUnlisten: WorkbenchRuntimeUnlisten | null = null;
  let startPromise: Promise<void> | null = null;
  let resyncPromise: Promise<void> | null = null;
  let resyncRequested = false;
  let bufferOverflowed = false;
  const bufferedEvents = new Map<number, WorkbenchRuntimeEvent>();
  const storeRef: { current: StoreApi<WorkbenchRuntimeStoreState> | null } = {
    current: null,
  };

  const getStore = (): StoreApi<WorkbenchRuntimeStoreState> => {
    if (!storeRef.current) throw new Error('workbench_runtime_store_uninitialized');
    return storeRef.current;
  };

  const setState = (state: Partial<WorkbenchRuntimeStoreState>): void => {
    getStore().setState(state);
  };

  const fail = (code: string, expectedGeneration: number): void => {
    if (generation !== expectedGeneration) return;
    activeUnlisten?.();
    activeUnlisten = null;
    resyncRequested = false;
    bufferOverflowed = false;
    bufferedEvents.clear();
    setState({
      syncStatus: 'failed',
      snapshot: null,
      lastEvent: null,
      stableErrorCode: code,
    });
  };

  const bufferEvent = (event: WorkbenchRuntimeEvent): void => {
    if (bufferedEvents.has(event.sequence)) return;
    if (bufferedEvents.size >= EVENT_BUFFER_LIMIT) {
      bufferOverflowed = true;
      const oldest = bufferedEvents.keys().next().value as number | undefined;
      if (oldest !== undefined) bufferedEvents.delete(oldest);
    }
    bufferedEvents.set(event.sequence, event);
  };

  const discardCoveredEvents = (): void => {
    for (const sequence of bufferedEvents.keys()) {
      if (sequence <= cursor) bufferedEvents.delete(sequence);
    }
  };

  const drainContinuousEvents = (): boolean => {
    discardCoveredEvents();
    let latest: WorkbenchRuntimeEvent | null = null;
    while (bufferedEvents.has(cursor + 1)) {
      latest = bufferedEvents.get(cursor + 1) ?? null;
      bufferedEvents.delete(cursor + 1);
      cursor += 1;
    }
    if (latest) setState({ lastEvent: latest });
    return latest !== null;
  };

  const readValidatedSnapshot = async (): Promise<WorkbenchRuntimeSnapshot> => {
    const next: unknown = await client.readSnapshot();
    return sanitizeSnapshot(next);
  };

  const runResync = async (expectedGeneration: number): Promise<void> => {
    let readsWithoutProgress = 0;
    try {
      do {
        resyncRequested = false;
        const previousCursor = cursor;
        const next = await readValidatedSnapshot();
        if (generation !== expectedGeneration) return;
        if (next.lastSequence < cursor) {
          readsWithoutProgress += 1;
          resyncRequested = true;
          continue;
        }

        cursor = next.lastSequence;
        setState({ snapshot: next, stableErrorCode: null });
        discardCoveredEvents();
        const acceptedNewerEvents = drainContinuousEvents();
        resyncRequested = acceptedNewerEvents
          || bufferedEvents.size > 0
          || bufferOverflowed;
        bufferOverflowed = false;
        if (!resyncRequested || cursor > previousCursor) {
          readsWithoutProgress = 0;
        } else {
          readsWithoutProgress += 1;
        }
      } while (
        resyncRequested
        && readsWithoutProgress < MAX_RESYNC_READS_WITHOUT_PROGRESS
        && generation === expectedGeneration
      );

      if (readsWithoutProgress >= MAX_RESYNC_READS_WITHOUT_PROGRESS) {
        const code = bufferedEvents.size > 0 || bufferOverflowed
          ? 'runtime_event_gap'
          : 'runtime_snapshot_stale';
        fail(code, expectedGeneration);
        return;
      }
      if (generation === expectedGeneration) setState({ syncStatus: 'ready' });
    } catch (error) {
      const code = error instanceof Error && error.message === 'runtime_contract_mismatch'
        ? 'runtime_contract_mismatch'
        : 'runtime_transport_unavailable';
      fail(code, expectedGeneration);
    }
  };

  const scheduleResync = (expectedGeneration: number): void => {
    if (generation !== expectedGeneration) return;
    if (resyncPromise) {
      resyncRequested = true;
      return;
    }
    setState({ syncStatus: 'resyncing' });
    const pendingResync = runResync(expectedGeneration);
    resyncPromise = pendingResync;
    void pendingResync.finally(() => {
      if (resyncPromise !== pendingResync) return;
      resyncPromise = null;
      if (resyncRequested && generation === expectedGeneration) {
        scheduleResync(expectedGeneration);
      }
    });
  };

  const onEvent = (expectedGeneration: number, event: WorkbenchRuntimeEvent): void => {
    if (generation !== expectedGeneration) return;
    const sanitizedEvent = sanitizeEvent(event);
    if (!sanitizedEvent || sanitizedEvent.sequence <= cursor) return;
    bufferEvent(sanitizedEvent);
    if (getStore().getState().syncStatus === 'bootstrapping') return;

    const accepted = drainContinuousEvents();
    if (accepted || bufferedEvents.size > 0 || bufferOverflowed) {
      scheduleResync(expectedGeneration);
    }
  };

  const runStart = async (expectedGeneration: number): Promise<void> => {
    try {
      const rawUnlisten = await client.subscribe(event => onEvent(expectedGeneration, event));
      const localUnlisten = once(rawUnlisten);
      if (generation !== expectedGeneration) {
        localUnlisten();
        return;
      }
      activeUnlisten = localUnlisten;

      const initial = await readValidatedSnapshot();
      if (generation !== expectedGeneration) return;
      cursor = initial.lastSequence;
      setState({ snapshot: initial, stableErrorCode: null });
      discardCoveredEvents();
      const accepted = drainContinuousEvents();
      if (accepted || bufferedEvents.size > 0 || bufferOverflowed) {
        scheduleResync(expectedGeneration);
      } else {
        setState({ syncStatus: 'ready' });
      }
    } catch (error) {
      const code = error instanceof Error && error.message === 'runtime_contract_mismatch'
        ? 'runtime_contract_mismatch'
        : 'runtime_transport_unavailable';
      fail(code, expectedGeneration);
    }
  };

  const start = (): Promise<void> => {
    if (startPromise) return startPromise;
    if (activeUnlisten && getStore().getState().syncStatus !== 'failed') return Promise.resolve();

    const expectedGeneration = generation + 1;
    generation = expectedGeneration;
    cursor = 0;
    bufferedEvents.clear();
    bufferOverflowed = false;
    resyncRequested = false;
    setState({
      syncStatus: 'bootstrapping',
      snapshot: null,
      lastEvent: null,
      stableErrorCode: null,
    });
    const pendingStart = runStart(expectedGeneration);
    startPromise = pendingStart;
    void pendingStart.finally(() => {
      if (startPromise === pendingStart) startPromise = null;
    });
    return pendingStart;
  };

  const stop = (): void => {
    generation += 1;
    activeUnlisten?.();
    activeUnlisten = null;
    startPromise = null;
    resyncPromise = null;
    resyncRequested = false;
    bufferOverflowed = false;
    bufferedEvents.clear();
    cursor = 0;
    setState({
      syncStatus: 'idle',
      snapshot: null,
      lastEvent: null,
      stableErrorCode: null,
    });
  };

  storeRef.current = createStore<WorkbenchRuntimeStoreState>(() => ({
    syncStatus: 'idle',
    snapshot: null,
    lastEvent: null,
    stableErrorCode: null,
    start,
    stop,
    submitIntent: request => client.submitIntent(request),
  }));

  return getStore();
};

export const workbenchRuntimeStore = createWorkbenchRuntimeStore(
  createWorkbenchRuntimeClient(createTauriWorkbenchRuntimeTransport()),
);
