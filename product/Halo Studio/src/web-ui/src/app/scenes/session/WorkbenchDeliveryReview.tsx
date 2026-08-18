import React, { useState } from 'react';
import { CheckCircle2, FileDiff, PauseCircle, PlayCircle, XCircle } from 'lucide-react';

import { useI18n } from '@/infrastructure/i18n';
import {
  submitWorkbenchRuntimeIntent,
  type WorkbenchRuntimeSession,
} from '@/infrastructure/workbench-runtime';

import './WorkbenchDeliveryReview.scss';

interface WorkbenchDeliveryReviewProps {
  session: WorkbenchRuntimeSession;
  onStartNewRun: () => void;
}

const WorkbenchDeliveryReview: React.FC<WorkbenchDeliveryReviewProps> = ({
  session,
  onStartNewRun,
}) => {
  const { t, formatDate } = useI18n('common');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [keptCurrentState, setKeptCurrentState] = useState(false);

  if (session.mode !== 'managed') return null;

  const review = session.deliveryReview;

  const startNewRun = () => {
    setKeptCurrentState(false);
    onStartNewRun();
  };

  const finishAndReview = () => {
    setBusy(true);
    setError(null);
    void submitWorkbenchRuntimeIntent({
      type: 'finishAndReview',
      sessionId: session.sessionId,
    })
      .catch(() => setError('nav.sessions.workbenchRuntime.deliveryReview.actionFailed'))
      .finally(() => setBusy(false));
  };

  if (session.phase === 'interrupted' && !review) {
    return (
      <div className="halo-workbench-delivery-review" data-testid="workbench-interruption-actions">
        <div className="halo-workbench-delivery-review__actions">
          <button
            type="button"
            disabled={busy}
            onClick={startNewRun}
            data-testid="workbench-interruption-new-run"
          >
            <PlayCircle size={14} aria-hidden="true" />
            <span>{t('nav.sessions.workbenchRuntime.interruptionDisposition.newRun')}</span>
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => setKeptCurrentState(true)}
            data-testid="workbench-interruption-keep-current"
          >
            <PauseCircle size={14} aria-hidden="true" />
            <span>{t('nav.sessions.workbenchRuntime.interruptionDisposition.keepCurrent')}</span>
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={finishAndReview}
            data-testid="workbench-interruption-review"
          >
            <FileDiff size={14} aria-hidden="true" />
            <span>{t('nav.sessions.workbenchRuntime.interruptionDisposition.review')}</span>
          </button>
        </div>
        {keptCurrentState ? (
          <span className="halo-workbench-delivery-review__status" role="status">
            {t('nav.sessions.workbenchRuntime.interruptionDisposition.kept')}
          </span>
        ) : null}
        {error ? (
          <span className="halo-workbench-delivery-review__error" role="alert">{t(error)}</span>
        ) : null}
      </div>
    );
  }

  if (session.phase === 'waitingDeveloper') {
    return (
      <div className="halo-workbench-delivery-review" data-testid="workbench-delivery-finish">
        <button
          type="button"
          disabled={busy}
          onClick={finishAndReview}
          data-testid="workbench-delivery-finish-button"
        >
          <FileDiff size={14} aria-hidden="true" />
          <span>{t('nav.sessions.workbenchRuntime.deliveryReview.finishAndReview')}</span>
        </button>
        {error ? (
          <span className="halo-workbench-delivery-review__error" role="alert">{t(error)}</span>
        ) : null}
      </div>
    );
  }

  if ((session.phase !== 'reviewing' && session.phase !== 'interrupted') || !review) return null;

  return (
    <section
      className="halo-workbench-delivery-review"
      data-testid="workbench-delivery-review"
    >
      <header className="halo-workbench-delivery-review__header">
        <FileDiff size={15} aria-hidden="true" />
        <span>{t('nav.sessions.workbenchRuntime.deliveryReview.title')}</span>
        <span className="halo-workbench-delivery-review__freshness">
          {t('nav.sessions.workbenchRuntime.deliveryReview.freshness')}
          {': '}
          {formatDate(review.evidence.capturedAtMs, {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
          })}
        </span>
      </header>

      <div className="halo-workbench-delivery-review__grid">
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
        <ul className="halo-workbench-delivery-review__files">
          {review.evidence.changedFiles.map(file => (
            <li key={file}>{file}</li>
          ))}
        </ul>
      </section>

      {review.evidence.attribution.length > 0 ? (
        <section>
          <h3>{t('nav.sessions.workbenchRuntime.deliveryReview.attribution')}</h3>
          <ul className="halo-workbench-delivery-review__attribution">
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
        <pre className="halo-workbench-delivery-review__diff" data-testid="workbench-delivery-diff">
          {review.evidence.diffPreview || '—'}
        </pre>
      </section>

      <div className="halo-workbench-delivery-review__actions">
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
          <span className="halo-workbench-delivery-review__error" role="alert">{t(error)}</span>
        ) : null}
      </div>
    </section>
  );
};

export default WorkbenchDeliveryReview;
