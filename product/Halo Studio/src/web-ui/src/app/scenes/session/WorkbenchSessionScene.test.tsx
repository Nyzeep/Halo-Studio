// @vitest-environment jsdom

import React, { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import WorkbenchSessionScene from './WorkbenchSessionScene';

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

const runtimeStore = vi.hoisted(() => {
  let state: Record<string, unknown>;
  return {
    getState: () => state,
    getInitialState: () => state,
    setState: (next: Record<string, unknown>) => { state = next; },
    subscribe: () => () => undefined,
  };
});
const submitWorkbenchRuntimeIntent = vi.hoisted(() => vi.fn());

vi.mock('@/infrastructure/i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
    formatDate: (value: number | Date) => `formatted:${value}`,
  }),
}));

vi.mock('@/infrastructure/workbench-runtime', () => ({
  selectWorkbenchRuntimePhaseMessageKey: () => 'nav.sessions.workbenchRuntime.runtimePhase.ready',
  selectWorkbenchRuntimeSessionPhaseMessageKey: () => (
    'nav.sessions.workbenchRuntime.sessionPhase.waitingDeveloper'
  ),
  selectWorkbenchRuntimeInterruptionReasonMessageKey: (session: { cancellationMode?: string | null }) => (
    `nav.sessions.workbenchRuntime.interruptionReason.${session.cancellationMode ?? 'unknown'}`
  ),
  selectWorkbenchRuntimeErrorMessageKey: () => 'nav.sessions.workbenchRuntime.error',
  submitWorkbenchRuntimeIntent,
  workbenchRuntimeStore: runtimeStore,
}));

