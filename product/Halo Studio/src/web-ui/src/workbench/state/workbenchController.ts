/**
 * Workbench interaction controller (M5, ADR-0076; issue #57).
 *
 * The seam between the two stores and the DOM: the UI store must not know
 * runtime facts and the runtime store must not know UI concerns, so every
 * cross-store interaction (keyboard focus movement, task creation anchored
 * right of focus, overview navigation, operation/delivery decisions) is
 * expressed here as a plain function. Components and tests call these
 * functions only — they never wire the stores together themselves.
 *
 * Command synthesis: until the real Tauri driver lands, local intents
 * (create task/workspace, resolve operation, delivery decision) are
 * synthesized as projection events via `publishEvent`, exactly the vocabulary
 * the Runtime will emit. This is a driver replacement later, not a rewrite.
 */

import type { WorkbenchExecutorId } from './workbenchExecutors';
import { getWorkbenchExecutor } from './workbenchExecutors';
import { moveStripFocus, resolveInsertionAnchor } from './workbenchOrdering';
import { createMockWorkbenchRuntimeDriver } from './workbenchRuntimeMockDriver';
import type { WorkbenchProjectionEventInput, WorkbenchTask } from './workbenchProjectionTypes';
import { useWorkbenchRuntimeStore } from './workbenchRuntimeStore';
import { useWorkbenchUiStore, type WorkbenchSurfaceId } from './workbenchUiStore';
import { WORKBENCH_LEGACY_VIEW_STORAGE_KEY, WORKBENCH_LEGACY_VIEW_VALUE } from '../workbenchGate';

/** Extreme-column-count pagination for Overview (P0, ADR-0076). */
export const OVERVIEW_PAGE_SIZE = 8;

const DEFAULT_EXECUTOR: WorkbenchExecutorId = 'pi-rpc';

let localTaskCounter = 0;
let localWorkspaceCounter = 0;

/** Starts the mock event stream once (the M5 driver; Tauri replaces it later). */
export function ensureRuntimeStarted(): void {
  const runtime = useWorkbenchRuntimeStore.getState();
  if (runtime.runtimePhase !== 'disconnected') return;
  runtime.startDriver(createMockWorkbenchRuntimeDriver({ simulate: true }));
}

function currentWorkspace(): { workspaceId: string; order: readonly string[] } | null {
  const { activeWorkspaceId } = useWorkbenchUiStore.getState();
  if (!activeWorkspaceId) return null;
  const runtime = useWorkbenchRuntimeStore.getState();
  const workspace = runtime.workspaces[activeWorkspaceId];
  if (!workspace) return null;
  return { workspaceId: activeWorkspaceId, order: workspace.taskOrder };
}

/** Guarantees the focus cursor points at an existing task (first task default). */
export function ensureWorkspaceFocus(workspaceId: string): void {
  const ui = useWorkbenchUiStore.getState();
  const runtime = useWorkbenchRuntimeStore.getState();
  const workspace = runtime.workspaces[workspaceId];
  const focused = ui.focusedTaskIdByWorkspace[workspaceId];
  if (focused !== undefined && (focused === null || workspace?.tasks[focused])) return;
  ui.setFocusedTask(workspaceId, workspace ? workspace.taskOrder[0] ?? null : null);
}

export function moveFocus(delta: 1 | -1): void {
  const workspace = currentWorkspace();
  if (!workspace) return;
  const ui = useWorkbenchUiStore.getState();
  const focused = ui.focusedTaskIdByWorkspace[workspace.workspaceId] ?? null;
  const next = moveStripFocus(workspace.order, focused, delta);
  if (next === null || next === focused) return;
  ui.setFocusedTask(workspace.workspaceId, next);
}

