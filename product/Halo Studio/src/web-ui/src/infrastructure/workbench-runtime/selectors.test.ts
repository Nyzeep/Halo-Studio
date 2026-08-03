import { describe, expect, it } from 'vitest';

import type { WorkbenchRuntimeStoreState } from './store';
import {
  selectWorkbenchRuntimeConnected,
  selectWorkbenchRuntimeErrorMessageKey,
  selectWorkbenchRuntimePhase,
  selectWorkbenchRuntimePhaseMessageKey,
  selectWorkbenchRuntimeSessionNeedsDecision,
  selectWorkbenchRuntimeSessionPhaseMessageKey,
  selectWorkbenchRuntimeSessionsForWorkspace,
} from './selectors';
import type { WorkbenchRuntimeSnapshot } from './types';

const snapshot = (phase: WorkbenchRuntimeSnapshot['phase']): WorkbenchRuntimeSnapshot => ({
  schemaVersion: 1,
  phase,
  adapter: { identity: 'pi-rpc-p0', available: phase === 'ready' },
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

  it('recognizes only an awaiting decision as a confirmation request', () => {
    const runtimeState = state({
      ...snapshot('ready'),
      pendingOperations: [{
        operationId: 'operation-1',
        sessionId: 'session-1',
        kind: 'permission',
        phase: 'decisionSubmitted',
      }],
    });

    expect(selectWorkbenchRuntimeSessionNeedsDecision(runtimeState, 'session-1')).toBe(false);

    runtimeState.snapshot!.pendingOperations[0].phase = 'awaitingDecision';
    expect(selectWorkbenchRuntimeSessionNeedsDecision(runtimeState, 'session-1')).toBe(true);
    expect(selectWorkbenchRuntimeSessionNeedsDecision(runtimeState, 'session-2')).toBe(false);
  });

  it('maps public runtime phases and failures to semantic i18n keys', () => {
    const readyState = state(snapshot('ready'));
    expect(selectWorkbenchRuntimePhaseMessageKey(readyState)).toBe(
      'nav.sessions.workbenchRuntime.runtimePhase.ready',
    );
    expect(selectWorkbenchRuntimePhaseMessageKey(state(null))).toBe(
      'nav.sessions.workbenchRuntime.runtimePhase.disconnected',
    );
    expect(selectWorkbenchRuntimeErrorMessageKey({
      ...readyState,
      stableErrorCode: 'pi_protocol_error',
    })).toBe('nav.sessions.workbenchRuntime.error');
    expect(selectWorkbenchRuntimeSessionPhaseMessageKey('waitingDeveloper')).toBe(
      'nav.sessions.workbenchRuntime.sessionPhase.waitingDeveloper',
    );
    expect(selectWorkbenchRuntimeSessionPhaseMessageKey('interrupted')).toBe(
      'nav.sessions.workbenchRuntime.sessionPhase.interrupted',
    );
  });
});

