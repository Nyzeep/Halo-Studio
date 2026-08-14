import React, { useState } from 'react';
import { AlertCircle, CircleDot, Loader2 } from 'lucide-react';
import { useStore } from 'zustand';

import { useI18n } from '@/infrastructure/i18n';
import {
  selectWorkbenchRuntimeErrorMessageKey,
  selectWorkbenchRuntimeInterruptionReasonMessageKey,
  selectWorkbenchRuntimePhaseMessageKey,
  selectWorkbenchRuntimeSessionPhaseMessageKey,
  workbenchRuntimeStore,
} from '@/infrastructure/workbench-runtime';

import WorkbenchDeliveryReview from './WorkbenchDeliveryReview';
import WorkbenchManagedTaskComposer from './WorkbenchManagedTaskComposer';
import WorkbenchPermissionDecision from './WorkbenchPermissionDecision';
import './WorkbenchSessionScene.scss';

interface WorkbenchSessionSceneProps {
  isEntering?: boolean;
  isActive?: boolean;
}

const WorkbenchSessionScene: React.FC<WorkbenchSessionSceneProps> = ({
  isEntering = false,
  isActive = false,
}) => {
  const { t } = useI18n('common');
  const runtimeState = useStore(workbenchRuntimeStore);
  const snapshot = runtimeState.snapshot;
  const [newRunVersion, setNewRunVersion] = useState(0);
  const isSyncing = runtimeState.syncStatus === 'bootstrapping'
    || runtimeState.syncStatus === 'resyncing';
  const runtimePhaseMessageKey = selectWorkbenchRuntimePhaseMessageKey(runtimeState);

  return (
    <section
      className={[
        'bitfun-workbench-session-scene',
        isEntering && 'bitfun-workbench-session-scene--entering',
      ].filter(Boolean).join(' ')}
      aria-hidden={!isActive}
      data-testid="workbench-session-scene"
      data-runtime-phase={snapshot?.phase ?? 'disconnected'}
    >
      <header className="bitfun-workbench-session-scene__header">
        <div className="bitfun-workbench-session-scene__heading">
          <span className="bitfun-workbench-session-scene__workspace">
            {snapshot?.workspace?.displayName ?? t('nav.items.sessions')}
          </span>
          {snapshot?.workspace?.rootPath ? (
            <code className="bitfun-workbench-session-scene__root-path">
              {snapshot.workspace.rootPath}
            </code>
          ) : null}
          <span className="bitfun-workbench-session-scene__phase">
            {t(runtimePhaseMessageKey)}
          </span>
        </div>
        <span className="bitfun-workbench-session-scene__adapter">
          {snapshot?.adapter.identity ?? 'pi-rpc-p0'}
        </span>
      </header>

      <div className="bitfun-workbench-session-scene__body" aria-live="polite">
        <WorkbenchManagedTaskComposer newRunVersion={newRunVersion} />

        {isSyncing && !snapshot ? (
          <div className="bitfun-workbench-session-scene__state" role="status">
            <Loader2 size={16} aria-hidden="true" />
            <span>{t('nav.sessions.loading')}</span>
          </div>
        ) : null}

        {runtimeState.syncStatus === 'failed' ? (
          <div className="bitfun-workbench-session-scene__state is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t(selectWorkbenchRuntimeErrorMessageKey(runtimeState))}</span>
          </div>
        ) : null}

        {snapshot?.error ? (
          <div className="bitfun-workbench-session-scene__state is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{t('nav.sessions.workbenchRuntime.error')}</span>
          </div>
        ) : null}

        {snapshot && snapshot.sessions.length === 0 ? (
          <div className="bitfun-workbench-session-scene__state">
            <span>{t('nav.sessions.noSessions')}</span>
          </div>
        ) : null}

        {snapshot?.sessions.map((session, index) => (
          <article
            key={session.sessionId}
            className="bitfun-workbench-session-scene__session"
            data-session-id={session.sessionId}
            data-session-phase={session.phase}
          >
            <CircleDot size={15} aria-hidden="true" />
            <span className="bitfun-workbench-session-scene__session-name">
              {t('nav.sessions.newSession')} {index + 1}
            </span>
            <span className="bitfun-workbench-session-scene__session-mode">
              {session.mode === 'managed'
                ? t('nav.sessions.workbenchRuntime.sessionMode.managed')
                : t('nav.sessions.workbenchRuntime.sessionMode.standard')}
            </span>
            <span className="bitfun-workbench-session-scene__session-phase">
              {t(selectWorkbenchRuntimeSessionPhaseMessageKey(session.phase))}
            </span>
            {session.baseline?.canonicalRoot ? (
              <div className="bitfun-workbench-session-scene__baseline">
                <span>{t('nav.sessions.workbenchRuntime.baseline.root')}</span>
                <code>{session.baseline.canonicalRoot}</code>
                <span>
                  {t('nav.sessions.workbenchRuntime.baseline.changedFiles', {
                    count: session.baseline.existingChangedFiles.length,
                  })}
                </span>
              </div>
            ) : null}
            {(session.messages ?? []).length > 0 ? (
              <div className="bitfun-workbench-session-scene__messages">
                {(session.messages ?? []).map((message, messageIndex) => (
                  <div
                    key={`${session.sessionId}-message-${messageIndex}`}
                    className={`bitfun-workbench-session-scene__message is-${message.role}`}
                  >
                    <span className="bitfun-workbench-session-scene__message-role">
                      {t(message.role === 'user'
                        ? 'nav.sessions.workbenchRuntime.messageRole.user'
                        : 'nav.sessions.workbenchRuntime.messageRole.assistant')}
                    </span>
                    <p>{message.content}</p>
                  </div>
                ))}
              </div>
            ) : null}
            {(session.activities ?? []).length > 0 ? (
              <div className="bitfun-workbench-session-scene__activities">
                {(session.activities ?? []).map(activity => (
                  <div
                    key={`${session.sessionId}-${activity.activityId}`}
                    className={`bitfun-workbench-session-scene__activity${activity.isError ? ' is-error' : ''}`}
                  >
                    <span>{activity.label}</span>
                    <span>
                      {t(`nav.sessions.workbenchRuntime.activityStatus.${activity.status}`)}
                    </span>
                  </div>
                ))}
              </div>
            ) : null}
            {(snapshot.pendingOperations ?? [])
              .filter(operation => (
                operation.sessionId === session.sessionId
                && operation.phase === 'awaitingDecision'
              ))
              .map(operation => (
                <WorkbenchPermissionDecision
                  key={operation.operationId}
                  operation={operation}
                />
              ))}
            <WorkbenchDeliveryReview
              session={session}
              onStartNewRun={() => setNewRunVersion(version => version + 1)}
            />
            {session.phase === 'interrupted' || session.error ? (
              <div className="bitfun-workbench-session-scene__session-error" role="alert">
                <AlertCircle size={14} aria-hidden="true" />
                <span
                  data-testid={session.phase === 'interrupted' ? 'workbench-interruption-reason' : undefined}
                >
                  {session.phase === 'interrupted'
                    ? t(selectWorkbenchRuntimeInterruptionReasonMessageKey(session))
                    : t('nav.sessions.workbenchRuntime.sessionError')}
                </span>
              </div>
            ) : null}
          </article>
        ))}
      </div>
    </section>
  );
};

export default WorkbenchSessionScene;
