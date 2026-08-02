export const HALO_WORKBENCH_SCHEMA_VERSION = 1 as const;
export const PI_RPC_ADAPTER_IDENTITY = 'pi-rpc' as const;

export type WorkbenchRuntimePhase =
  | 'disconnected'
  | 'probing'
  | 'starting'
  | 'ready'
  | 'failed'
  | 'stopping';

export interface WorkbenchRuntimeWorkspace {
  workspaceId: string;
  displayName: string;
  rootPath: string;
  trusted: boolean;
  gitRepository: boolean;
}

export interface WorkbenchRuntimeSession {
  sessionId: string;
  mode: 'standard' | 'managed';
  phase: 'creating' | 'idle' | 'running' | 'stopping' | 'ended' | 'failed';
}

export interface WorkbenchRuntimePendingOperation {
  operationId: string;
  sessionId: string;
  kind: 'permission' | 'question';
  redactedToolCallId: string | null;
  phase: 'awaitingDecision' | 'decisionSubmitted';
}

export interface WorkbenchRuntimeError {
  code: string;
  recoveryAction: string;
  summary: string;
}

export interface WorkbenchRuntimeSnapshot {
  schemaVersion: typeof HALO_WORKBENCH_SCHEMA_VERSION;
  phase: WorkbenchRuntimePhase;
  adapter: {
    identity: typeof PI_RPC_ADAPTER_IDENTITY;
    available: boolean;
  };
  workspace: WorkbenchRuntimeWorkspace | null;
  sessions: WorkbenchRuntimeSession[];
  pendingOperations: WorkbenchRuntimePendingOperation[];
  lastSequence: number;
  stateVersion: number;
  error: WorkbenchRuntimeError | null;
}

export type WorkbenchRuntimeEventKind =
  | 'runtimeStateChanged'
  | 'workspaceChanged'
  | 'sessionStateChanged'
  | 'operationRequested'
  | 'operationResolved';

export interface WorkbenchRuntimeEvent {
  sequence: number;
  stateVersion: number;
  correlationId: string | null;
  kind: WorkbenchRuntimeEventKind;
  summary: string;
  sessionId: string | null;
  operationId: string | null;
  occurredAtMs: number;
}

export interface WorkbenchRuntimeWorkspaceInput {
  workspaceId: string;
  displayName: string;
  rootPath: string;
}

export type WorkbenchRuntimeOperationDecision =
  | { type: 'allowOnce' }
  | { type: 'deny' }
  | { type: 'answer'; content: string };

export type WorkbenchRuntimeIntent =
  | { type: 'openWorkspace'; workspace: WorkbenchRuntimeWorkspaceInput }
  | { type: 'closeWorkspace' }
  | { type: 'createSession'; mode: 'standard' | 'managed' }
  | { type: 'sendUserInput'; sessionId: string; content: string }
  | { type: 'stopSession'; sessionId: string }
  | { type: 'endSession'; sessionId: string }
  | {
      type: 'resolveOperation';
      operationId: string;
      decision: WorkbenchRuntimeOperationDecision;
    };

export interface WorkbenchRuntimeIntentRequest {
  requestId: string;
  intent: WorkbenchRuntimeIntent;
}

export interface WorkbenchRuntimeIntentReceipt {
  requestId: string;
  stateVersion: number;
  sessionId: string | null;
}
