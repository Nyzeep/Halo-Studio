// @vitest-environment jsdom

/**
 * WorkbenchShell interaction tests (M5, issue #57 acceptance).
 *
 * Covers the P0 keyboard set end-to-end against the real stores:
 *   - 新列在焦点右侧插入且既有列零重排（niri 第一原则）；
 *   - ←→ 焦点移动（不回绕、右端钳制）；
 *   - Overview：打开、←→ 选择跨页、分页指示器、Enter 跳回、Esc 退出；
 *   - 1..9 工作区切换、Esc 分层、⌘/Ctrl+K 命令面板。
 */

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import type { WorkbenchTask } from '../state/workbenchProjectionTypes';
import { resetWorkbenchRuntimeStoreForTests, useWorkbenchRuntimeStore } from '../state/workbenchRuntimeStore';
import { resetWorkbenchUiStoreForTests, useWorkbenchUiStore } from '../state/workbenchUiStore';
import { WorkbenchShell } from './WorkbenchShell';

function makeTask(id: string, overrides: Partial<WorkbenchTask> = {}): WorkbenchTask {
  return {
    taskId: id,
    sessionId: `session-${id}`,
    title: `任务 ${id}`,
    executor: 'pi-rpc',
    mode: 'managed',
    phase: 'running',
    messages: [{ role: 'user', content: `描述 ${id}` }],
    activities: [],
    pendingOperation: null,
    deliveryReview: null,
    error: null,
    updatedAtMs: 0,
    ...overrides,
  };
}

function openWorkspace(workspaceId: string, displayName: string, tasks: WorkbenchTask[]): void {
  useWorkbenchRuntimeStore.getState().publishEvent({
    occurredAtMs: 0,
    kind: 'workspaceOpened',
    summary: `workspace ${workspaceId}`,
    workspace: {
      workspaceId,
      displayName,
      rootPath: `D:/ws/${workspaceId}`,
      branch: 'main',
      trusted: true,
      gitRepository: true,
      taskOrder: tasks.map(task => task.taskId),
      tasks: Object.fromEntries(tasks.map(task => [task.taskId, task])),
    },
  });
}

let host: HTMLElement;
let root: Root | null = null;

beforeEach(() => {
  resetWorkbenchRuntimeStoreForTests();
  resetWorkbenchUiStoreForTests();
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement('div');
  document.body.appendChild(host);
});

afterEach(() => {
  if (root) {
    act(() => {
      root?.unmount();
    });
    root = null;
  }
  host.remove();
});

function renderShell(): void {
  root = createRoot(host);
  act(() => {
    root?.render(<WorkbenchShell />);
  });
}

function pressKey(key: string, init: KeyboardEventInit = {}): void {
  act(() => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }));
  });
}

function columnIds(): string[] {
  return Array.from(host.querySelectorAll('[data-testid="task-column"]'))
    .map(element => element.getAttribute('data-task-id'));
}

function focusedTaskId(): string | null {
  return host.querySelector('[data-testid="task-column"][data-focused="true"]')
    ?.getAttribute('data-task-id') ?? null;
}

function queryOne(testId: string): Element | null {
  return host.querySelector(`[data-testid="${testId}"]`);
}

