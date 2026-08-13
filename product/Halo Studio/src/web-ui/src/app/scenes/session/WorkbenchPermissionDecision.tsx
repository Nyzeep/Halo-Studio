import React from 'react';

import { useI18n } from '@/infrastructure/i18n';
import { submitWorkbenchRuntimeIntent } from '@/infrastructure/workbench-runtime';
import type { WorkbenchRuntimePendingOperation } from '@/infrastructure/workbench-runtime';

import './WorkbenchPermissionDecision.scss';

interface WorkbenchPermissionDecisionProps {
  operation: WorkbenchRuntimePendingOperation;
}

/**
 * One-time allow/deny control for a pending Pi tool permission. The runtime
 * only supports "allow this once" and "deny"; no permanent, session-level, or
 * free-text approval surface exists here.
 */
const WorkbenchPermissionDecision: React.FC<WorkbenchPermissionDecisionProps> = ({
  operation,
}) => {
  const { t } = useI18n('common');

  return (
    <div
      className={[
        'bitfun-workbench-permission-decision',
        operation.riskLevel === 'highRisk' ? 'is-high-risk' : '',
      ].filter(Boolean).join(' ')}
      data-testid="workbench-permission-decision"
      data-risk-level={operation.riskLevel}
    >
      <div className="bitfun-workbench-permission-decision__summary">
        <code className="bitfun-workbench-permission-decision__tool">
          {operation.toolName}
        </code>
        {operation.arguments ? (
          <code className="bitfun-workbench-permission-decision__arguments">
            {operation.arguments}
          </code>
        ) : null}
        {operation.riskLevel === 'highRisk' ? (
          <span className="bitfun-workbench-permission-decision__risk">
            {t('nav.sessions.workbenchRuntime.permission.highRisk')}
          </span>
        ) : null}
      </div>
      <div className="bitfun-workbench-permission-decision__actions">
        <button
          type="button"
          data-testid="workbench-permission-allow"
          onClick={() => submitWorkbenchRuntimeIntent({
            type: 'resolveOperation',
            operationId: operation.operationId,
            decision: { type: 'allowOnce' },
          })}
        >
          {t('nav.sessions.workbenchRuntime.permission.allowOnce')}
        </button>
        <button
          type="button"
          data-testid="workbench-permission-deny"
          onClick={() => submitWorkbenchRuntimeIntent({
            type: 'resolveOperation',
            operationId: operation.operationId,
            decision: { type: 'deny' },
          })}
        >
          {t('nav.sessions.workbenchRuntime.permission.deny')}
        </button>
      </div>
    </div>
  );
};

export default WorkbenchPermissionDecision;
