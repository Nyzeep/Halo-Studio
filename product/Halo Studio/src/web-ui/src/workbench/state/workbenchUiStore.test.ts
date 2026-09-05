/**
 * WorkbenchUiStore tests (M5, issue #57 acceptance #4).
 *
 * The dual-store boundary is asserted three ways:
 *   1. compile-time — WORKBENCH_UI_STORE_BOUNDARY (workbenchUiBoundary.ts);
 *   2. source-level — the module imports no fact-bearing module;
 *   3. runtime — the serialized state carries no fact/credential/evidence key.
 */

import { describe, expect, it } from 'vitest';

import { WORKBENCH_UI_STORE_BOUNDARY } from './workbenchUiBoundary';
import {
  buildInitialUiState,
  createWorkbenchUiStore,
} from './workbenchUiStore';
import uiStoreSource from './workbenchUiStore?raw';

describe('WorkbenchUiStore boundary (UI store carries no facts)', () => {
  it('passes the compile-time boundary assertion', () => {
    expect(WORKBENCH_UI_STORE_BOUNDARY).toBe(true);
  });

  it('imports nothing from the runtime projection surface', () => {
    expect(uiStoreSource).not.toMatch(/from\s+'[^']*(workbenchProjectionTypes|workbenchRuntimeStore|workbenchRuntimeMockDriver|workbench-runtime|workbenchOrdering)[^']*'/);
  });

  it('serializes without any fact/credential/evidence key', () => {
    const store = createWorkbenchUiStore();
    store.setState({
      activeWorkspaceId: 'ws-a',
      focusedTaskIdByWorkspace: { 'ws-a': 'task-1' },
      stripScrollLeftByWorkspace: { 'ws-a': 120 },
      overviewPageByWorkspace: { 'ws-a': 1 },
    });
    const serialized = JSON.stringify(store.getState());
    const factMarkers = [
      'messages',
      'activities',
      'pendingOperation',
      'deliveryReview',
      'evidence',
      'workingTreeFingerprint',
      'diffPreview',
      'credential',
      'taskOrder',
      'tasks',
      'eventBuffer',
    ];
    for (const marker of factMarkers) {
      expect(serialized).not.toContain(`"${marker}"`);
    }
  });
});

describe('WorkbenchUiStore behaviour', () => {
  it('closes the overview when switching the active workspace', () => {
    const store = createWorkbenchUiStore();
    store.getState().setOverviewOpen(true);
    store.getState().setActiveWorkspace('ws-b');
    expect(store.getState().activeWorkspaceId).toBe('ws-b');
    expect(store.getState().overviewOpen).toBe(false);
  });

  it('keeps the focus cursor per workspace', () => {
    const store = createWorkbenchUiStore();
    store.getState().setFocusedTask('ws-a', 'task-1');
    store.getState().setFocusedTask('ws-b', 'task-2');
    expect(store.getState().focusedTaskIdByWorkspace['ws-a']).toBe('task-1');
    expect(store.getState().focusedTaskIdByWorkspace['ws-b']).toBe('task-2');
  });

  it('stores the gesture transient scroll offset per workspace', () => {
    const store = createWorkbenchUiStore();
    store.getState().setStripScrollLeft('ws-a', 240);
    expect(store.getState().stripScrollLeftByWorkspace['ws-a']).toBe(240);
  });

  it('tracks overview selection and per-workspace pagination cursors', () => {
    const store = createWorkbenchUiStore();
    store.getState().setOverviewSelection('task-9');
    store.getState().setOverviewPage('ws-a', 1);
    expect(store.getState().overviewSelectedTaskId).toBe('task-9');
    expect(store.getState().overviewPageByWorkspace['ws-a']).toBe(1);
  });

  it('starts empty (no workspace, no focus, everything closed)', () => {
    const initial = buildInitialUiState();
    expect(initial.activeWorkspaceId).toBeNull();
    expect(initial.overviewOpen).toBe(false);
    expect(initial.paletteOpen).toBe(false);
    expect(initial.openSurface).toBe('none');
  });
});