describe('WorkbenchSessionScene', () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    submitWorkbenchRuntimeIntent.mockReset();
    runtimeStore.setState({
      syncStatus: 'failed',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { displayName: 'Halo Studio' },
        sessions: [{ sessionId: 'session-1', phase: 'waitingDeveloper' }],
      },
      stableErrorCode: 'pi_protocol_error',
    });
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it('renders semantic i18n labels instead of internal phase and error codes', async () => {
    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.runtimePhase.ready');
    expect(container.textContent)
      .toContain('nav.sessions.workbenchRuntime.sessionPhase.waitingDeveloper');
    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.error');
    expect(container.textContent).not.toContain('pi_protocol_error');
  });

  it('does not offer a standard session as a managed task reply target', async () => {
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { displayName: 'Halo Studio', gitRepository: true, trusted: true },
        sessions: [{ sessionId: 'standard-session', mode: 'standard', phase: 'waitingDeveloper' }],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(container.querySelector('[data-testid="workbench-managed-task-session"]')).toBeNull();
    expect(container.querySelector('[data-testid="workbench-managed-task-reply"]')).toBeNull();
  });

  it('leaves a settled managed task waiting without exposing follow-up controls', async () => {
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { displayName: 'Halo Studio', gitRepository: true, trusted: true },
        sessions: [{ sessionId: 'managed-session', mode: 'managed', phase: 'waitingDeveloper' }],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(container.querySelector('[data-testid="workbench-managed-task-session"]')).toBeNull();
    expect(container.querySelector('[data-testid="workbench-managed-task-reply"]')).toBeNull();
    expect(container.querySelector('[data-testid="workbench-managed-task-send-reply"]')).toBeNull();
  });

  it('does not send a first prompt to a standard session returned by a malformed create receipt', async () => {
    submitWorkbenchRuntimeIntent
      .mockResolvedValue({})
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ sessionId: 'standard-session' });
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'C:/work/halo',
          gitRepository: true,
          trusted: true,
        },
        sessions: [],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });
    const prompt = container.querySelector<HTMLTextAreaElement>(
      '[data-testid="workbench-managed-task-prompt"]',
    );
    const trust = container.querySelector<HTMLInputElement>(
      '[data-testid="workbench-managed-task-trust"]',
    );
    const form = container.querySelector('form');
    expect(prompt).not.toBeNull();
    expect(trust).not.toBeNull();
    expect(form).not.toBeNull();

    await act(async () => {
      const setPrompt = Object.getOwnPropertyDescriptor(
        HTMLTextAreaElement.prototype,
        'value',
      )?.set;
      setPrompt?.call(prompt, 'Run the managed task');
      prompt!.dispatchEvent(new Event('input', { bubbles: true }));
      trust!.click();
    });
    await act(async () => {
      form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(2);

    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'C:/work/halo',
          gitRepository: true,
          trusted: true,
        },
        sessions: [{ sessionId: 'standard-session', mode: 'standard', phase: 'idle' }],
      },
      stableErrorCode: null,
    });
    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
      await Promise.resolve();
    });

    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(2);
    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.managedTask.actionFailed');
  });

  it('sends the first managed task prompt only after the created managed session becomes idle', async () => {
    submitWorkbenchRuntimeIntent
      .mockResolvedValue({})
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ sessionId: 'managed-session' });
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'C:/work/halo',
          gitRepository: true,
          trusted: true,
        },
        sessions: [],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });
    const prompt = container.querySelector<HTMLTextAreaElement>(
      '[data-testid="workbench-managed-task-prompt"]',
    );
    const trust = container.querySelector<HTMLInputElement>(
      '[data-testid="workbench-managed-task-trust"]',
    );
    const form = container.querySelector('form');
    expect(prompt).not.toBeNull();
    expect(trust).not.toBeNull();
    expect(form).not.toBeNull();

    await act(async () => {
      const setPrompt = Object.getOwnPropertyDescriptor(
        HTMLTextAreaElement.prototype,
        'value',
      )?.set;
      setPrompt?.call(prompt, 'Run the managed task');
      prompt!.dispatchEvent(new Event('input', { bubbles: true }));
      trust!.click();
    });
    await act(async () => {
      form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(2);
    expect(submitWorkbenchRuntimeIntent).toHaveBeenNthCalledWith(1, {
      type: 'confirmManagedWorkspace',
      workspaceId: 'workspace-1',
      rootPath: 'C:/work/halo',
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenNthCalledWith(2, {
      type: 'createSession',
      taskId: expect.stringMatching(/^managed-task-/),
      mode: 'managed',
    });

    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'C:/work/halo',
          gitRepository: true,
          trusted: true,
        },
        sessions: [{ sessionId: 'managed-session', mode: 'managed', phase: 'creating' }],
      },
      stableErrorCode: null,
    });
    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(2);

    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'C:/work/halo',
          gitRepository: true,
          trusted: true,
        },
        sessions: [{ sessionId: 'managed-session', mode: 'managed', phase: 'idle' }],
      },
      stableErrorCode: null,
    });
    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
      await Promise.resolve();
    });

    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(3);
    expect(submitWorkbenchRuntimeIntent).toHaveBeenLastCalledWith({
      type: 'sendUserInput',
      sessionId: 'managed-session',
      content: 'Run the managed task',
    });
  });

  it('offers only a one-time allow/deny control for a pending permission', async () => {
    submitWorkbenchRuntimeIntent.mockResolvedValue({});
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { workspaceId: 'workspace-1', displayName: 'Halo Studio' },
        sessions: [{ sessionId: 'session-1', mode: 'managed', phase: 'running' }],
        pendingOperations: [{
          operationId: 'operation-1',
          taskId: 'task-1',
          sessionId: 'session-1',
          kind: 'permission',
          phase: 'awaitingDecision',
          toolName: 'browser',
          arguments: '{"action":"[redacted]"}',
          riskLevel: 'highRisk',
        }],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    const decision = container.querySelector('[data-testid="workbench-permission-decision"]');
    expect(decision).not.toBeNull();
    expect(decision?.getAttribute('data-risk-level')).toBe('highRisk');
    expect(container.textContent).toContain('browser');
    expect(container.textContent).toContain('{"action":"[redacted]"}');
    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.permission.allowOnce');
    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.permission.deny');
    expect(container.textContent).toContain('nav.sessions.workbenchRuntime.permission.highRisk');

    // No permanent, session-level, or free-text approval surface is offered
    // inside the permission control itself.
    expect(container.textContent).not.toContain('always');
    expect(container.textContent).not.toContain('始终允许');
    expect(decision?.querySelector('textarea')).toBeNull();
    expect(decision?.querySelector('input')).toBeNull();

    const allow = container.querySelector<HTMLButtonElement>(
      '[data-testid="workbench-permission-allow"]',
    );
    allow?.click();
    await act(async () => {
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledWith({
      type: 'resolveOperation',
      operationId: 'operation-1',
      decision: { type: 'allowOnce' },
    });
  });
  it('renders a read-only delivery review and dispatches accept and reject decisions', async () => {
    submitWorkbenchRuntimeIntent.mockResolvedValue({});
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { displayName: 'Halo Studio', gitRepository: true, trusted: true },
        sessions: [
          {
            sessionId: 'managed-session',
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
                changedFiles: ['tracked.rs'],
                diffPreview: 'diff --git a/tracked.rs b/tracked.rs\n+changed',
                attribution: [{ path: 'tracked.rs', kind: 'taskModification' }],
              },
              summary: 'summary',
              verificationResults: 'verification',
              runConclusion: 'conclusion',
              decision: null,
            },
            error: null,
          },
        ],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(container.querySelector('[data-testid="workbench-delivery-review"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="workbench-delivery-diff"]')?.textContent)
      .toContain('+changed');

    const accept = container.querySelector<HTMLButtonElement>(
      '[data-testid="workbench-delivery-accept"]',
    );
    accept?.click();
    await act(async () => {
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledWith({
      type: 'acceptDelivery',
      sessionId: 'managed-session',
    });

    submitWorkbenchRuntimeIntent.mockClear();
    const reject = container.querySelector<HTMLButtonElement>(
      '[data-testid="workbench-delivery-reject"]',
    );
    reject?.click();
    await act(async () => {
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledWith({
      type: 'rejectDelivery',
      sessionId: 'managed-session',
    });
  });

  it('shows a frozen interrupted review after restart and resolves it without recapturing evidence', async () => {
    submitWorkbenchRuntimeIntent.mockResolvedValue({});
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'disconnected',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: null,
        sessions: [
          {
            sessionId: 'restored-managed-session',
            workspaceId: 'workspace-1',
            taskId: 'task-1',
            mode: 'managed',
            phase: 'interrupted',
            cancellationMode: null,
            baseline: null,
            messages: [],
            activities: [],
            deliveryReview: {
              evidence: {
                capturedAtMs: 1234,
                head: 'frozen-head',
                workingTreeFingerprint: 'f'.repeat(64),
                changedFiles: ['frozen.rs'],
                diffPreview: 'diff --git a/frozen.rs b/frozen.rs\n+frozen',
                attribution: [{ path: 'frozen.rs', kind: 'taskModification' }],
              },
              summary: 'frozen summary',
              verificationResults: 'frozen verification',
              runConclusion: 'frozen conclusion',
              decision: null,
            },
            error: null,
          },
        ],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(container.querySelector('[data-testid="workbench-delivery-review"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="workbench-delivery-diff"]')?.textContent)
      .toContain('+frozen');
    expect(container.querySelector('[data-testid="workbench-interruption-actions"]')).toBeNull();

    container.querySelector<HTMLButtonElement>(
      '[data-testid="workbench-delivery-accept"]',
    )?.click();
    await act(async () => {
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(1);
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledWith({
      type: 'acceptDelivery',
      sessionId: 'restored-managed-session',
    });
  });

  it('offers an interrupted managed session an explicit review path without replaying it', async () => {
    submitWorkbenchRuntimeIntent.mockResolvedValue({});
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: { displayName: 'Halo Studio', gitRepository: true, trusted: true },
        sessions: [
          {
            sessionId: 'interrupted-managed-session',
            mode: 'managed',
            phase: 'interrupted',
            cancellationMode: 'forced',
            error: {
              code: 'pi_transport_unavailable',
              summary: 'The managed Pi transport was interrupted.',
              recoveryAction: 'review_or_start_new_run',
            },
          },
        ],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    expect(submitWorkbenchRuntimeIntent).not.toHaveBeenCalled();
    expect(container.querySelector('[data-testid="workbench-interruption-actions"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="workbench-interruption-reason"]')?.textContent)
      .toContain('nav.sessions.workbenchRuntime.interruptionReason.forced');
    expect(container.textContent).not.toContain('The managed Pi transport was interrupted.');
    expect(container.textContent).not.toContain('pi_transport_unavailable');

    container.querySelector<HTMLButtonElement>(
      '[data-testid="workbench-interruption-review"]',
    )?.click();
    await act(async () => {
      await Promise.resolve();
    });
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledTimes(1);
    expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledWith({
      type: 'finishAndReview',
      sessionId: 'interrupted-managed-session',
    });
  });

  it('starts a fresh managed run from an interruption without reusing task state or sending a request', async () => {
    runtimeStore.setState({
      syncStatus: 'ready',
      snapshot: {
        phase: 'ready',
        adapter: { identity: 'pi-rpc-p0' },
        workspace: {
          workspaceId: 'workspace-1',
          displayName: 'Halo Studio',
          rootPath: 'C:/work/halo',
          gitRepository: true,
          trusted: true,
        },
        sessions: [{
          sessionId: 'interrupted-managed-session',
          workspaceId: 'workspace-1',
          taskId: 'interrupted-task',
          mode: 'managed',
          phase: 'interrupted',
          cancellationMode: 'forced',
          error: null,
        }],
      },
      stableErrorCode: null,
    });

    await act(async () => {
      root.render(<WorkbenchSessionScene isActive />);
    });

    const taskId = container.querySelector<HTMLInputElement>(
      '[data-testid="workbench-managed-task-id"]',
    );
    const prompt = container.querySelector<HTMLTextAreaElement>(
      '[data-testid="workbench-managed-task-prompt"]',
    );
    const trust = container.querySelector<HTMLInputElement>(
      '[data-testid="workbench-managed-task-trust"]',
    );
    const newRun = container.querySelector<HTMLButtonElement>(
      '[data-testid="workbench-interruption-new-run"]',
    );
    expect(taskId).not.toBeNull();
    expect(prompt).not.toBeNull();
    expect(trust).not.toBeNull();
    expect(newRun).not.toBeNull();

    await act(async () => {
      const setTaskId = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setTaskId?.call(taskId, 'interrupted-task');
      taskId!.dispatchEvent(new Event('input', { bubbles: true }));
      const setPrompt = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
      setPrompt?.call(prompt, 'Resume the interrupted work');
      prompt!.dispatchEvent(new Event('input', { bubbles: true }));
      trust!.click();
    });
    expect(taskId!.value).toBe('interrupted-task');
    expect(prompt!.value).toBe('Resume the interrupted work');
    expect(trust!.checked).toBe(true);

    await act(async () => {
      newRun!.click();
      await Promise.resolve();
    });

    expect(taskId!.value).not.toBe('interrupted-task');
    expect(prompt!.value).toBe('');
    expect(trust!.checked).toBe(false);
    expect(document.activeElement).toBe(prompt);
    expect(submitWorkbenchRuntimeIntent).not.toHaveBeenCalled();
  });

  it.each([
    ['native', 'nav.sessions.workbenchRuntime.interruptionReason.native'],
    ['forced', 'nav.sessions.workbenchRuntime.interruptionReason.forced'],
  ] as const)(
    'shows a safe reason and explicit dispositions when %s interruption has no error payload',
    async (cancellationMode, reasonKey) => {
      submitWorkbenchRuntimeIntent.mockResolvedValue({});
      runtimeStore.setState({
        syncStatus: 'ready',
        snapshot: {
          phase: 'ready',
          adapter: { identity: 'pi-rpc-p0' },
          workspace: {
            workspaceId: 'workspace-1',
            displayName: 'Halo Studio',
            rootPath: 'C:/work/halo',
            gitRepository: true,
            trusted: true,
          },
          sessions: [{
            sessionId: `interrupted-${cancellationMode}`,
            workspaceId: 'workspace-1',
            taskId: 'task-1',
            mode: 'managed',
            phase: 'interrupted',
            cancellationMode,
            error: null,
          }],
        },
        stableErrorCode: null,
      });

      await act(async () => {
        root.render(<WorkbenchSessionScene isActive />);
      });

      expect(container.querySelector('[data-testid="workbench-interruption-reason"]')?.textContent)
        .toContain(reasonKey);
      expect(container.querySelector('[data-testid="workbench-interruption-actions"]'))
        .not.toBeNull();
      expect(container.querySelector('[data-testid="workbench-interruption-new-run"]'))
        .not.toBeNull();
      expect(container.querySelector('[data-testid="workbench-interruption-keep-current"]'))
        .not.toBeNull();
      expect(container.querySelector('[data-testid="workbench-interruption-review"]'))
        .not.toBeNull();
      expect(submitWorkbenchRuntimeIntent).not.toHaveBeenCalled();

      container.querySelector<HTMLButtonElement>(
        '[data-testid="workbench-interruption-keep-current"]',
      )?.click();
      await act(async () => {
        await Promise.resolve();
      });
      expect(submitWorkbenchRuntimeIntent).not.toHaveBeenCalled();
      expect(container.textContent)
        .toContain('nav.sessions.workbenchRuntime.interruptionDisposition.kept');

      container.querySelector<HTMLButtonElement>(
        '[data-testid="workbench-interruption-new-run"]',
      )?.click();
      expect(submitWorkbenchRuntimeIntent).not.toHaveBeenCalled();

      container.querySelector<HTMLButtonElement>(
        '[data-testid="workbench-interruption-review"]',
      )?.click();
      await act(async () => {
        await Promise.resolve();
      });
      expect(submitWorkbenchRuntimeIntent).toHaveBeenCalledWith({
        type: 'finishAndReview',
        sessionId: `interrupted-${cancellationMode}`,
      });
    },
  );
});
