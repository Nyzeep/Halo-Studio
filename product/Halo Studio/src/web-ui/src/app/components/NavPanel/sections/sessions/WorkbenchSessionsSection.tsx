import React from 'react';
import { CircleHelp, Code2, Loader2 } from 'lucide-react';
import { useStore } from 'zustand';

import { useI18n } from '@/infrastructure/i18n';
import {
  selectWorkbenchRuntimeSessionNeedsDecision,
  selectWorkbenchRuntimeSessionsForWorkspace,
  workbenchRuntimeStore,
} from '@/infrastructure/workbench-runtime';

interface WorkbenchSessionsSectionProps {
  workspaceId: string;
  isActiveWorkspace?: boolean;
  showSessionModeIcon?: boolean;
}

const WorkbenchSessionsSection: React.FC<WorkbenchSessionsSectionProps> = ({
  workspaceId,
  isActiveWorkspace = true,
  showSessionModeIcon = true,
}) => {
  const { t } = useI18n('common');
  const runtimeState = useStore(workbenchRuntimeStore);
  const sessions = selectWorkbenchRuntimeSessionsForWorkspace(runtimeState, workspaceId);

  if (sessions.length === 0) {
    if (isActiveWorkspace && (
      runtimeState.syncStatus === 'bootstrapping'
      || runtimeState.syncStatus === 'resyncing'
    )) {
      return (
        <div className="bitfun-nav-panel__inline-list">
          <div className="bitfun-nav-panel__inline-loading">
            <Loader2 size={12} />
            <span>{t('nav.sessions.loading')}</span>
          </div>
        </div>
      );
    }
    return null;
  }

  return (
    <div className="bitfun-nav-panel__inline-list" data-testid="workbench-session-list">
      {sessions.map((session, index) => {
        const awaitingDecision = selectWorkbenchRuntimeSessionNeedsDecision(
          runtimeState,
          session.sessionId,
        );
        const isRunning = session.phase === 'creating'
          || session.phase === 'running'
          || session.phase === 'stopping';

        return (
          <div
            key={session.sessionId}
            className="bitfun-nav-panel__inline-item"
            data-testid="workbench-session-item"
            data-session-id={session.sessionId}
            data-session-phase={session.phase}
          >
            {showSessionModeIcon ? (
              <span className="bitfun-nav-panel__inline-item-icon-slot">
                {awaitingDecision ? (
                  <CircleHelp
                    size={14}
                    className="bitfun-nav-panel__inline-item-icon is-ask-user"
                    aria-hidden="true"
                  />
                ) : isRunning ? (
                  <Loader2
                    size={14}
                    className="bitfun-nav-panel__inline-item-icon is-running"
                    aria-hidden="true"
                  />
                ) : (
                  <Code2
                    size={14}
                    className="bitfun-nav-panel__inline-item-icon is-code"
                    aria-hidden="true"
                  />
                )}
              </span>
            ) : null}
            <span className="bitfun-nav-panel__inline-item-main">
              <span className="bitfun-nav-panel__inline-item-label">
                {t('nav.sessions.newSession')} {index + 1}
              </span>
              {awaitingDecision ? (
                <span className="bitfun-nav-panel__inline-item-attention-badge">
                  {t('nav.sessions.badgeNeedsConfirm')}
                </span>
              ) : null}
            </span>
          </div>
        );
      })}
    </div>
  );
};

export default WorkbenchSessionsSection;