export function createTaskAtFocus(input: { title?: string; executor?: WorkbenchExecutorId } = {}): string | null {
  const workspace = currentWorkspace();
  if (!workspace) return null;
  const runtime = useWorkbenchRuntimeStore.getState();
  const ui = useWorkbenchUiStore.getState();
  const executor = getWorkbenchExecutor(input.executor ?? DEFAULT_EXECUTOR);
  const focusedTaskId = ui.focusedTaskIdByWorkspace[workspace.workspaceId] ?? null;
  const anchor = resolveInsertionAnchor(workspace.order, focusedTaskId);
  const title = input.title?.trim() || `新任务 ${workspace.order.length + 1}`;
  const taskId = `task-local-${Date.now().toString(36)}-${localTaskCounter += 1}`;
  const task: WorkbenchTask = {
    taskId,
    sessionId: `session-${taskId}`,
    title,
    executor: executor.id,
    mode: 'managed',
    phase: 'running',
    messages: [{ role: 'user', content: title }],
    activities: [{
      activityId: `${taskId}-start`,
      kind: 'tool',
      label: '启动会话',
      status: 'started',
      isError: false,
    }],
    pendingOperation: null,
    deliveryReview: null,
    error: null,
    updatedAtMs: Date.now(),
  };
  runtime.publishEvent({
    occurredAtMs: Date.now(),
    kind: 'taskCreated',
    summary: `Workbench task "${title}" was created (${executor.displayName})`,
    workspaceId: workspace.workspaceId,
    task,
    insertAfterTaskId: anchor,
  });
  ui.setFocusedTask(workspace.workspaceId, taskId);
  return taskId;
}

/** Rail 「新建」位: niri 的「永远存在的空工作区」转译（mock 模式本地合成）。 */
export function createWorkspaceFromRail(): string | null {
  const runtime = useWorkbenchRuntimeStore.getState();
  const ui = useWorkbenchUiStore.getState();
  const workspaceId = `ws-local-${localWorkspaceCounter += 1}`;
  const displayName = `ws-${localWorkspaceCounter}`;
  runtime.publishEvent({
    occurredAtMs: Date.now(),
    kind: 'workspaceOpened',
    summary: `Workbench workspace ${displayName} was created`,
    workspace: {
      workspaceId,
      displayName,
      rootPath: '',
      branch: '（新工作区）',
      trusted: false,
      gitRepository: false,
      taskOrder: [],
      tasks: {},
    },
  });
  ui.setActiveWorkspace(workspaceId);
  ensureWorkspaceFocus(workspaceId);
  return workspaceId;
}

/** 1..9 rail jumps; no-op beyond the rail length (never wraps). */
export function selectWorkspaceByIndex(index: number): boolean {
  const runtime = useWorkbenchRuntimeStore.getState();
  const workspaceId = runtime.workspaceOrder[index];
  if (!workspaceId) return false;
  const ui = useWorkbenchUiStore.getState();
  ui.setActiveWorkspace(workspaceId);
  ensureWorkspaceFocus(workspaceId);
  return true;
}

/** Rail click / palette jump / Overview jump all land here. */
export function jumpToTask(workspaceId: string, taskId: string | null): void {
  const ui = useWorkbenchUiStore.getState();
  ui.setActiveWorkspace(workspaceId);
  ui.setFocusedTask(workspaceId, taskId);
}

export function toggleOverview(open?: boolean): void {
  const ui = useWorkbenchUiStore.getState();
  const next = open ?? !ui.overviewOpen;
  if (next === ui.overviewOpen) return;
  if (next) {
    const workspace = currentWorkspace();
    const focused = workspace
      ? ui.focusedTaskIdByWorkspace[workspace.workspaceId] ?? null
      : null;
    ui.setOverviewSelection(focused);
  }
  ui.setOverviewOpen(next);
}

export interface OverviewEntry {
  workspaceId: string;
  taskId: string;
}

export function buildOverviewEntries(): OverviewEntry[] {
  const runtime = useWorkbenchRuntimeStore.getState();
  const entries: OverviewEntry[] = [];
  for (const workspaceId of runtime.workspaceOrder) {
    const workspace = runtime.workspaces[workspaceId];
    if (!workspace) continue;
    for (const taskId of workspace.taskOrder) {
      entries.push({ workspaceId, taskId });
    }
  }
  return entries;
}

