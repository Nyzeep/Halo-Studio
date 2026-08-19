import React, { type FormEvent, useEffect, useRef, useState } from 'react';
import { CheckCircle2, GitBranch, Loader2, ShieldCheck } from 'lucide-react';
import { useStore } from 'zustand';

import { useI18n } from '@/infrastructure/i18n';
import {
  submitWorkbenchRuntimeIntent,
  workbenchRuntimeStore,
} from '@/infrastructure/workbench-runtime';

import './WorkbenchManagedTaskComposer.scss';

interface PendingFirstPrompt {
  sessionId: string;
  content: string;
}

let managedTaskIdSequence = 0;

const createDefaultTaskId = (): string => `managed-task-${Date.now()}-${++managedTaskIdSequence}`;

interface WorkbenchManagedTaskComposerProps {
  newRunVersion: number;
}

const WorkbenchManagedTaskComposer: React.FC<WorkbenchManagedTaskComposerProps> = ({ newRunVersion }) => {
  const { t } = useI18n('common');
  const runtimeState = useStore(workbenchRuntimeStore);
  const snapshot = runtimeState.snapshot;
  const workspace = snapshot?.workspace;
  const [taskId, setTaskId] = useState(createDefaultTaskId);
  const [firstPrompt, setFirstPrompt] = useState('');
  const [managedWorkspaceConfirmed, setManagedWorkspaceConfirmed] = useState(false);
  const [pendingFirstPrompt, setPendingFirstPrompt] = useState<PendingFirstPrompt | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [isSendingFirstPrompt, setIsSendingFirstPrompt] = useState(false);
  const [actionErrorKey, setActionErrorKey] = useState<string | null>(null);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const previousNewRunVersion = useRef(newRunVersion);

  useEffect(() => {
    setManagedWorkspaceConfirmed(false);
    setActionErrorKey(null);
  }, [workspace?.workspaceId, workspace?.rootPath]);

  useEffect(() => {
    if (previousNewRunVersion.current === newRunVersion) return;

    previousNewRunVersion.current = newRunVersion;
    setTaskId(createDefaultTaskId());
    setFirstPrompt('');
    setManagedWorkspaceConfirmed(false);
    setActionErrorKey(null);
    promptRef.current?.focus();
    promptRef.current?.scrollIntoView?.({ block: 'nearest' });
  }, [newRunVersion]);

  useEffect(() => {
    if (!pendingFirstPrompt || isSendingFirstPrompt) return undefined;
    const session = snapshot?.sessions.find(item => item.sessionId === pendingFirstPrompt.sessionId);
    if (!session) return undefined;

    if (session.phase === 'failed' || session.phase === 'ended' || session.phase === 'interrupted') {
      setPendingFirstPrompt(null);
      setActionErrorKey('nav.sessions.workbenchRuntime.managedTask.actionFailed');
      return undefined;
    }
    if (session.mode !== 'managed') {
      setPendingFirstPrompt(null);
      setActionErrorKey('nav.sessions.workbenchRuntime.managedTask.actionFailed');
      return undefined;
    }
    if (session.phase !== 'idle') return undefined;

    setIsSendingFirstPrompt(true);
    void submitWorkbenchRuntimeIntent({
      type: 'sendUserInput',
      sessionId: pendingFirstPrompt.sessionId,
      content: pendingFirstPrompt.content,
    })
      .then(() => {
        setPendingFirstPrompt(null);
        setFirstPrompt('');
        setActionErrorKey(null);
      })
      .catch(() => {
        setPendingFirstPrompt(null);
        setActionErrorKey('nav.sessions.workbenchRuntime.managedTask.actionFailed');
      })
      .finally(() => {
        setIsSendingFirstPrompt(false);
      });

    return undefined;
  }, [isSendingFirstPrompt, pendingFirstPrompt, snapshot?.sessions]);

  const handleCreateManagedTask = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (
      !workspace
      || snapshot?.phase !== 'ready'
      || !workspace.gitRepository
      || !managedWorkspaceConfirmed
      || !taskId.trim()
      || !firstPrompt.trim()
      || isCreating
      || pendingFirstPrompt
    ) {
      return;
    }

    setIsCreating(true);
    setActionErrorKey(null);
    try {
      await submitWorkbenchRuntimeIntent({
        type: 'confirmManagedWorkspace',
        workspaceId: workspace.workspaceId,
        rootPath: workspace.rootPath,
      });
      const receipt = await submitWorkbenchRuntimeIntent({
        type: 'createSession',
        taskId: taskId.trim(),
        mode: 'managed',
      });
      if (!receipt.sessionId) throw new Error('managed_session_missing');
      setPendingFirstPrompt({
        sessionId: receipt.sessionId,
        content: firstPrompt.trim(),
      });
    } catch {
      setActionErrorKey('nav.sessions.workbenchRuntime.managedTask.actionFailed');
    } finally {
      setIsCreating(false);
    }
  };

  const canCreateManagedTask = Boolean(
    workspace
    && snapshot?.phase === 'ready'
    && workspace.gitRepository
    && managedWorkspaceConfirmed
    && taskId.trim()
    && firstPrompt.trim()
    && !isCreating
    && !pendingFirstPrompt,
  );
  return (
    <section className="halo-workbench-managed-task" data-testid="workbench-managed-task-composer">
      <header className="halo-workbench-managed-task__header">
        <div>
          <h2>{t('nav.sessions.workbenchRuntime.managedTask.title')}</h2>
          <p>{t('nav.sessions.workbenchRuntime.managedTask.description')}</p>
        </div>
        <ShieldCheck size={18} aria-hidden="true" />
      </header>

      <div className="halo-workbench-managed-task__workspace" role="status">
        <div className="halo-workbench-managed-task__workspace-line">
          <GitBranch size={14} aria-hidden="true" />
          <span>{t('nav.sessions.workbenchRuntime.managedTask.workspaceRoot')}</span>
          <code>{workspace?.rootPath ?? t('nav.sessions.workbenchRuntime.managedTask.workspaceUnavailable')}</code>
        </div>
        <div className="halo-workbench-managed-task__workspace-facts">
          <span>
            {t(workspace?.gitRepository
              ? 'nav.sessions.workbenchRuntime.managedTask.gitRepository'
              : 'nav.sessions.workbenchRuntime.managedTask.notGitRepository')}
          </span>
          <span>
            {t(workspace?.trusted
              ? 'nav.sessions.workbenchRuntime.managedTask.trustConfirmed'
              : 'nav.sessions.workbenchRuntime.managedTask.trustRequired')}
          </span>
        </div>
      </div>

      <form className="halo-workbench-managed-task__form" onSubmit={handleCreateManagedTask}>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.managedTask.taskId')}</span>
          <input
            value={taskId}
            onChange={event => setTaskId(event.target.value)}
            maxLength={256}
            autoComplete="off"
            data-testid="workbench-managed-task-id"
          />
        </label>
        <label>
          <span>{t('nav.sessions.workbenchRuntime.managedTask.firstPrompt')}</span>
          <textarea
            ref={promptRef}
            value={firstPrompt}
            onChange={event => setFirstPrompt(event.target.value)}
            rows={3}
            maxLength={16 * 1024}
            data-testid="workbench-managed-task-prompt"
          />
        </label>
        <label className="halo-workbench-managed-task__trust-check">
          <input
            type="checkbox"
            checked={managedWorkspaceConfirmed}
            onChange={event => setManagedWorkspaceConfirmed(event.target.checked)}
            data-testid="workbench-managed-task-trust"
          />
          <span>{t('nav.sessions.workbenchRuntime.managedTask.trustCheckbox')}</span>
        </label>
        <button
          type="submit"
          disabled={!canCreateManagedTask}
          data-testid="workbench-managed-task-create"
        >
          {isCreating || isSendingFirstPrompt ? (
            <Loader2 size={14} className="is-spinning" aria-hidden="true" />
          ) : (
            <CheckCircle2 size={14} aria-hidden="true" />
          )}
          <span>{t('nav.sessions.workbenchRuntime.managedTask.create')}</span>
        </button>
      </form>

      {pendingFirstPrompt ? (
        <p className="halo-workbench-managed-task__pending" role="status">
          <Loader2 size={13} className="is-spinning" aria-hidden="true" />
          {t('nav.sessions.workbenchRuntime.managedTask.firstTurnPending')}
        </p>
      ) : null}

      {actionErrorKey ? (
        <p className="halo-workbench-managed-task__error" role="alert">{t(actionErrorKey)}</p>
      ) : null}
    </section>
  );
};

export default WorkbenchManagedTaskComposer;
