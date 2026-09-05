/**
 * WorkbenchRuntimeStore projection tests (M5, issue #57).
 *
 * Covers the ingest reducer vocabulary and — critically — the niri strip
 * invariant: taskCreated inserts right of the anchor and NEVER reorders the
 * existing columns.
 */

import { beforeEach, describe, expect, it } from 'vitest';

import type { WorkbenchProjectionEvent, WorkbenchTask, WorkbenchWorkspace } from './workbenchProjectionTypes';
import {
  createWorkbenchRuntimeStore,
  type WorkbenchRuntimeStore,
} from './workbenchRuntimeStore';

let store: WorkbenchRuntimeStore;

beforeEach(() => {
  store = createWorkbenchRuntimeStore();
});

let seq = 0;
function emit(event: Omit<WorkbenchProjectionEvent, 'sequence'>): void {
  seq += 1;
  store.getState().ingestEvent({ ...event, sequence: seq } as WorkbenchProjectionEvent);
}

function makeTask(id: string, overrides: Partial<WorkbenchTask> = {}): WorkbenchTask {
  return {
    taskId: id,
    sessionId: `session-${id}`,
    title: `任务 ${id}`,
    executor: 'pi-rpc',
    mode: 'managed',
    phase: 'running',
    messages: [],
    activities: [],
    pendingOperation: null,
    deliveryReview: null,
    error: null,
    updatedAtMs: 0,
    ...overrides,
  };
}

function emitWorkspaceOpened(
  workspaceId: string,
  tasks: WorkbenchTask[],
  overrides: Partial<WorkbenchWorkspace> = {},
): void {
  emit({
    occurredAtMs: 0,
    kind: 'workspaceOpened',
    summary: `workspace ${workspaceId}`,
    workspace: {
      workspaceId,
      displayName: workspaceId,
      rootPath: `D:/ws/${workspaceId}`,
      branch: 'main',
      trusted: true,
      gitRepository: true,
      taskOrder: tasks.map(task => task.taskId),
      tasks: Object.fromEntries(tasks.map(task => [task.taskId, task])),
      ...overrides,
    },
  });
}

function orderOf(workspaceId: string): string[] {
  return store.getState().workspaces[workspaceId].taskOrder;
}

