import type { WorkbenchRuntimeStoreState } from './store';
import type {
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
