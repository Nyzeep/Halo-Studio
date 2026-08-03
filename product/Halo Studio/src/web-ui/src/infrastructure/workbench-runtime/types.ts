export const HALO_WORKBENCH_SCHEMA_VERSION = 1 as const;
export const PI_RPC_ADAPTER_IDENTITY = 'pi-rpc-p0' as const;

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
  phase:
    | 'creating'
    | 'idle'
    | 'running'
    | 'waitingDeveloper'
    | 'interrupted'
    | 'stopping'
    | 'ended'
    | 'failed';
}

export interface WorkbenchRuntimePendingOperation {
  operationId: string;
  sessionId: string;
  kind: 'permission';
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
    readiness: WorkbenchRuntimeAdapterReadiness | null;
  };
  workspace: WorkbenchRuntimeWorkspace | null;
  sessions: WorkbenchRuntimeSession[];
  pendingOperations: WorkbenchRuntimePendingOperation[];
  lastSequence: number;
  stateVersion: number;
  error: WorkbenchRuntimeError | null;
}

export type WorkbenchPiRpcVersion = '0.81.1' | '0.83.0';

export type WorkbenchPiRpcCompatibilityProfile =
  | 'pi-rpc-0.81.1-p0'
  | 'pi-rpc-0.83.0-p0';

export type WorkbenchPiRpcVersionEvidenceSource = 'local_version_probe';

export type WorkbenchPiRpcCapability =
  | 'prompt'
  | 'follow_up'
  | 'abort'
  | 'get_state'
  | 'get_entries'
  | 'get_entries.entries'
  | 'get_entries.leaf_id'
  | 'get_entries.since'
  | 'message_update'
  | 'tool_execution_start'
  | 'tool_execution_update'
  | 'tool_execution_end'
  | 'agent_settled'
  | 'extension_ui_request'
  | 'extension_ui_response';

export interface WorkbenchRuntimeAdapterReadiness {
  version: {
    version: WorkbenchPiRpcVersion;
    profile: WorkbenchPiRpcCompatibilityProfile;
    evidenceSource: WorkbenchPiRpcVersionEvidenceSource;
  };
  capabilities: {
    required: WorkbenchPiRpcCapability[];
  };
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
  | { type: 'deny' };

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
