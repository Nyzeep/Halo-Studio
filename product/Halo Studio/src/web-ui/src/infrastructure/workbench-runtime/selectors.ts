import type { WorkbenchRuntimeStoreState } from './store';
import type {
  WorkbenchRuntimeCancellationMode,
  WorkbenchRuntimePhase,
  WorkbenchRuntimeSession,
} from './types';

const EMPTY_WORKBENCH_SESSIONS: WorkbenchRuntimeSession[] = [];

const RUNTIME_PHASE_MESSAGE_KEYS: Record<WorkbenchRuntimePhase, string> = {
  disconnected: 'nav.sessions.workbenchRuntime.runtimePhase.disconnected',
  probing: 'nav.sessions.workbenchRuntime.runtimePhase.probing',
  starting: 'nav.sessions.workbenchRuntime.runtimePhase.starting',
  ready: 'nav.sessions.workbenchRuntime.runtimePhase.ready',
  failed: 'nav.sessions.workbenchRuntime.runtimePhase.failed',
  stopping: 'nav.sessions.workbenchRuntime.runtimePhase.stopping',
};

const SESSION_PHASE_MESSAGE_KEYS: Record<WorkbenchRuntimeSession['phase'], string> = {
  creating: 'nav.sessions.workbenchRuntime.sessionPhase.creating',
  idle: 'nav.sessions.workbenchRuntime.sessionPhase.idle',
  running: 'nav.sessions.workbenchRuntime.sessionPhase.running',
  waitingDeveloper: 'nav.sessions.workbenchRuntime.sessionPhase.waitingDeveloper',
  reviewing: 'nav.sessions.workbenchRuntime.sessionPhase.reviewing',
  interrupted: 'nav.sessions.workbenchRuntime.sessionPhase.interrupted',
  stopping: 'nav.sessions.workbenchRuntime.sessionPhase.stopping',
  ended: 'nav.sessions.workbenchRuntime.sessionPhase.ended',
  failed: 'nav.sessions.workbenchRuntime.sessionPhase.failed',
};

type WorkbenchInterruptionReason = WorkbenchRuntimeCancellationMode
  | 'applicationInterrupted'
  | 'workspaceClosed'
  | 'runtimeShutdown'
  | 'runtimeFailure'
  | 'piTransportUnavailable'
  | 'piProtocolError'
  | 'eventStreamInterrupted'
  | 'unknown';

const INTERRUPTION_REASON_MESSAGE_KEYS: Record<WorkbenchInterruptionReason, string> = {
  native: 'nav.sessions.workbenchRuntime.interruptionReason.native',
  forced: 'nav.sessions.workbenchRuntime.interruptionReason.forced',
  applicationInterrupted: 'nav.sessions.workbenchRuntime.interruptionReason.applicationInterrupted',
  workspaceClosed: 'nav.sessions.workbenchRuntime.interruptionReason.workspaceClosed',
  runtimeShutdown: 'nav.sessions.workbenchRuntime.interruptionReason.runtimeShutdown',
  runtimeFailure: 'nav.sessions.workbenchRuntime.interruptionReason.runtimeFailure',
  piTransportUnavailable: 'nav.sessions.workbenchRuntime.interruptionReason.piTransportUnavailable',
  piProtocolError: 'nav.sessions.workbenchRuntime.interruptionReason.piProtocolError',
  eventStreamInterrupted: 'nav.sessions.workbenchRuntime.interruptionReason.eventStreamInterrupted',
  unknown: 'nav.sessions.workbenchRuntime.interruptionReason.unknown',
};

const INTERRUPTION_ERROR_REASONS: Record<string, WorkbenchInterruptionReason> = {
  runtime_shutdown: 'runtimeShutdown',
  runtime_internal: 'runtimeFailure',
  pi_transport_unavailable: 'piTransportUnavailable',
  pi_protocol_error: 'piProtocolError',
  adapter_event_gap: 'eventStreamInterrupted',
  adapter_event_stream_closed: 'eventStreamInterrupted',
};

const interruptionReason = (session: WorkbenchRuntimeSession): WorkbenchInterruptionReason => {
  if (session.cancellationMode) return session.cancellationMode;
  if (session.error?.code === 'application_interrupted') return 'applicationInterrupted';
  if (session.error?.code === 'workspace_closed') return 'workspaceClosed';
  return INTERRUPTION_ERROR_REASONS[session.error?.code ?? ''] ?? 'unknown';
};

export const selectWorkbenchRuntimeConnected = (
  state: WorkbenchRuntimeStoreState,
): boolean => state.snapshot?.phase === 'ready';

export const selectWorkbenchRuntimePhase = (
  state: WorkbenchRuntimeStoreState,
) => state.snapshot?.phase ?? 'disconnected';

export const selectWorkbenchRuntimePhaseMessageKey = (
  state: WorkbenchRuntimeStoreState,
): string => RUNTIME_PHASE_MESSAGE_KEYS[selectWorkbenchRuntimePhase(state)];

export const selectWorkbenchRuntimeSessionPhaseMessageKey = (
  phase: WorkbenchRuntimeSession['phase'],
): string => SESSION_PHASE_MESSAGE_KEYS[phase];

export const selectWorkbenchRuntimeInterruptionReasonMessageKey = (
  session: WorkbenchRuntimeSession,
): string => INTERRUPTION_REASON_MESSAGE_KEYS[interruptionReason(session)];

export const selectWorkbenchRuntimeErrorMessageKey = (
  _state: WorkbenchRuntimeStoreState,
): string => 'nav.sessions.workbenchRuntime.error';

export const selectWorkbenchRuntimeSessionsForWorkspace = (
  state: WorkbenchRuntimeStoreState,
  workspaceId: string,
): WorkbenchRuntimeSession[] => {
  if (state.snapshot?.workspace?.workspaceId !== workspaceId) return EMPTY_WORKBENCH_SESSIONS;
  return state.snapshot.sessions.filter(session => session.workspaceId === workspaceId);
};

export const selectWorkbenchRuntimeSessionNeedsDecision = (
  state: WorkbenchRuntimeStoreState,
  sessionId: string,
): boolean => state.snapshot?.pendingOperations.some(
  operation => operation.sessionId === sessionId && operation.phase === 'awaitingDecision',
) ?? false;
