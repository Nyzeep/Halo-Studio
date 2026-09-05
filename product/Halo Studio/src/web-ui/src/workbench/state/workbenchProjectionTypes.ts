/**
 * Workbench runtime projection types (M5, ADR-0075/0076/0077; issue #57).
 *
 * The M5 strip workbench projects the Halo Workbench Runtime into a
 * multi-workspace task model. Session-level fact shapes are reused verbatim
 * from the M1 projection surface (src/infrastructure/workbench-runtime —
 * issue #54), so the future Tauri event driver can feed the same vocabulary:
 * messages, tool activities, pending operations and delivery reviews are
 * THE SAME types, not lookalikes.
 *
 * Boundary note: nothing in this file may be imported by the UI store
 * (WorkbenchUiStore holds no facts/credentials/evidence — M5 acceptance #4).
 */

import type {
  WorkbenchRuntimeActivity,
  WorkbenchRuntimeDeliveryReview,
  WorkbenchRuntimeError,
  WorkbenchRuntimeMessage,
  WorkbenchRuntimePendingOperation,
  WorkbenchRuntimePhase,
  WorkbenchRuntimeSession,
} from '@/infrastructure/workbench-runtime/types';

import type { WorkbenchExecutorId } from './workbenchExecutors';

/** Session/task phases reuse the M1 session phase vocabulary verbatim. */
export type WorkbenchTaskPhase = WorkbenchRuntimeSession['phase'];

export interface WorkbenchTask {
  taskId: string;
  sessionId: string | null;
  title: string;
  executor: WorkbenchExecutorId;
  mode: 'managed' | 'standard';
  phase: WorkbenchTaskPhase;
  messages: WorkbenchRuntimeMessage[];
  activities: WorkbenchRuntimeActivity[];
  pendingOperation: WorkbenchRuntimePendingOperation | null;
  deliveryReview: WorkbenchRuntimeDeliveryReview | null;
  error: WorkbenchRuntimeError | null;
  updatedAtMs: number;
}

export interface WorkbenchWorkspace {
  workspaceId: string;
  displayName: string;
  rootPath: string;
  branch: string;
  trusted: boolean;
  gitRepository: boolean;
  /**
   * niri strip order — append-only. Only `createTask` inserts (right of the
   * focused column); there is no reorder operation anywhere in the store.
   */
  taskOrder: string[];
  tasks: Record<string, WorkbenchTask>;
}

/**
 * Transport behind the projection. 'mock-event-stream' fills the store with
 * the in-memory mock event stream; 'tauri-event-stream' is the reserved seam
 * for the real Halo Workbench Runtime event wiring.
 */
export type WorkbenchRuntimeLinkKind = 'mock-event-stream' | 'tauri-event-stream';

export interface WorkbenchRuntimeLinkStatus {
  kind: WorkbenchRuntimeLinkKind;
  connected: boolean;
}

/** Delivery decision vocabulary (M1-aligned). */
export type WorkbenchDeliveryDecision = 'accepted' | 'rejected';

/** Operation decision vocabulary (M1 `WorkbenchRuntimeOperationDecision`). */
export type WorkbenchOperationDecision = 'allowOnce' | 'deny';

/**
 * Projection event domain — the UI-projection side of the ADR-0080
 * durable/live event seam. Every mutation of runtime facts in the strip
 * workbench enters through `ingestEvent` as one of these.
 */
export type WorkbenchProjectionEvent =
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'runtimeStateChanged';
    summary: string;
    runtimePhase: WorkbenchRuntimePhase;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'workspaceOpened';
    summary: string;
    workspace: WorkbenchWorkspace;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'taskCreated';
    summary: string;
    workspaceId: string;
    task: WorkbenchTask;
    insertAfterTaskId: string | null;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'taskPhaseChanged';
    summary: string;
    workspaceId: string;
    taskId: string;
    phase: WorkbenchTaskPhase;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'sessionMessageAppended';
    summary: string;
    workspaceId: string;
    taskId: string;
    message: WorkbenchRuntimeMessage;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'sessionActivityUpdated';
    summary: string;
    workspaceId: string;
    taskId: string;
    activity: WorkbenchRuntimeActivity;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'operationRequested';
    summary: string;
    workspaceId: string;
    taskId: string;
    operation: WorkbenchRuntimePendingOperation;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'operationResolved';
    summary: string;
    workspaceId: string;
    taskId: string;
    operationId: string;
    decision: WorkbenchOperationDecision;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'deliveryReviewUpdated';
    summary: string;
    workspaceId: string;
    taskId: string;
    review: WorkbenchRuntimeDeliveryReview | null;
  }
  | {
    sequence: number;
    occurredAtMs: number;
    kind: 'deliveryDecisionRecorded';
    summary: string;
    workspaceId: string;
    taskId: string;
    decision: WorkbenchDeliveryDecision;
  };

/**
 * Distributive Omit over the event union: keeps the discriminated-union shape
 * so producers (mock driver, local intent synthesis) can build event literals
 * without repeating `sequence` — the store stamps it.
 */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

export type WorkbenchProjectionEventInput = DistributiveOmit<WorkbenchProjectionEvent, 'sequence'>;

/** Bounded event ring shown in the shell diagnostics area. */
export const WORKBENCH_EVENT_BUFFER_LIMIT = 100;

export type CreateWorkbenchTaskRequest = {
  workspaceId: string;
  title: string;
  executor: WorkbenchExecutorId;
  /** Insert right after this task (niri: right of focus); null appends. */
  insertAfterTaskId: string | null;
};

export type CreateWorkbenchWorkspaceRequest = {
  displayName: string;
  rootPath: string;
  branch: string;
};
