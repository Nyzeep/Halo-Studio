import React, { useState } from 'react';
import { CheckCircle2, FileDiff, XCircle } from 'lucide-react';

import { useI18n } from '@/infrastructure/i18n';
import {
  submitWorkbenchRuntimeIntent,
  type WorkbenchRuntimeSession,
} from '@/infrastructure/workbench-runtime';

import './WorkbenchDeliveryReview.scss';

interface WorkbenchDeliveryReviewProps {
  session: WorkbenchRuntimeSession;
}

const WorkbenchDeliveryReview: React.FC<WorkbenchDeliveryReviewProps> = ({ session }) => {
  const { t } = useI18n('common');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (session.mode !== 'managed') return null;

  const review = session.deliveryReview;
  if (session.phase === 'waitingDeveloper') {
    return (
      <div className="bitfun-workbench-delivery-review" data-testid="workbench-delivery-finish">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            setError(null);
            void submitWorkbenchRuntimeIntent({
              type: 'finishAndReview',
              sessionId: session.sessionId,
            })
              .catch(() => setError('nav.sessions.workbenchRuntime.deliveryReview.actionFailed'))
              .finally(() => setBusy(false));
          }}
          data-testid="workbench-delivery-finish-button"
        >
          <FileDiff size={14} aria-hidden="true" />
          <span>{t('nav.sessions.workbenchRuntime.deliveryReview.finishAndReview')}</span>
        </button>
        {error ? (
          <span className="bitfun-workbench-delivery-review__error" role="alert">{t(error)}</span>
        ) : null}
      </div>
    );
  }

  if (session.phase !== 'reviewing' || !review) return null;

  return (
    <section
      className="bitfun-workbench-delivery-review"
      data-testid="workbench-delivery-review"
    >
      <header className="bitfun-workbench-delivery-review__header">
        <FileDiff size={15} aria-hidden="true" />
        <span>{t('nav.sessions.workbenchRuntime.deliveryReview.title')}</span>
        <span className="bitfun-workbench-delivery-review__freshness">
          {t('nav.sessions.workbenchRuntime.deliveryReview.freshness')}
          {': '}
          {new Date(review.evidence.capturedAtMs).toLocaleString()}
        </span>
      </header>

      <div className="bitfun-workbench-delivery-review__grid">
        <section>
          <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.summary')}</h3>
          <p>{review.summary || '—'}</p>
        </section>
        <section>
          <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.verificationResults')}</h3>
          <p>{review.verificationResults || '—'}</p>
        </section>
        <section>
          <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.runConclusion')}</h3>
          <p>{review.runConclusion || '—'}</p>
        </section>
      </div>

      <section>
        <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.changedFiles')}</h3>
        <ul className="bitfun-workbench-delivery-review__files">
          {review.evidence.changedFiles.map(file => (
            <li key={file}>{file}</li>
          ))}
        </ul>
      </section>

      {review.evidence.attribution.length > 0 ? (
        <section>
          <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.attribution')}</h3>
          <ul className="bitfun-workbench-delivery-review__attribution">
            {review.evidence.attribution.map((item, index) => (
              <li key={`${item.path}-${index}`}>
                <code>{item.path}</code>
                <span>
                  {t(`nav.sessions.workbenchRuntime.deliveryReview.${item.kind}`)}
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <section>
        <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.diffPreview')}</h3>
        <pre className="bitfun-workbench-delivery-review__diff" data-testid="workbench-delivery-diff">
          {review.evidence.diffPreview || '—'}
        </pre>
      </section>

      <div className="bitfun-workbench-delivery-review__actions">
        <button
          type="button"
          disabled={busy || review.decision !== null}
          onClick={() => {
            setBusy(true);
            setError(null);
            void submitWorkbenchRuntimeIntent({
              type: 'acceptDelivery',
              sessionId: session.sessionId,
            })
              .catch(() => setError('nav.sessions.workbenchRuntime.deliveryReview.actionFailed'))
              .finally(() => setBusy(false));
          }}
          data-testid="workbench-delivery-accept"
        >
          <CheckCircle2 size={14} aria-hidden="true" />
          <span>{t('nav.sessions.workbenchRuntime.deliveryReview.accept')}</span>
        </button>
        <button
          type="button"
          disabled={busy || review.decision !== null}
          onClick={() => {
            setBusy(true);
            setError(null);
            void submitWorkbenchRuntimeIntent({
              type: 'rejectDelivery',
              sessionId: session.sessionId,
            })
              .catch(() => setError('nav.sessions.workbenchRuntime.deliveryReview.actionFailed'))
              .finally(() => setBusy(false));
          }}
          data-testid="workbench-delivery-reject"
        >
          <XCircle size={14} aria-hidden="true" />
          <span>{t('nav.sessions.workbenchRuntime.deliveryReview.reject')}</span>
        </button>
        {error ? (
          <span className="bitfun-workbench-delivery-review__error" role="alert">{t(error)}</span>
        ) : null}
      </div>
    </section>
  );
};

export default WorkbenchDeliveryReview;