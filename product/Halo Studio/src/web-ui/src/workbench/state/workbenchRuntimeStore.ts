/**
 * WorkbenchRuntimeStore — Runtime projection for the strip workbench
 * (M5, ADR-0075/0076/0080; issue #57).
 *
 * This store holds RUNTIME FACTS only: workspaces, tasks, sessions, fact
 * events. It has zero knowledge of focus, overview, palette or any other UI
 * concern — those live in workbenchUiStore.ts. The two stores are strictly
 * separated; workbenchUiBoundary.ts type-asserts the UI side never grows a
 * fact-bearing key, and nothing here imports the UI store.
 *
 * Every mutation enters through `ingestEvent` (or `publishEvent`, which stamps
 * the next sequence for locally synthesized events — the mock-mode stand-in
 * for the real Runtime's intent receipts).
 */

import { create } from 'zustand';

import type { WorkbenchRuntimePhase } from '@/infrastructure/workbench-runtime/types';

import { insertTaskIntoStrip } from './workbenchOrdering';

import {
  WORKBENCH_EVENT_BUFFER_LIMIT,
  type WorkbenchProjectionEvent,
  type WorkbenchProjectionEventInput,
  type WorkbenchRuntimeLinkStatus,
  type WorkbenchTask,
  type WorkbenchWorkspace,
} from './workbenchProjectionTypes';
import type { WorkbenchRuntimeDriver } from './workbenchRuntimeStoreDriver';

export type { WorkbenchProjectionEventInput };

const INITIAL_RUNTIME_PHASE: WorkbenchRuntimePhase = 'disconnected';

interface WorkbenchRuntimeFields {
  link: WorkbenchRuntimeLinkStatus;
  runtimePhase: WorkbenchRuntimePhase;
  lastSequence: number;
  workspaceOrder: string[];
  workspaces: Record<string, WorkbenchWorkspace>;
  eventBuffer: WorkbenchEventBufferEntry[];
}

function buildInitialState(): WorkbenchRuntimeFields {
  return {
    link: { kind: 'tauri-event-stream', connected: false },
    runtimePhase: INITIAL_RUNTIME_PHASE,
    lastSequence: 0,
    workspaceOrder: [],
    workspaces: {},
    eventBuffer: [],
  };
}

/** Bounded diagnostics ring entry (head is rendered by the shell HUD). */
export interface WorkbenchEventBufferEntry {
  sequence: number;
  occurredAtMs: number;
  kind: WorkbenchProjectionEvent['kind'];
  summary: string;
}

export interface WorkbenchRuntimeState {
  /** Transport behind the projection; flips to mock/tauri when a driver starts. */
  link: WorkbenchRuntimeLinkStatus;
  runtimePhase: WorkbenchRuntimePhase;
  lastSequence: number;
  /** Rail order — append-only (niri: 永远存在的空工作区排最右). */
  workspaceOrder: string[];
  workspaces: Record<string, WorkbenchWorkspace>;
  eventBuffer: WorkbenchEventBufferEntry[];
  ingestEvent: (event: WorkbenchProjectionEvent) => void;
  /** Stamps `sequence` and feeds ingestEvent (local intent synthesis seam). */
  publishEvent: (event: WorkbenchProjectionEventInput) => void;
  startDriver: (driver: WorkbenchRuntimeDriver) => void;
  stopDriver: () => void;
}

/**
 * Event inputs are typed by WorkbenchProjectionEventInput
 * (./workbenchProjectionTypes), re-exported above for controller ergonomics.
 */

function appendEventBuffer(
  buffer: WorkbenchEventBufferEntry[],
  event: WorkbenchProjectionEvent,
): WorkbenchEventBufferEntry[] {
  const next = [
    ...buffer,
    { sequence: event.sequence, occurredAtMs: event.occurredAtMs, kind: event.kind, summary: event.summary },
  ];
  return next.length > WORKBENCH_EVENT_BUFFER_LIMIT
    ? next.slice(next.length - WORKBENCH_EVENT_BUFFER_LIMIT)
    : next;
}

