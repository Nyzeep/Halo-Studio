import { describe, expect, it } from 'vitest';

import type { WorkbenchRuntimeStoreState } from './store';
import {
  selectWorkbenchRuntimeConnected,
  selectWorkbenchRuntimeErrorMessageKey,
  selectWorkbenchRuntimeInterruptionReasonMessageKey,
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
  adapter: { identity: 'pi-rpc-p0', available: phase === 'ready', readiness: null },
  workspace: {
    workspaceId: 'workspace-1',
    displayName: 'Halo Studio',
    rootPath: 'D:\\workspace',
    trusted: true,
    gitRepository: true,
  },
  sessions: [{
    workspaceId: 'workspace-1',
    taskId: 'task-1',
    sessionId: 'session-1',
    mode: 'standard',
    phase: 'idle',
  }],
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
      .toEqual([{
        workspaceId: 'workspace-1',
        taskId: 'task-1',
        sessionId: 'session-1',
        mode: 'standard',
        phase: 'idle',
      }]);
    expect(selectWorkbenchRuntimeSessionsForWorkspace(runtimeState, 'workspace-2')).toEqual([]);
  });

  it('does not project a session whose Halo workspace binding disagrees with the snapshot', () => {
    const runtimeState = state({
      ...snapshot('ready'),
      sessions: [
        ...snapshot('ready').sessions,
        {
          workspaceId: 'workspace-2',
          taskId: 'task-2',
          sessionId: 'session-2',
          mode: 'managed',
          phase: 'idle',
        },
      ],
    });

    expect(selectWorkbenchRuntimeSessionsForWorkspace(runtimeState, 'workspace-1'))
      .toEqual([snapshot('ready').sessions[0]]);
  });

  it('recognizes only an awaiting decision as a confirmation request', () => {
    const runtimeState = state({
      ...snapshot('ready'),
      pendingOperations: [{
        operationId: 'operation-1',
        taskId: 'task-1',
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

  it('keeps known interruption facts visible without exposing the error summary', () => {
    const interrupted = {
      ...snapshot('disconnected').sessions[0],
      mode: 'managed' as const,
      phase: 'interrupted' as const,
      cancellationMode: null,
      error: {
        code: 'application_interrupted',
        recoveryAction: 'start_new_run_or_review_interruption',
        summary: 'untrusted raw runtime detail',
      },
    };

    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey(interrupted)).toBe(
      'nav.sessions.workbenchRuntime.interruptionReason.applicationInterrupted',
    );
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      error: { ...interrupted.error, code: 'workspace_closed' },
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.workspaceClosed');
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      cancellationMode: 'forced',
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.forced');
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      error: { ...interrupted.error, code: 'runtime_shutdown' },
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.runtimeShutdown');
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      error: { ...interrupted.error, code: 'runtime_internal' },
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.runtimeFailure');
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      error: { ...interrupted.error, code: 'pi_transport_unavailable' },
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.piTransportUnavailable');
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      error: { ...interrupted.error, code: 'pi_protocol_error' },
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.piProtocolError');
    expect(selectWorkbenchRuntimeInterruptionReasonMessageKey({
      ...interrupted,
      error: { ...interrupted.error, code: 'adapter_event_stream_closed' },
    })).toBe('nav.sessions.workbenchRuntime.interruptionReason.eventStreamInterrupted');
  });
});
