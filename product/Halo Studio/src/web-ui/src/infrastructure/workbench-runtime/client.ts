import {
  PI_RPC_ADAPTER_IDENTITY,
  type WorkbenchRuntimeEvent,
  type WorkbenchRuntimeIntentReceipt,
  type WorkbenchRuntimeIntentRequest,
  type WorkbenchRuntimeSnapshot,
} from './types';

export const HALO_WORKBENCH_RUNTIME_SNAPSHOT_COMMAND = 'halo_workbench_runtime_snapshot';
export const HALO_WORKBENCH_RUNTIME_SUBMIT_INTENT_COMMAND = 'halo_workbench_runtime_submit_intent';
export const HALO_WORKBENCH_RUNTIME_EVENT = 'halo-workbench://event';

export type WorkbenchRuntimeUnlisten = () => void;

export interface WorkbenchRuntimeTransport {
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T>;
  listen<T>(
    event: string,
    callback: (payload: T) => void,
  ): Promise<WorkbenchRuntimeUnlisten> | WorkbenchRuntimeUnlisten;
}

export interface WorkbenchRuntimeClient {
  readSnapshot(): Promise<WorkbenchRuntimeSnapshot>;
  submitIntent(request: WorkbenchRuntimeIntentRequest): Promise<WorkbenchRuntimeIntentReceipt>;
  subscribe(
    callback: (event: WorkbenchRuntimeEvent) => void,
  ): Promise<WorkbenchRuntimeUnlisten>;
}

export const createWorkbenchRuntimeClient = (
  transport: WorkbenchRuntimeTransport,
): WorkbenchRuntimeClient => ({
  readSnapshot: () => transport.invoke<WorkbenchRuntimeSnapshot>(
    HALO_WORKBENCH_RUNTIME_SNAPSHOT_COMMAND,
    { request: {} },
  ),
  submitIntent: request => transport.invoke<WorkbenchRuntimeIntentReceipt>(
    HALO_WORKBENCH_RUNTIME_SUBMIT_INTENT_COMMAND,
    { request },
  ),
  subscribe: async callback => Promise.resolve(
    transport.listen<WorkbenchRuntimeEvent>(HALO_WORKBENCH_RUNTIME_EVENT, callback),
  ),
});

export const createTauriWorkbenchRuntimeTransport = (): WorkbenchRuntimeTransport => ({
  invoke: async <T>(command: string, args: Record<string, unknown>) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(command, args);
  },
  listen: async <T>(event: string, callback: (payload: T) => void) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<T>(event, message => callback(message.payload));
  },
});

export const EXPECTED_WORKBENCH_ADAPTER_IDENTITY = PI_RPC_ADAPTER_IDENTITY;

let requestSequence = 0;

export const createWorkbenchRuntimeRequestId = (intent: string): string => {
  requestSequence += 1;
  const randomId = globalThis.crypto?.randomUUID?.();
  return randomId
    ? `${intent}:${randomId}`
    : `${intent}:${Date.now()}:${requestSequence}`;
};