describe('WorkbenchRuntimeStore projection', () => {
  it('appends opened workspaces to the rail order', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1')]);
    emitWorkspaceOpened('ws-b', []);
    expect(store.getState().workspaceOrder).toEqual(['ws-a', 'ws-b']);
  });

  it('inserts a new task right of the anchor without reordering existing columns', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1'), makeTask('t2'), makeTask('t3')]);
    emit({
      occurredAtMs: 1,
      kind: 'taskCreated',
      summary: 'created',
      workspaceId: 'ws-a',
      task: makeTask('t-new'),
      insertAfterTaskId: 't1',
    });
    expect(orderOf('ws-a')).toEqual(['t1', 't-new', 't2', 't3']);
  });

  it('appends a task at the right edge when there is no anchor', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1')]);
    emit({
      occurredAtMs: 1,
      kind: 'taskCreated',
      summary: 'created',
      workspaceId: 'ws-a',
      task: makeTask('t-new'),
      insertAfterTaskId: null,
    });
    expect(orderOf('ws-a')).toEqual(['t1', 't-new']);
  });

  it('ignores duplicate taskCreated events (idempotent strip)', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1')]);
    const task = makeTask('t-dup');
    emit({ occurredAtMs: 1, kind: 'taskCreated', summary: 'a', workspaceId: 'ws-a', task, insertAfterTaskId: null });
    emit({ occurredAtMs: 2, kind: 'taskCreated', summary: 'b', workspaceId: 'ws-a', task, insertAfterTaskId: null });
    expect(orderOf('ws-a')).toEqual(['t1', 't-dup']);
  });

  it('appends session messages and upserts activities onto the projected task', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1', {
      activities: [{ activityId: 'act-1', kind: 'tool', label: '扫描', status: 'started', isError: false }],
    })]);
    emit({
      occurredAtMs: 1, kind: 'sessionMessageAppended', summary: 'msg', workspaceId: 'ws-a', taskId: 't1',
      message: { role: 'assistant', content: 'hello' },
    });
    emit({
      occurredAtMs: 2, kind: 'sessionActivityUpdated', summary: 'act', workspaceId: 'ws-a', taskId: 't1',
      activity: { activityId: 'act-1', kind: 'tool', label: '扫描', status: 'completed', isError: false },
    });
    const task = store.getState().workspaces['ws-a'].tasks['t1'];
    expect(task.messages).toEqual([{ role: 'assistant', content: 'hello' }]);
    expect(task.activities).toHaveLength(1);
    expect(task.activities[0].status).toBe('completed');
  });

  it('only clears a pending operation when the resolution matches its id', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1', {
      pendingOperation: {
        operationId: 'op-1', taskId: 't1', sessionId: 'session-t1', kind: 'permission',
        phase: 'awaitingDecision', toolName: '编辑 x.ts', arguments: '-', riskLevel: 'standard',
      },
    })]);
    emit({
      occurredAtMs: 1, kind: 'operationResolved', summary: 'stale', workspaceId: 'ws-a',
      taskId: 't1', operationId: 'op-other', decision: 'allowOnce',
    });
    expect(store.getState().workspaces['ws-a'].tasks['t1'].pendingOperation?.operationId).toBe('op-1');
    emit({
      occurredAtMs: 2, kind: 'operationResolved', summary: 'match', workspaceId: 'ws-a',
      taskId: 't1', operationId: 'op-1', decision: 'allowOnce',
    });
    expect(store.getState().workspaces['ws-a'].tasks['t1'].pendingOperation).toBeNull();
  });

  it('records delivery decisions onto the projected review', () => {
    emitWorkspaceOpened('ws-a', [makeTask('t1', {
      phase: 'reviewing',
      deliveryReview: {
        evidence: {
          capturedAtMs: 0, head: '9f2c1ab', workingTreeFingerprint: 'f'.repeat(64),
          changedFiles: ['a.ts'], diffPreview: 'diff', attribution: [],
        },
        summary: 's', verificationResults: 'v', runConclusion: 'r', decision: null,
      },
    })]);
    emit({
      occurredAtMs: 1, kind: 'deliveryDecisionRecorded', summary: 'd', workspaceId: 'ws-a',
      taskId: 't1', decision: 'accepted',
    });
    expect(store.getState().workspaces['ws-a'].tasks['t1'].deliveryReview?.decision).toBe('accepted');
  });

  it('bounds the diagnostics event ring and keeps the freshest entries', () => {
    for (let index = 0; index < 120; index += 1) {
      store.getState().publishEvent({
        occurredAtMs: index,
        kind: 'runtimeStateChanged',
        summary: `tick ${index}`,
        runtimePhase: index === 119 ? 'ready' : 'probing',
      });
    }
    const { eventBuffer, lastSequence } = store.getState();
    expect(eventBuffer).toHaveLength(100);
    expect(lastSequence).toBe(120);
    expect(eventBuffer[eventBuffer.length - 1].sequence).toBe(120);
  });

  it('never rewinds the projection for stale sequences', () => {
    store.getState().publishEvent({
      occurredAtMs: 0, kind: 'runtimeStateChanged', summary: 'ready', runtimePhase: 'ready',
    });
    const before = store.getState();
    store.getState().ingestEvent({
      ...({ occurredAtMs: 0, kind: 'runtimeStateChanged', summary: 'stale', runtimePhase: 'failed' } as Omit<WorkbenchProjectionEvent, 'sequence'>),
      sequence: 1,
    } as WorkbenchProjectionEvent);
    expect(store.getState().runtimePhase).toBe(before.runtimePhase);
    expect(store.getState().eventBuffer).toHaveLength(before.eventBuffer.length);
  });
});
