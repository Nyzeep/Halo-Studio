import { describe, expect, it, vi } from 'vitest';

import type { WorkbenchRuntimeClient } from './client';
import { createWorkbenchRuntimeStore } from './store';
import type {
  WorkbenchRuntimeEvent,
  WorkbenchRuntimeIntentRequest,
  WorkbenchRuntimeSnapshot,
} from './types';

const snapshot = (
  lastSequence = 0,
  stateVersion = lastSequence,
  phase: WorkbenchRuntimeSnapshot['phase'] = 'disconnected',
): WorkbenchRuntimeSnapshot => {
  const readiness = phase === 'ready'
    ? {
        version: {
          version: '0.83.0' as const,
          profile: 'pi-rpc-0.83.0-p0' as const,
          evidenceSource: 'local_version_probe' as const,
        },
        capabilities: {
          required: [
            'userInput',
            'followUpInput',
            'sessionAbort',
            'sessionState',
            'sessionEntries',
            'sessionEntryCollection',
            'sessionEntryCursor',
            'sessionEntryIncremental',
            'assistantMessageStream',
            'toolExecutionStart',
            'toolExecutionUpdate',
            'toolExecutionEnd',
            'agentSettled',
            'permissionUiRequest',
            'permissionUiResponse',
          ],
          verified: [
            'sessionAbort',
            'sessionState',
            'sessionEntries',
            'sessionEntryCollection',
            'sessionEntryCursor',
            'sessionEntryIncremental',
          ],
        },
      }
    : null;
  return {
    schemaVersion: 1,
    phase,
    adapter: { identity: 'pi-rpc-p0', available: phase === 'ready', readiness },
    workspace: null,
    sessions: [],
    pendingOperations: [],
    lastSequence,
    stateVersion,
    error: null,
  };
};

const event = (sequence: number): WorkbenchRuntimeEvent => ({
  sequence,
  stateVersion: sequence,
  correlationId: `request-${sequence}`,
  kind: 'runtimeStateChanged',
  summary: 'Workbench Runtime is ready',
  sessionId: null,
  operationId: null,
  occurredAtMs: sequence,
});

const eventWithSummary = (
  sequence: number,
  summary: WorkbenchRuntimeEvent['summary'],
): WorkbenchRuntimeEvent => ({
  ...event(sequence),
  kind: 'sessionStateChanged',
  summary,
  sessionId: 'session-1',
});

const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(resolver => { resolve = resolver; });
  return { promise, resolve };
};

const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

