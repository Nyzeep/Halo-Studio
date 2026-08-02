import React from 'react';
import { AlertCircle, CircleDot, Loader2 } from 'lucide-react';
import { useStore } from 'zustand';

import { useI18n } from '@/infrastructure/i18n';
import {
  PI_RPC_ADAPTER_IDENTITY,
  workbenchRuntimeStore,
} from '@/infrastructure/workbench-runtime';

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
  const isSyncing = runtimeState.syncStatus === 'bootstrapping'
    || runtimeState.syncStatus === 'resyncing';

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
            {snapshot?.workspace?.displayName ?? t('nav.groups.sessions')}
          </span>
          <span className="bitfun-workbench-session-scene__phase">
            {snapshot?.phase ?? 'disconnected'}
          </span>
        </div>
        <span className="bitfun-workbench-session-scene__adapter">
          {snapshot?.adapter.identity ?? PI_RPC_ADAPTER_IDENTITY}
        </span>
      </header>

      <div className="bitfun-workbench-session-scene__body" aria-live="polite">
        {isSyncing && !snapshot ? (
          <div className="bitfun-workbench-session-scene__state" role="status">
            <Loader2 size={16} aria-hidden="true" />
            <span>{t('nav.sessions.loading')}</span>
          </div>
        ) : null}

        {runtimeState.syncStatus === 'failed' ? (
          <div className="bitfun-workbench-session-scene__state is-error" role="alert">
            <AlertCircle size={16} aria-hidden="true" />
            <span>{runtimeState.stableErrorCode ?? 'runtime_transport_unavailable'}</span>
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
            <span className="bitfun-workbench-session-scene__session-phase">
              {session.phase}
            </span>
          </article>
        ))}
      </div>
    </section>
  );
};

export default WorkbenchSessionScene;
