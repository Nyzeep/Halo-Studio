/**
 * Overview — the zoomed-out navigation layer (M5, ADR-0076 surface 1).
 *
 * 按工作区分组、状态色 + 标题截断；极端列数分页（每页 OVERVIEW_PAGE_SIZE 列，
 * 页脚 ‹ › 与指示器，键盘选择跨页时页面自动跟随）。逐条带测宽 scale 适配，
 * 与 #43 原型一致。
 */

import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';

import {
  OVERVIEW_PAGE_SIZE,
  jumpToTask,
  moveOverviewPage,
} from '../state/workbenchController';
import type { WorkbenchWorkspace } from '../state/workbenchProjectionTypes';
import { useWorkbenchRuntimeStore } from '../state/workbenchRuntimeStore';
import { useWorkbenchUiStore } from '../state/workbenchUiStore';
import {
  WORKBENCH_PHASE_PRESENTATION,
  type WorkbenchPhaseTone,
} from './workbenchPresentation';
import styles from './Overview.module.css';

const TONE_CLASS: Record<WorkbenchPhaseTone, string> = {
  running: styles.toneRunning,
  waiting: styles.toneWaiting,
  review: styles.toneReview,
  accepted: styles.toneAccepted,
  failed: styles.toneFailed,
  idle: styles.toneIdle,
};

interface OverviewRowProps {
  workspace: WorkbenchWorkspace;
  selectedTaskId: string | null;
  pageIndex: number;
}

function OverviewRow({ workspace, selectedTaskId, pageIndex }: OverviewRowProps) {
  const clipRef = useRef<HTMLDivElement | null>(null);
  const stripRef = useRef<HTMLDivElement | null>(null);

  const applyScale = useCallback(() => {
    const clip = clipRef.current;
    const strip = stripRef.current;
    if (!clip || !strip) return;
    const width = strip.scrollWidth || 1;
    const available = clip.clientWidth || 1;
    const scale = Math.min(0.9, available / width);
    strip.style.transform = `scale(${scale})`;
    clip.style.height = `${Math.round(strip.offsetHeight * scale)}px`;
    clip.style.width = `${Math.round(width * scale)}px`;
  }, []);

  useLayoutEffect(() => {
    applyScale();
  }, [applyScale, pageIndex, workspace.taskOrder.length]);

  useEffect(() => {
    window.addEventListener('resize', applyScale);
    return () => window.removeEventListener('resize', applyScale);
  }, [applyScale]);

  const pageCount = Math.max(1, Math.ceil(workspace.taskOrder.length / OVERVIEW_PAGE_SIZE));
  const safePage = Math.min(pageIndex, pageCount - 1);
  const pageTasks = workspace.taskOrder.slice(
    safePage * OVERVIEW_PAGE_SIZE,
    (safePage + 1) * OVERVIEW_PAGE_SIZE,
  );

  return (
    <section className={styles.row} data-testid="overview-row" data-workspace-id={workspace.workspaceId}>
      <div className={styles.rowLabel}>
        <span className={styles.rowTitle}>
          <b>{workspace.displayName}</b> · {workspace.branch} · {workspace.taskOrder.length} 列
        </span>
        {pageCount > 1 ? (
          <span className={styles.pageControls}>
            <button type="button" aria-label="上一页" onClick={() => moveOverviewPage(workspace.workspaceId, -1)}>
              ‹
            </button>
            <span data-testid="overview-page-indicator">第 {safePage + 1}/{pageCount} 页</span>
            <button type="button" aria-label="下一页" onClick={() => moveOverviewPage(workspace.workspaceId, 1)}>
              ›
            </button>
          </span>
        ) : null}
      </div>
      <div className={styles.clip} ref={clipRef}>
        <div className={styles.strip} ref={stripRef} data-testid="overview-strip">
          {workspace.taskOrder.length === 0 ? (
            <div className={styles.empty}>空工作区（永远存在的空位）</div>
          ) : pageTasks.map(taskId => {
            const task = workspace.tasks[taskId];
            if (!task) return null;
            const phase = WORKBENCH_PHASE_PRESENTATION[task.phase];
            const cellClass = taskId === selectedTaskId
              ? `${styles.cell} ${styles.cellSelected}`
              : styles.cell;
            return (
              <button
                key={taskId}
                type="button"
                className={cellClass}
                data-testid="overview-cell"
                data-task-id={taskId}
                onClick={() => jumpToTask(workspace.workspaceId, taskId)}
              >
                <span className={styles.cellTitle} title={task.title}>{task.title}</span>
                <span className={`${styles.badge} ${TONE_CLASS[phase.tone]}`}>{phase.label}</span>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}

export function Overview() {
  const workspaceOrder = useWorkbenchRuntimeStore(s => s.workspaceOrder);
  const workspaces = useWorkbenchRuntimeStore(s => s.workspaces);
  const selectedTaskId = useWorkbenchUiStore(s => s.overviewSelectedTaskId);
  const pageByWorkspace = useWorkbenchUiStore(s => s.overviewPageByWorkspace);

  return (
    <section className={styles.overview} data-testid="overview">
      <p className={styles.overviewHint}>
        Overview —— 所有条带缩远呈现；←→ 选择，Enter 跳回，Esc 退出
      </p>
      {workspaceOrder.map(workspaceId => {
        const workspace = workspaces[workspaceId];
        if (!workspace) return null;
        return (
          <OverviewRow
            key={workspaceId}
            workspace={workspace}
            selectedTaskId={selectedTaskId}
            pageIndex={pageByWorkspace[workspaceId] ?? 0}
          />
        );
      })}
    </section>
  );
}