describe('WorkbenchRuntimeStore', () => {
  it('subscribes before reading the bootstrap snapshot and drains newer events', async () => {
    const order: string[] = [];
    const bootstrap = deferred<WorkbenchRuntimeSnapshot>();
    let onEvent: ((value: WorkbenchRuntimeEvent) => void) | undefined;
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async callback => {
        order.push('subscribe');
        onEvent = callback;
        return vi.fn();
      }),
      readSnapshot: vi
        .fn<() => Promise<WorkbenchRuntimeSnapshot>>()
        .mockImplementationOnce(async () => {
          order.push('snapshot');
          return bootstrap.promise;
        })
        .mockResolvedValueOnce(snapshot(1, 1, 'probing')),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    const started = store.getState().start();
    await flush();
    onEvent?.(event(1));
    bootstrap.resolve(snapshot(0));
    await started;
    await flush();

    expect(order).toEqual(['subscribe', 'snapshot']);
    expect(store.getState().lastEvent?.sequence).toBe(1);
    expect(client.readSnapshot).toHaveBeenCalledTimes(2);
  });

  it('drops buffered events already covered by the bootstrap snapshot', async () => {
    let onEvent: ((value: WorkbenchRuntimeEvent) => void) | undefined;
    const bootstrap = deferred<WorkbenchRuntimeSnapshot>();
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async callback => {
        onEvent = callback;
        return vi.fn();
      }),
      readSnapshot: vi.fn(async () => bootstrap.promise),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    const started = store.getState().start();
    await flush();
    onEvent?.(event(1));
    bootstrap.resolve(snapshot(1, 1, 'probing'));
    await started;

    expect(store.getState().snapshot?.phase).toBe('probing');
    expect(store.getState().lastEvent).toBeNull();
    expect(client.readSnapshot).toHaveBeenCalledTimes(1);
  });

  it('coalesces an event gap into a serialized snapshot resync', async () => {
    let onEvent: ((value: WorkbenchRuntimeEvent) => void) | undefined;
    const resync = deferred<WorkbenchRuntimeSnapshot>();
    const readSnapshot = vi
      .fn<() => Promise<WorkbenchRuntimeSnapshot>>()
      .mockResolvedValueOnce(snapshot())
      .mockImplementationOnce(() => resync.promise);
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async callback => {
        onEvent = callback;
        return vi.fn();
      }),
      readSnapshot,
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);
    await store.getState().start();

    onEvent?.(event(2));
    onEvent?.(event(3));
    await flush();

    expect(store.getState().syncStatus).toBe('resyncing');
    expect(readSnapshot).toHaveBeenCalledTimes(2);
    resync.resolve(snapshot(3, 3, 'ready'));
    await flush();

    expect(store.getState().syncStatus).toBe('ready');
    expect(store.getState().snapshot?.lastSequence).toBe(3);
  });

  it('fails closed when snapshots cannot bridge an event gap', async () => {
    let onEvent: ((value: WorkbenchRuntimeEvent) => void) | undefined;
    const readSnapshot = vi
      .fn<() => Promise<WorkbenchRuntimeSnapshot>>()
      .mockResolvedValueOnce(snapshot())
      .mockResolvedValueOnce(snapshot())
      .mockResolvedValueOnce(snapshot())
      .mockRejectedValue(new Error('unbounded resync'));
    const unlisten = vi.fn();
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async callback => {
        onEvent = callback;
        return unlisten;
      }),
      readSnapshot,
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);
    await store.getState().start();

    onEvent?.(event(2));
    await flush();
    await flush();

    expect(readSnapshot).toHaveBeenCalledTimes(3);
    expect(store.getState().syncStatus).toBe('failed');
    expect(store.getState().stableErrorCode).toBe('runtime_event_gap');
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('shares concurrent starts and unlistens exactly once on repeated stop', async () => {
    const unlisten = vi.fn();
    const subscribeGate = deferred<() => void>();
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => subscribeGate.promise),
      readSnapshot: vi.fn(async () => snapshot()),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    const first = store.getState().start();
    const second = store.getState().start();
    expect(first).toBe(second);
    subscribeGate.resolve(unlisten);
    await first;

    store.getState().stop();
    store.getState().stop();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('does not let a fenced start completion clear a newer in-flight start', async () => {
    const firstSubscribe = deferred<() => void>();
    const secondSubscribe = deferred<() => void>();
    const subscribe = vi
      .fn<(callback: (value: WorkbenchRuntimeEvent) => void) => Promise<() => void>>()
      .mockImplementationOnce(async () => firstSubscribe.promise)
      .mockImplementationOnce(async () => secondSubscribe.promise)
      .mockResolvedValue(vi.fn());
    const client: WorkbenchRuntimeClient = {
      subscribe,
      readSnapshot: vi.fn(async () => snapshot()),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    const first = store.getState().start();
    store.getState().stop();
    const second = store.getState().start();

    firstSubscribe.resolve(vi.fn());
    await first;

    const concurrent = store.getState().start();
    expect(concurrent).toBe(second);
    expect(subscribe).toHaveBeenCalledTimes(2);

    secondSubscribe.resolve(vi.fn());
    await second;
  });

  it('fences late bootstrap completion after stop', async () => {
    const bootstrap = deferred<WorkbenchRuntimeSnapshot>();
    const unlisten = vi.fn();
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => unlisten),
      readSnapshot: vi.fn(async () => bootstrap.promise),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    const started = store.getState().start();
    await flush();
    store.getState().stop();
    bootstrap.resolve(snapshot(4, 4, 'ready'));
    await started;

    expect(store.getState().syncStatus).toBe('idle');
    expect(store.getState().snapshot).toBeNull();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('submits intents without mutating the projected snapshot optimistically', async () => {
    const submitIntent = vi.fn(async (request: WorkbenchRuntimeIntentRequest) => ({
      requestId: request.requestId,
      stateVersion: 1,
      sessionId: 'session-1',
    }));
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => snapshot(0, 0, 'ready')),
      submitIntent,
    };
    const store = createWorkbenchRuntimeStore(client);
    await store.getState().start();
    const before = store.getState().snapshot;

    await store.getState().submitIntent({
      requestId: 'request-create',
      intent: { type: 'createSession', taskId: 'task-1', mode: 'standard' },
    });

    expect(store.getState().snapshot).toBe(before);
    expect(submitIntent).toHaveBeenCalledTimes(1);
  });

  it('fails closed on an incompatible schema or adapter identity', async () => {
    const incompatible = {
      ...snapshot(),
      schemaVersion: 2,
      adapter: { identity: 'another-runtime', available: false, readiness: null },
    } as unknown as WorkbenchRuntimeSnapshot;
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => incompatible),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();

    expect(store.getState().syncStatus).toBe('failed');
    expect(store.getState().stableErrorCode).toBe('runtime_contract_mismatch');
    expect(store.getState().snapshot).toBeNull();
  });

  it('projects persisted interruption errors without exposing their summaries', async () => {
    const secretCanary = 'interruption-history-secret';
    const restored = snapshot();
    restored.sessions = [
      {
        workspaceId: 'workspace-1',
        taskId: 'task-application',
        sessionId: 'session-application',
        mode: 'managed',
        cancellationMode: null,
        phase: 'interrupted',
        error: {
          code: 'application_interrupted',
          recoveryAction: 'start_new_run_or_review_interruption',
          summary: secretCanary,
        },
      },
      {
        workspaceId: 'workspace-1',
        taskId: 'task-workspace',
        sessionId: 'session-workspace',
        mode: 'managed',
        cancellationMode: null,
        phase: 'interrupted',
        error: {
          code: 'workspace_closed',
          recoveryAction: 'start_new_run_or_review_interruption',
          summary: secretCanary,
        },
      },
    ];
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => restored),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();

    expect(store.getState().syncStatus).toBe('ready');
    expect(store.getState().snapshot?.sessions.map(session => session.error)).toEqual([
      {
        code: 'application_interrupted',
        recoveryAction: 'start_new_run_or_review_interruption',
        summary: 'The Halo Workbench Runtime reported an error',
      },
      {
        code: 'workspace_closed',
        recoveryAction: 'start_new_run_or_review_interruption',
        summary: 'The Halo Workbench Runtime reported an error',
      },
    ]);
    expect(JSON.stringify(store.getState().snapshot)).not.toContain(secretCanary);
  });

  it('strips unknown fields from snapshots and events before storing them', async () => {
    const secretCanary = 'secret-canary-value';
    let onEvent: ((value: WorkbenchRuntimeEvent) => void) | undefined;
    const taintedSnapshot = {
      ...snapshot(0, 0, 'ready'),
      rawPayload: secretCanary,
      adapter: {
        identity: 'pi-rpc-p0',
        available: true,
        readiness: {
          version: {
            version: '0.83.0',
            profile: 'pi-rpc-0.83.0-p0',
            evidenceSource: 'local_version_probe',
          },
          capabilities: {
            required: [
              'userInput',
              'followUpInput',
              'sessionAbort',
              'sessionState',
              'sessionEntries',
              'sessionEntryCollection',
              'sessionEntryCursor',
              'sessionEntryIncremental',
              'assistantMessageStream',
              'toolExecutionStart',
              'toolExecutionUpdate',
              'toolExecutionEnd',
              'agentSettled',
              'permissionUiRequest',
              'permissionUiResponse',
            ],
            verified: [
              'sessionAbort',
              'sessionState',
              'sessionEntries',
              'sessionEntryCollection',
              'sessionEntryCursor',
              'sessionEntryIncremental',
            ],
          },
        },
        endpoint: secretCanary,
      },
      error: {
        code: 'provider_unavailable',
        recoveryAction: 'configure_provider',
        summary: secretCanary,
      },
    } as unknown as WorkbenchRuntimeSnapshot;
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async callback => {
        onEvent = callback;
        return vi.fn();
      }),
      readSnapshot: vi
        .fn<() => Promise<WorkbenchRuntimeSnapshot>>()
        .mockResolvedValueOnce(taintedSnapshot)
        .mockResolvedValueOnce(snapshot(1, 1, 'ready')),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();
    expect(JSON.stringify(store.getState().snapshot)).not.toContain(secretCanary);
    expect(store.getState().snapshot?.adapter.readiness?.version.version).toBe('0.83.0');
    expect(store.getState().snapshot?.error?.summary).toBe(
      'The Halo Workbench Runtime reported an error',
    );

    onEvent?.({
      ...event(1),
      summary: secretCanary,
      rawPayload: secretCanary,
    } as unknown as WorkbenchRuntimeEvent);
    await flush();

    expect(JSON.stringify(store.getState().lastEvent)).not.toContain(secretCanary);
    expect(store.getState().lastEvent).toBeNull();
  });

  it('fails closed when a ready snapshot lacks handshake verification evidence', async () => {
    const invalidReady = snapshot(0, 0, 'ready');
    invalidReady.adapter.readiness!.capabilities.verified = [];
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => invalidReady),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();

    expect(store.getState().syncStatus).toBe('failed');
    expect(store.getState().stableErrorCode).toBe('runtime_contract_mismatch');
    expect(store.getState().snapshot).toBeNull();
  });

  it('fails closed when adapter readiness contains non-enumerated public fields', async () => {
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => ({
        ...snapshot(0, 0, 'ready'),
        adapter: {
          identity: 'pi-rpc-p0',
          available: true,
          readiness: {
            version: {
              version: '0.83.0 C:\\Users\\secret',
              profile: 'pi-rpc-0.83.0-p0',
              evidenceSource: 'local_version_probe',
            },
            capabilities: { required: ['userInput'], verified: [] },
          },
        },
      } as unknown as WorkbenchRuntimeSnapshot)),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();

    expect(store.getState().syncStatus).toBe('failed');
    expect(store.getState().stableErrorCode).toBe('runtime_contract_mismatch');
    expect(store.getState().snapshot).toBeNull();
  });

  it('fails closed when adapter readiness version and profile disagree', async () => {
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => ({
        ...snapshot(0, 0, 'ready'),
        adapter: {
          identity: 'pi-rpc-p0',
          available: true,
          readiness: {
            version: {
              version: '0.83.0',
              profile: 'pi-rpc-0.81.1-p0',
              evidenceSource: 'local_version_probe',
            },
            capabilities: {
              required: [
                'userInput',
                'followUpInput',
                'sessionAbort',
                'sessionState',
                'sessionEntries',
                'sessionEntryCollection',
                'sessionEntryCursor',
                'sessionEntryIncremental',
                'assistantMessageStream',
                'toolExecutionStart',
                'toolExecutionUpdate',
                'toolExecutionEnd',
                'agentSettled',
                'permissionUiRequest',
                'permissionUiResponse',
              ],
              verified: [
                'sessionAbort',
                'sessionState',
                'sessionEntries',
                'sessionEntryCollection',
                'sessionEntryCursor',
                'sessionEntryIncremental',
              ],
            },
          },
        },
      } as unknown as WorkbenchRuntimeSnapshot)),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();

    expect(store.getState().syncStatus).toBe('failed');
    expect(store.getState().stableErrorCode).toBe('runtime_contract_mismatch');
    expect(store.getState().snapshot).toBeNull();
  });

  it('classifies malformed snapshot payloads as a contract mismatch', async () => {
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi.fn(async () => null as unknown as WorkbenchRuntimeSnapshot),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();

    expect(store.getState().syncStatus).toBe('failed');
    expect(store.getState().stableErrorCode).toBe('runtime_contract_mismatch');
  });

  it('sanitizes delivery review fields and fails closed on an invalid decision', async () => {
    const reviewingSnapshot = {
      ...snapshot(0, 0, 'ready'),
      sessions: [
        {
          workspaceId: 'workspace-1',
          taskId: 'task-1',
          sessionId: 'session-1',
          mode: 'managed',
          phase: 'reviewing',
          baseline: null,
          messages: [],
          activities: [],
          deliveryReview: {
            evidence: {
              capturedAtMs: 1234,
              head: 'test-head',
              workingTreeFingerprint: 'a'.repeat(64),
              changedFiles: ['tracked.rs', 'new-file.rs'],
              diffPreview: 'diff --git a/tracked.rs b/tracked.rs',
              attribution: [
                { path: 'already-tracked.rs', kind: 'existingUserModification' },
                { path: 'tracked.rs', kind: 'taskModification' },
              ],
            },
            summary: 'task summary',
            verificationResults: 'verification',
            runConclusion: 'conclusion',
            decision: null,
            rawPayload: 'secret-canary-value',
          },
          error: null,
        },
      ],
    } as unknown as WorkbenchRuntimeSnapshot;
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi
        .fn<() => Promise<WorkbenchRuntimeSnapshot>>()
        .mockResolvedValueOnce(reviewingSnapshot)
        .mockResolvedValueOnce(snapshot(1, 1, 'ready')),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);
    await store.getState().start();

    const session = store.getState().snapshot?.sessions[0];
    expect(session?.deliveryReview?.evidence.changedFiles).toEqual(['tracked.rs', 'new-file.rs']);
    expect(session?.deliveryReview?.summary).toBe('task summary');
    expect(JSON.stringify(session)).not.toContain('rawPayload');

    const invalidDecision = {
      ...reviewingSnapshot,
      sessions: [
        {
          ...reviewingSnapshot.sessions[0],
          deliveryReview: {
            ...reviewingSnapshot.sessions[0].deliveryReview,
            decision: 'allow',
          },
        },
      ],
    } as unknown as WorkbenchRuntimeSnapshot;
    const invalidClient: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async () => vi.fn()),
      readSnapshot: vi
        .fn<() => Promise<WorkbenchRuntimeSnapshot>>()
        .mockResolvedValueOnce(invalidDecision)
        .mockResolvedValueOnce(snapshot(1, 1, 'ready')),
      submitIntent: vi.fn(),
    };
    const invalidStore = createWorkbenchRuntimeStore(invalidClient);
    await invalidStore.getState().start();
    expect(invalidStore.getState().stableErrorCode).toBe('runtime_contract_mismatch');
  });

  it('accepts settled and interrupted session phases with only safe event summaries', async () => {
    let onEvent: ((value: WorkbenchRuntimeEvent) => void) | undefined;
    const initial = {
      ...snapshot(1, 1, 'ready'),
      sessions: [
        {
          workspaceId: 'workspace-1',
          taskId: 'task-1',
          sessionId: 'session-1',
          mode: 'managed' as const,
          phase: 'waitingDeveloper' as const,
          cancellationMode: null,
        },
        {
          workspaceId: 'workspace-1',
          taskId: 'task-2',
          sessionId: 'session-2',
          mode: 'standard' as const,
          phase: 'interrupted' as const,
          cancellationMode: 'forced' as const,
        },
      ],
    };
    const client: WorkbenchRuntimeClient = {
      subscribe: vi.fn(async callback => {
        onEvent = callback;
        return vi.fn();
      }),
      readSnapshot: vi.fn()
        .mockResolvedValueOnce(initial)
        .mockResolvedValue({ ...initial, lastSequence: 2, stateVersion: 2 }),
      submitIntent: vi.fn(),
    };
    const store = createWorkbenchRuntimeStore(client);

    await store.getState().start();
    onEvent?.(eventWithSummary(2, 'Workbench session is waiting for developer'));
    await flush();

    expect(store.getState().snapshot?.sessions.map(session => session.phase)).toEqual([
      'waitingDeveloper',
      'interrupted',
    ]);
    expect(store.getState().snapshot?.sessions[1].cancellationMode).toBe('forced');
    expect(store.getState().lastEvent?.summary).toBe(
      'Workbench session is waiting for developer',
    );

    onEvent?.({
      ...eventWithSummary(3, 'agent_settled' as WorkbenchRuntimeEvent['summary']),
    });
    await flush();
    expect(store.getState().lastEvent?.summary).toBe(
      'Workbench session is waiting for developer',
    );
  });
});
