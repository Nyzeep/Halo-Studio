import { describe, expect, it } from 'vitest';

import type { WorkbenchRuntimeStoreState } from './store';
import {
  selectWorkbenchRuntimeConnected,
  selectWorkbenchRuntimePhase,
  selectWorkbenchRuntimeSessionsForWorkspace,
} from './selectors';
import type { WorkbenchRuntimeSnapshot } from './types';

const snapshot = (phase: WorkbenchRuntimeSnapshot['phase']): WorkbenchRuntimeSnapshot => ({
  schemaVersion: 1,
  phase,
  adapter: { identity: 'pi-rpc', available: phase === 'ready' },
  workspace: {
    workspaceId: 'workspace-1',
    displayName: 'Halo Studio',
    rootPath: 'D:\\workspace',
    trusted: true,
    gitRepository: true,
  },
  sessions: [{ sessionId: 'session-1', mode: 'standard', phase: 'idle' }],
  pendingOperations: [],
  lastSequence: 1,
  stateVersion: 1,
  error: null,
});

const state = (runtimeSnapshot: WorkbenchRuntimeSnapshot | null): WorkbenchRuntimeStoreState => ({
  syncStatus: runtimeSnapshot ? 'ready' : 'idle',
  snapshot: runtimeSnapshot,
  lastEvent: null,
  stableErrorCode: null,
  start: async () => undefined,
  stop: () => undefined,
  submitIntent: async request => ({
    requestId: request.requestId,
    stateVersion: runtimeSnapshot?.stateVersion ?? 0,
    sessionId: null,
  }),
});

describe('workbench runtime selectors', () => {
  it('derives connection state only from the authoritative runtime snapshot', () => {
    expect(selectWorkbenchRuntimeConnected(state(snapshot('ready')))).toBe(true);
    expect(selectWorkbenchRuntimeConnected(state(snapshot('starting')))).toBe(false);
    expect(selectWorkbenchRuntimePhase(state(null))).toBe('disconnected');
  });

  it('does not project active-workspace sessions into another workspace', () => {
    const runtimeState = state(snapshot('ready'));

    expect(selectWorkbenchRuntimeSessionsForWorkspace(runtimeState, 'workspace-1'))
      .toEqual([{ sessionId: 'session-1', mode: 'standard', phase: 'idle' }]);
    expect(selectWorkbenchRuntimeSessionsForWorkspace(runtimeState, 'workspace-2')).toEqual([]);
  });
});
