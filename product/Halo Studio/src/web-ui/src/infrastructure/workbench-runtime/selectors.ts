import type { WorkbenchRuntimeStoreState } from './store';
import type { WorkbenchRuntimeSession } from './types';

const EMPTY_WORKBENCH_SESSIONS: WorkbenchRuntimeSession[] = [];

export const selectWorkbenchRuntimeConnected = (
  state: WorkbenchRuntimeStoreState,
): boolean => state.snapshot?.phase === 'ready';

export const selectWorkbenchRuntimePhase = (
  state: WorkbenchRuntimeStoreState,
) => state.snapshot?.phase ?? 'disconnected';

export const selectWorkbenchRuntimeSessionsForWorkspace = (
  state: WorkbenchRuntimeStoreState,
  workspaceId: string,
): WorkbenchRuntimeSession[] => {
  if (state.snapshot?.workspace?.workspaceId !== workspaceId) return EMPTY_WORKBENCH_SESSIONS;
  return state.snapshot.sessions;
};
