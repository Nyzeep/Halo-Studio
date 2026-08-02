// @vitest-environment jsdom

import React, { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import WorkbenchSessionsSection from './WorkbenchSessionsSection';

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

const runtimeStore = vi.hoisted(() => {
  let state: Record<string, unknown>;
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    getInitialState: () => state,
    setState: (next: Record<string, unknown>) => {
      state = { ...state, ...next };
      listeners.forEach(listener => listener());
    },
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
});

vi.mock('@/infrastructure/i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('@/infrastructure/workbench-runtime', () => ({
  selectWorkbenchRuntimeSessionsForWorkspace: (
    state: typeof runtimeStore.getState extends () => infer T ? T : never,
    workspaceId: string,
  ) => state.snapshot?.workspace?.workspaceId === workspaceId ? state.snapshot.sessions : [],
  selectWorkbenchRuntimeSessionNeedsDecision: (
    state: typeof runtimeStore.getState extends () => infer T ? T : never,
    sessionId: string,
  ) => state.snapshot?.pendingOperations?.some(
    (operation: { sessionId: string; phase: string }) => (
      operation.sessionId === sessionId && operation.phase === 'awaitingDecision'
    ),
  ) ?? false,
  workbenchRuntimeStore: runtimeStore,
}));

const makeState = (operationPhase: 'awaitingDecision' | 'decisionSubmitted') => ({
  syncStatus: 'ready',
  snapshot: {
    workspace: { workspaceId: 'workspace-1' },
    sessions: [{ sessionId: 'session-1', mode: 'managed', phase: 'waitingDeveloper' }],
    pendingOperations: [{
      operationId: 'operation-1',
      sessionId: 'session-1',
      kind: 'permission',
      phase: operationPhase,
    }],
  },
  lastEvent: null,
  stableErrorCode: null,
});

describe('WorkbenchSessionsSection', () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  async function renderWithOperationPhase(
    operationPhase: 'awaitingDecision' | 'decisionSubmitted',
  ): Promise<void> {
    runtimeStore.setState(makeState(operationPhase));
    await act(async () => {
      root.render(<WorkbenchSessionsSection workspaceId="workspace-1" />);
    });
  }

  it('shows the confirmation badge only while a decision is awaiting', async () => {
    await renderWithOperationPhase('decisionSubmitted');
    expect(container.querySelector('[data-testid="workbench-session-item"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="workbench-session-item"]')?.textContent)
      .not.toContain('nav.sessions.badgeNeedsConfirm');

    await renderWithOperationPhase('awaitingDecision');
    expect(container.querySelector('[data-testid="workbench-session-item"]')?.textContent)
      .toContain('nav.sessions.badgeNeedsConfirm');
  });
});
