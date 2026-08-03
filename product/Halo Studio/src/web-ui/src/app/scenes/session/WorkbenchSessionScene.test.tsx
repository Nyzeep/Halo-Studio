// @vitest-environment jsdom

import React, { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import WorkbenchSessionScene from './WorkbenchSessionScene';

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

const runtimeStore = vi.hoisted(() => {
  let state: Record<string, unknown>;
  return {
    getState: () => state,
    getInitialState: () => state,
    setState: (next: Record<string, unknown>) => { state = next; },
    subscribe: () => () => undefined,
  };
});

vi.mock('@/infrastructure/i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('@/infrastructure/workbench-runtime', () => ({
  selectWorkbenchRuntimePhaseMessageKey: () => 'nav.sessions.workbenchRuntime.runtimePhase.ready',
  selectWorkbenchRuntimeSessionPhaseMessageKey: () => (
    'nav.sessions.workbenchRuntime.sessionPhase.waitingDeveloper'
  ),
  selectWorkbenchRuntimeErrorMessageKey: () => 'nav.sessions.workbenchRuntime.error',
  workbenchRuntimeStore: runtimeStore,
}));

describe('WorkbenchSessionScene', () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    runtimeStore.setState({
      syncStatus: 'failed',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { displayName: 'Halo Studio' },
        sessions: [{ sessionId: 'session-1', phase: 'waitingDeveloper' }],
      },
      stableErrorCode: 'pi_protocol_error',
    });
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it('renders semantic i18n labels instead of internal phase and error codes', async () => {
    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.runtimePhase.ready');
    expect(container.textContent)
      .toContain('nav.sessions.workbenchRuntime.sessionPhase.waitingDeveloper');
    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.error');
    expect(container.textContent).not.toContain('pi_protocol_error');
  });
});