describe('WorkbenchShell (strip workbench)', () => {
  it('auto-selects the first projected workspace and focuses its first task', () => {
    openWorkspace('ws-alpha', 'alpha', [makeTask('t1'), makeTask('t2'), makeTask('t3')]);
    renderShell();
    expect(useWorkbenchUiStore.getState().activeWorkspaceId).toBe('ws-alpha');
    expect(focusedTaskId()).toBe('t1');
    expect(columnIds()).toEqual(['t1', 't2', 't3']);
  });

  it('inserts a new column right of focus without reordering existing columns', () => {
    openWorkspace('ws-alpha', 'alpha', [makeTask('t1'), makeTask('t2'), makeTask('t3')]);
    renderShell();
    pressKey('n');
    const ids = columnIds();
    expect(ids).toHaveLength(4);
    expect(ids[0]).toBe('t1');
    expect(ids[1]).toMatch(/^task-local-/);
    expect(ids[2]).toBe('t2');
    expect(ids[3]).toBe('t3');
    expect(focusedTaskId()).toBe(ids[1]);
    // Existing columns keep their DOM position: no squeeze, no reorder.
    expect(host.querySelectorAll('[data-testid="task-column"]')[0].getAttribute('data-task-id')).toBe('t1');
    expect(host.querySelectorAll('[data-testid="task-column"]')[3].getAttribute('data-task-id')).toBe('t3');
  });

  it('moves focus with arrow keys and clamps at the strip edges (never wraps)', () => {
    openWorkspace('ws-alpha', 'alpha', [makeTask('t1'), makeTask('t2'), makeTask('t3')]);
    renderShell();
    pressKey('ArrowRight');
    expect(focusedTaskId()).toBe('t2');
    pressKey('ArrowRight');
    pressKey('ArrowRight');
    expect(focusedTaskId()).toBe('t3');
    pressKey('ArrowLeft');
    expect(focusedTaskId()).toBe('t2');
    pressKey('ArrowLeft');
    pressKey('ArrowLeft');
    expect(focusedTaskId()).toBe('t1');
  });

  it('paginates the overview at extreme column counts and jumps back with Enter', () => {
    const tasks = Array.from({ length: 10 }, (_, index) => makeTask(`t${index + 1}`));
    openWorkspace('ws-alpha', 'alpha', tasks);
    renderShell();
    pressKey('o');
    expect(queryOne('overview')).not.toBeNull();
    expect(queryOne('overview-page-indicator')?.textContent).toBe('第 1/2 页');
    expect(host.querySelectorAll('[data-testid="overview-cell"]')).toHaveLength(8);

    for (let index = 0; index < 8; index += 1) pressKey('ArrowRight');
    expect(queryOne('overview-page-indicator')?.textContent).toBe('第 2/2 页');
    const pageTwoCells = Array.from(host.querySelectorAll('[data-testid="overview-cell"]'))
      .map(element => element.getAttribute('data-task-id'));
    expect(pageTwoCells).toEqual(['t9', 't10']);

    pressKey('Enter');
    expect(queryOne('overview')).toBeNull();
    expect(useWorkbenchUiStore.getState().overviewOpen).toBe(false);
    expect(focusedTaskId()).toBe('t9');
  });

  it('closes layers with Escape in order and switches workspaces with 1..9', () => {
    openWorkspace('ws-alpha', 'alpha', [makeTask('t1'), makeTask('t2')]);
    openWorkspace('ws-beta', 'beta', [makeTask('t4')]);
    renderShell();

    pressKey('Escape');
    expect(queryOne('command-palette')).toBeNull();
    expect(queryOne('workbench-surface')).toBeNull();
    expect(queryOne('overview')).toBeNull();

    pressKey('2');
    expect(useWorkbenchUiStore.getState().activeWorkspaceId).toBe('ws-beta');
    expect(focusedTaskId()).toBe('t4');
    pressKey('1');
    expect(useWorkbenchUiStore.getState().activeWorkspaceId).toBe('ws-alpha');
    expect(focusedTaskId()).toBe('t1');

    pressKey('o');
    expect(queryOne('overview')).not.toBeNull();
    pressKey('Escape');
    expect(queryOne('overview')).toBeNull();
  });

  it('opens the command palette with Ctrl+K and closes it with Escape', () => {
    openWorkspace('ws-alpha', 'alpha', [makeTask('t1')]);
    renderShell();
    pressKey('k', { ctrlKey: true });
    expect(queryOne('command-palette')).not.toBeNull();
    const paletteItems = host.querySelectorAll('[data-testid="command-palette-item"]');
    expect(paletteItems.length).toBeGreaterThan(3);

    const paletteInput = host.querySelector<HTMLInputElement>('[data-testid="command-palette-input"]');
    expect(paletteInput).not.toBeNull();
    act(() => {
      paletteInput?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    });
    expect(queryOne('command-palette')).toBeNull();
  });

  it('creates a workspace from the rail 「新建」位 and shows the empty-strip hint', () => {
    openWorkspace('ws-alpha', 'alpha', [makeTask('t1')]);
    renderShell();
    const newSlot = host.querySelector<HTMLButtonElement>('[data-testid="workspace-rail-new"]');
    expect(newSlot).not.toBeNull();
    act(() => {
      newSlot?.click();
    });
    expect(useWorkbenchUiStore.getState().activeWorkspaceId).toMatch(/^ws-local-/);
    expect(queryOne('task-column')).toBeNull();
    expect(host.querySelector('[data-testid="task-strip"]')?.textContent).toContain('空工作区');
  });
});