function reduceProjection(state: WorkbenchRuntimeFields, event: WorkbenchProjectionEvent): WorkbenchRuntimeFields {
  switch (event.kind) {
    case 'runtimeStateChanged':
      return {
        ...state,
        runtimePhase: event.runtimePhase,
        link: { ...state.link, connected: event.runtimePhase === 'ready' },
      };
    case 'workspaceOpened': {
      if (state.workspaces[event.workspace.workspaceId]) {
        // Workspace re-opened: replace the projection wholesale (facts are the
        // Runtime's word, not an incremental patch).
        return {
          ...state,
          workspaces: { ...state.workspaces, [event.workspace.workspaceId]: event.workspace },
        };
      }
      return {
        ...state,
        workspaceOrder: [...state.workspaceOrder, event.workspace.workspaceId],
        workspaces: { ...state.workspaces, [event.workspace.workspaceId]: event.workspace },
      };
    }
    case 'taskCreated': {
      const workspace = state.workspaces[event.workspaceId];
      if (!workspace || workspace.tasks[event.task.taskId]) {
        return state;
      }
      // Canonical niri strip primitive: insert right of the anchor (null =
      // append at the right edge). Never reorders existing columns.
      const taskOrder = insertTaskIntoStrip(
        workspace.taskOrder,
        event.task.taskId,
        event.insertAfterTaskId,
      );
      return {
        ...state,
        workspaces: {
          ...state.workspaces,
          [event.workspaceId]: {
            ...workspace,
            taskOrder: taskOrder,
            tasks: { ...workspace.tasks, [event.task.taskId]: event.task },
          },
        },
      };
    }
    case 'taskPhaseChanged': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      return patchTask(state, event.workspaceId, event.taskId, {
        phase: event.phase,
        updatedAtMs: event.occurredAtMs,
      });
    }
    case 'sessionMessageAppended': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      return patchTask(state, event.workspaceId, event.taskId, {
        messages: [...task.messages, event.message],
        updatedAtMs: event.occurredAtMs,
      });
    }
    case 'sessionActivityUpdated': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      const existingIndex = task.activities.findIndex(a => a.activityId === event.activity.activityId);
      const activities = existingIndex < 0
        ? [...task.activities, event.activity]
        : task.activities.map((activity, index) => (index === existingIndex ? event.activity : activity));
      return patchTask(state, event.workspaceId, event.taskId, {
        activities,
        updatedAtMs: event.occurredAtMs,
      });
    }
    case 'operationRequested': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      return patchTask(state, event.workspaceId, event.taskId, {
        pendingOperation: event.operation,
        updatedAtMs: event.occurredAtMs,
      });
    }
    case 'operationResolved': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      if (task.pendingOperation?.operationId !== event.operationId) {
        // Stale resolution — the Runtime already moved on; do not clobber.
        return state;
      }
      return patchTask(state, event.workspaceId, event.taskId, {
        pendingOperation: null,
        updatedAtMs: event.occurredAtMs,
      });
    }
    case 'deliveryReviewUpdated': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      return patchTask(state, event.workspaceId, event.taskId, {
        deliveryReview: event.review,
        updatedAtMs: event.occurredAtMs,
      });
    }
    case 'deliveryDecisionRecorded': {
      const workspace = state.workspaces[event.workspaceId];
      const task = workspace?.tasks[event.taskId];
      if (!workspace || !task) return state;
      return patchTask(state, event.workspaceId, event.taskId, {
        deliveryReview: task.deliveryReview
          ? { ...task.deliveryReview, decision: event.decision }
          : task.deliveryReview,
        updatedAtMs: event.occurredAtMs,
      });
    }
  }
}

function patchTask(
  state: WorkbenchRuntimeFields,
  workspaceId: string,
  taskId: string,
  patch: Partial<WorkbenchTask>,
): WorkbenchRuntimeFields {
  const workspace = state.workspaces[workspaceId];
  const task = workspace.tasks[taskId];
  return {
    ...state,
    workspaces: {
      ...state.workspaces,
      [workspaceId]: {
        ...workspace,
        tasks: {
          ...workspace.tasks,
          [taskId]: { ...task, ...patch },
        },
      },
    },
  };
}

export type WorkbenchRuntimeStore = ReturnType<typeof createWorkbenchRuntimeStore>;

export function createWorkbenchRuntimeStore() {
  return create<WorkbenchRuntimeState>()((set, get) => ({
    ...buildInitialState(),
    ingestEvent: event => {
      set(state => {
        if (state.lastSequence > 0 && event.sequence <= state.lastSequence) {
          // Stale/duplicate event from the seam: never rewind the projection.
          return state;
        }
        return {
          ...reduceProjection(state, event),
          lastSequence: Math.max(state.lastSequence, event.sequence),
          eventBuffer: appendEventBuffer(state.eventBuffer, event),
        };
      });
    },
    publishEvent: event => {
      const sequence = get().lastSequence + 1;
      get().ingestEvent({ ...event, sequence } as WorkbenchProjectionEvent);
    },    startDriver: driver => {
      get().stopDriver();
      set({ link: { kind: driver.kind, connected: false }, runtimePhase: INITIAL_RUNTIME_PHASE });
      driver.start({ ingestEvent: get().ingestEvent });
    },
    stopDriver: () => {
      set(state => ({ link: { ...state.link, connected: false } }));
    },
  }));
}

/** App-level singleton. Tests create isolated stores via createWorkbenchRuntimeStore(). */
export const useWorkbenchRuntimeStore = createWorkbenchRuntimeStore();

/** Resets the singleton's data fields in place (actions are preserved). */
export function resetWorkbenchRuntimeStoreForTests(): void {
  useWorkbenchRuntimeStore.setState(buildInitialState());
}
