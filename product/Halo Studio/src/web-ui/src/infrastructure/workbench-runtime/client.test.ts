import { describe, expect, it, vi } from 'vitest';

import {
  createWorkbenchRuntimeClient,
  HALO_WORKBENCH_RUNTIME_EVENT,
  HALO_WORKBENCH_RUNTIME_SNAPSHOT_COMMAND,
  HALO_WORKBENCH_RUNTIME_SUBMIT_INTENT_COMMAND,
  type WorkbenchRuntimeTransport,
} from './client';
import type { WorkbenchRuntimeEvent, WorkbenchRuntimeSnapshot } from './types';

const INITIAL_SNAPSHOT: WorkbenchRuntimeSnapshot = {
  schemaVersion: 1,
  phase: 'disconnected',
  adapter: {
    identity: 'pi-rpc-p0',
    available: false,
    readiness: null,
  },
  workspace: null,
  sessions: [],
  pendingOperations: [],
  lastSequence: 0,
  stateVersion: 0,
  error: null,
};

describe('WorkbenchRuntimeClient', () => {
  it('exposes the runtime through two commands and one ordered event stream', async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === HALO_WORKBENCH_RUNTIME_SNAPSHOT_COMMAND) {
        return INITIAL_SNAPSHOT;
      }
      return undefined;
    });
    const unlisten = vi.fn();
    const listen = vi.fn(async () => unlisten);
    const transport: WorkbenchRuntimeTransport = { invoke, listen };
    const client = createWorkbenchRuntimeClient(transport);
    const onEvent = vi.fn<(event: WorkbenchRuntimeEvent) => void>();

    await expect(client.readSnapshot()).resolves.toEqual(INITIAL_SNAPSHOT);
    await client.submitIntent({
      requestId: 'request-1',
      intent: {
        type: 'openWorkspace',
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'D:\\workspace',
        },
      },
    });
    await expect(client.subscribe(onEvent)).resolves.toBe(unlisten);

    expect(invoke.mock.calls).toEqual([
      [HALO_WORKBENCH_RUNTIME_SNAPSHOT_COMMAND, { request: {} }],
      [HALO_WORKBENCH_RUNTIME_SUBMIT_INTENT_COMMAND, {
        request: {
          requestId: 'request-1',
          intent: {
            type: 'openWorkspace',
            workspace: {
              workspaceId: 'workspace-1',
              displayName: 'Halo Studio',
              rootPath: 'D:\\workspace',
            },
          },
        },
      }],
    ]);
    expect(listen).toHaveBeenCalledWith(HALO_WORKBENCH_RUNTIME_EVENT, onEvent);
  });
});