/** ←→ in Overview walk the flattened strip list; pages turn with the selection. */
export function moveOverviewSelection(delta: 1 | -1): void {
  const entries = buildOverviewEntries();
  if (entries.length === 0) return;
  const ui = useWorkbenchUiStore.getState();
  const currentIndex = ui.overviewSelectedTaskId
    ? entries.findIndex(entry => entry.taskId === ui.overviewSelectedTaskId)
    : -1;
  const nextIndex = currentIndex < 0
    ? (delta === 1 ? 0 : entries.length - 1)
    : Math.min(Math.max(currentIndex + delta, 0), entries.length - 1);
  const target = entries[nextIndex];
  ui.setOverviewSelection(target.taskId);
  const runtime = useWorkbenchRuntimeStore.getState();
  const order = runtime.workspaces[target.workspaceId]?.taskOrder ?? [];
  const page = Math.floor(Math.max(order.indexOf(target.taskId), 0) / OVERVIEW_PAGE_SIZE);
  if ((ui.overviewPageByWorkspace[target.workspaceId] ?? 0) !== page) {
    ui.setOverviewPage(target.workspaceId, page);
  }
}

/** Overview page turn (mouse path); keyboard turns pages via selection. */
export function moveOverviewPage(workspaceId: string, delta: 1 | -1): void {
  const runtime = useWorkbenchRuntimeStore.getState();
  const ui = useWorkbenchUiStore.getState();
  const order = runtime.workspaces[workspaceId]?.taskOrder ?? [];
  const pageCount = Math.max(1, Math.ceil(order.length / OVERVIEW_PAGE_SIZE));
  const current = ui.overviewPageByWorkspace[workspaceId] ?? 0;
  const next = Math.min(Math.max(current + delta, 0), pageCount - 1);
  if (next !== current) ui.setOverviewPage(workspaceId, next);
}

export function confirmOverviewSelection(): void {
  const ui = useWorkbenchUiStore.getState();
  if (!ui.overviewSelectedTaskId) return;
  const runtime = useWorkbenchRuntimeStore.getState();
  const owner = runtime.workspaceOrder.find(
    workspaceId => runtime.workspaces[workspaceId]?.taskOrder.includes(ui.overviewSelectedTaskId!),
  );
  if (owner) jumpToTask(owner, ui.overviewSelectedTaskId);
  ui.setOverviewOpen(false);
}

/** Esc peels layers: palette → side surface → overview. */
export function handleEscape(): boolean {
  const ui = useWorkbenchUiStore.getState();
  if (ui.paletteOpen) {
    ui.setPaletteOpen(false);
    return true;
  }
  if (ui.openSurface !== 'none') {
    ui.setOpenSurface('none');
    return true;
  }
  if (ui.overviewOpen) {
    ui.setOverviewOpen(false);
    return true;
  }
  return false;
}

export function openSurface(surface: Exclude<WorkbenchSurfaceId, 'none'>): void {
  useWorkbenchUiStore.getState().setOpenSurface(surface);
}

export function resolveOperation(workspaceId: string, taskId: string, decision: 'allowOnce' | 'deny'): void {
  const runtime = useWorkbenchRuntimeStore.getState();
  const task = runtime.workspaces[workspaceId]?.tasks[taskId];
  const operation = task?.pendingOperation;
  if (!operation) return;
  publish({
    occurredAtMs: Date.now(),
    kind: 'operationResolved',
    summary: `Workbench operation was resolved (${decision})`,
    workspaceId,
    taskId,
    operationId: operation.operationId,
    decision,
  });
}

export function recordDeliveryDecision(workspaceId: string, taskId: string, decision: 'accepted' | 'rejected'): void {
  publish({
    occurredAtMs: Date.now(),
    kind: 'deliveryDecisionRecorded',
    summary: `Workbench delivery decision recorded (${decision})`,
    workspaceId,
    taskId,
    decision,
  });
}

function publish(event: WorkbenchProjectionEventInput): void {
  useWorkbenchRuntimeStore.getState().publishEvent(event);
}

/** 旧视图回退：写 sessionStorage 标记后整页重载（AppLayout 条件挂载读取）。 */
export function switchToLegacyWorkbenchView(): void {
  try {
    window.sessionStorage.setItem(WORKBENCH_LEGACY_VIEW_STORAGE_KEY, WORKBENCH_LEGACY_VIEW_VALUE);
    window.location.reload();
  } catch {
    // Storage-less environments simply keep the new view.
  }
}
