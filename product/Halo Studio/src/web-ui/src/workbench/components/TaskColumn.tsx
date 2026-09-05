/**
 * TaskColumn — one fixed-width column on the strip (M5, ADR-0076 surface 2).
 *
 * 列内结构：会话流（纵向堆叠，纵向增长）+ 工具活动 chips + Agent 操作请求卡
 * （两执行器统一渲染，ADR-0078）+ 交付审查区（证据快照/新鲜度/接受拒绝，
 * ADR-0047/0050 语义不变）。新列加入永不改变本列的位置与宽度。
 */

import type { WorkbenchRuntimeActivity } from '@/infrastructure/workbench-runtime/types';

import { getWorkbenchExecutor } from '../state/workbenchExecutors';
import type { WorkbenchTask } from '../state/workbenchProjectionTypes';
import {
  recordDeliveryDecision,
  resolveOperation,
} from '../state/workbenchController';
import {
  WORKBENCH_ACTIVITY_STATUS_LABEL,
  WORKBENCH_PHASE_PRESENTATION,
  formatEvidenceAge,
  type WorkbenchPhaseTone,
} from './workbenchPresentation';
import styles from './TaskColumn.module.css';

const TONE_CLASS: Record<WorkbenchPhaseTone, string> = {
  running: styles.toneRunning,
  waiting: styles.toneWaiting,
  review: styles.toneReview,
  accepted: styles.toneAccepted,
  failed: styles.toneFailed,
  idle: styles.toneIdle,
};

function activityChipClass(activity: WorkbenchRuntimeActivity): string {
  if (activity.status === 'failed' || activity.isError) return styles.chipFail;
  if (activity.status === 'completed') return styles.chipOk;
  return styles.chipRun;
}

interface TaskColumnProps {
  workspaceId: string;
  task: WorkbenchTask;
  focused: boolean;
}

export function TaskColumn({ workspaceId, task, focused }: TaskColumnProps) {
  const phase = WORKBENCH_PHASE_PRESENTATION[task.phase];
  const executor = getWorkbenchExecutor(task.executor);
  const columnClassName = focused ? `${styles.column} ${styles.columnFocused}` : styles.column;

  return (
    <article
      className={columnClassName}
      data-testid="task-column"
      data-task-id={task.taskId}
      data-focused={focused ? 'true' : 'false'}
      aria-label={`任务列：${task.title}`}
    >
      <header className={styles.header}>
        <span className={styles.title} title={task.title}>{task.title}</span>
        <span className={styles.executorBadge}>{executor.displayName}</span>
        <span className={`${styles.badge} ${TONE_CLASS[phase.tone]}`}>{phase.label}</span>
      </header>

      {executor.capabilityNotes.length > 0 ? (
        <p className={styles.capabilityNotes}>{executor.capabilityNotes.join('；')}</p>
      ) : null}

      <div className={styles.messages} data-column-scroller="messages">
        {task.messages.map((message, index) => (
          <div
            key={`${message.role}-${index}`}
            className={message.role === 'user' ? `${styles.message} ${styles.messageUser}` : styles.message}
          >
            <span className={styles.messageWho}>{message.role === 'user' ? '开发者' : 'Halo'}</span>
            <span className={styles.messageText}>{message.content}</span>
          </div>
        ))}
        {task.messages.length === 0 ? <span className={styles.streamEmpty}>会话尚未开始</span> : null}
      </div>

      <div className={styles.tools} data-column-scroller="tools">
        {task.activities.map(activity => (
          <span key={activity.activityId} className={`${styles.chip} ${activityChipClass(activity)}`}>
            <span className={styles.chipDot} aria-hidden="true" />
            {activity.label} · {WORKBENCH_ACTIVITY_STATUS_LABEL[activity.status]}
          </span>
        ))}
        {task.activities.length === 0 ? <span className={styles.streamEmpty}>尚无工具活动</span> : null}
      </div>

      {task.pendingOperation ? (
        <section className={styles.operationCard} data-testid="operation-request-card">
          <div className={styles.operationTitle}>
            Agent 操作请求 · 一次性批准（两执行器统一流程）
          </div>
          <div className={styles.operationTool}>{task.pendingOperation.toolName}</div>
          <p className={styles.operationArgs}>{task.pendingOperation.arguments}</p>
          {task.pendingOperation.riskLevel === 'highRisk' ? (
            <span className={`${styles.badge} ${TONE_CLASS.failed}`}>高风险</span>
          ) : null}
          <div className={styles.actionRow}>
            <button
              type="button"
              className={styles.primaryAction}
              onClick={() => resolveOperation(workspaceId, task.taskId, 'allowOnce')}
            >
              一次性批准
            </button>
            <button
              type="button"
              className={styles.secondaryAction}
              onClick={() => resolveOperation(workspaceId, task.taskId, 'deny')}
            >
              拒绝
            </button>
          </div>
        </section>
      ) : null}

      <footer className={styles.review}>
        <DeliveryReviewArea workspaceId={workspaceId} task={task} />
      </footer>
    </article>
  );
}

function DeliveryReviewArea({ workspaceId, task }: { workspaceId: string; task: WorkbenchTask }) {
  const review = task.deliveryReview;
  if (!review) {
    return (
      <div className={styles.reviewBox} data-testid="delivery-review" data-decision="none">
        <div className={styles.reviewTitle}>交付审查</div>
        <span>尚无交付物</span>
      </div>
    );
  }
  const boxClass = review.decision === 'accepted'
    ? `${styles.reviewBox} ${styles.reviewAccepted}`
    : review.decision === 'rejected'
      ? `${styles.reviewBox} ${styles.reviewRejected}`
      : `${styles.reviewBox} ${styles.reviewPending}`;

  return (
    <div
      className={boxClass}
      data-testid="delivery-review"
      data-decision={review.decision ?? 'pending'}
    >
      <div className={styles.reviewTitle}>交付审查</div>
      <p className={styles.reviewSummary}>{review.summary}</p>
      <dl className={styles.evidence}>
        <div className={styles.evidenceRow}>
          <dt>证据快照</dt>
          <dd>
            {review.evidence.head} · {formatEvidenceAge(review.evidence.capturedAtMs, Date.now())}
          </dd>
        </div>
        <div className={styles.evidenceRow}>
          <dt>变更文件</dt>
          <dd>{review.evidence.changedFiles.length} 个 · {review.evidence.diffPreview}</dd>
        </div>
        <div className={styles.evidenceRow}>
          <dt>验证</dt>
          <dd>{review.verificationResults}</dd>
        </div>
        <div className={styles.evidenceRow}>
          <dt>结论</dt>
          <dd>{review.runConclusion}</dd>
        </div>
      </dl>
      {review.decision === null ? (
        <div className={styles.actionRow}>
          <button
            type="button"
            className={styles.primaryAction}
            onClick={() => recordDeliveryDecision(workspaceId, task.taskId, 'accepted')}
          >
            接受交付
          </button>
          <button
            type="button"
            className={styles.secondaryAction}
            onClick={() => recordDeliveryDecision(workspaceId, task.taskId, 'rejected')}
          >
            拒绝交付
          </button>
        </div>
      ) : (
        <span>开发者决定：{review.decision === 'accepted' ? '已接受' : '已拒绝'}</span>
      )}
    </div>
  );
}
