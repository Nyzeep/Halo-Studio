/**
 * TaskStrip — the horizontal stage: fixed-width columns on an endlessly
 * right-extending strip (M5, ADR-0076; issue #57).
 *
 * 手势 P0（#45，含对 #43 原型的修正）：
 *   - 触摸板双指横滚（deltaX）= 原生 overflow-x 横移条带；
 *   - 滚轮（deltaY）= 只滚列内会话流，到边后【不】链横移；
 *   - Shift+滚轮 = 条带横移。
 * 列固定宽（--workbench-column-width），新列插入焦点右侧零重排。
 */

import { useEffect, useRef } from 'react';

import { createTaskAtFocus } from '../state/workbenchController';
import { useWorkbenchRuntimeStore } from '../state/workbenchRuntimeStore';
import { useWorkbenchUiStore } from '../state/workbenchUiStore';
import { TaskColumn } from './TaskColumn';
import styles from './TaskStrip.module.css';

function prefersReducedMotion(): boolean {
  try {
    return typeof window.matchMedia === 'function'
      && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  } catch {
    return false;
  }
}

export function TaskStrip() {
  const activeWorkspaceId = useWorkbenchUiStore(s => s.activeWorkspaceId);
  const focusedTaskId = useWorkbenchUiStore(s =>
    (s.activeWorkspaceId ? s.focusedTaskIdByWorkspace[s.activeWorkspaceId] ?? null : null));
  const storedScrollLeft = useWorkbenchUiStore(s =>
    (s.activeWorkspaceId ? s.stripScrollLeftByWorkspace[s.activeWorkspaceId] ?? 0 : 0));
  const setStripScrollLeft = useWorkbenchUiStore(s => s.setStripScrollLeft);
  const workspace = useWorkbenchRuntimeStore(s =>
    (activeWorkspaceId ? s.workspaces[activeWorkspaceId] : undefined));

  const stripRef = useRef<HTMLDivElement | null>(null);
  const restoredWorkspaceRef = useRef<string | null>(null);

  // Restore the gesture transient once per workspace switch (not on every scroll).
  useEffect(() => {
    if (!stripRef.current || restoredWorkspaceRef.current === activeWorkspaceId) return;
    restoredWorkspaceRef.current = activeWorkspaceId;
    stripRef.current.scrollLeft = storedScrollLeft;
  }, [activeWorkspaceId, storedScrollLeft]);

  // Keep the focused column visible without moving any other column.
  useEffect(() => {
    if (!focusedTaskId || !stripRef.current) return;
    const element = stripRef.current.querySelector<HTMLElement>(`[data-task-id="${focusedTaskId}"]`);
    if (!element || typeof element.scrollIntoView !== 'function') return;
    element.scrollIntoView({
      block: 'nearest',
      inline: 'center',
      behavior: prefersReducedMotion() ? 'auto' : 'smooth',
    });
  }, [focusedTaskId]);

  useEffect(() => {
    const strip = stripRef.current;
    if (!strip) return;
    const onWheel = (event: WheelEvent) => {
      const multiplier = event.deltaMode === 1 ? 16 : 1;
      let deltaX = event.deltaX * multiplier;
      const deltaY = event.deltaY * multiplier;
      // Normalize Shift+滚轮（垂直量转横向量）。
      if (event.shiftKey && Math.abs(deltaX) < Math.abs(deltaY)) {
        deltaX = deltaY;
      }
      if (Math.abs(deltaX) > Math.abs(deltaY)) {
        if (event.shiftKey) {
          event.preventDefault();
          strip.scrollLeft += deltaX;
        }
        // Unshifted horizontal deltas (touchpad two-finger) use native overflow.
        return;
      }
      // 垂直滚轮：只滚列内，不链横移（ADR-0076 对原型的修正）。
      const scroller = (event.target as HTMLElement | null)?.closest<HTMLElement>('[data-column-scroller]');
      if (scroller) {
        const canScroll = deltaY > 0
          ? scroller.scrollTop + scroller.clientHeight < scroller.scrollHeight - 1
          : scroller.scrollTop > 1;
        if (canScroll) return;
      }
      event.preventDefault();
    };
    strip.addEventListener('wheel', onWheel, { passive: false });
    return () => strip.removeEventListener('wheel', onWheel);
  }, []);

  if (!workspace) {
    return <section className={styles.stage} data-testid="task-stage" />;
  }

  return (
    <section className={styles.stage} data-testid="task-stage">
      <div
        ref={stripRef}
        className={styles.strip}
        data-testid="task-strip"
        data-workspace-id={workspace.workspaceId}
        onScroll={event => setStripScrollLeft(workspace.workspaceId, event.currentTarget.scrollLeft)}
      >
        {workspace.taskOrder.map(taskId => {
          const task = workspace.tasks[taskId];
          if (!task) return null;
          return (
            <TaskColumn
              key={taskId}
              workspaceId={workspace.workspaceId}
              task={task}
              focused={taskId === focusedTaskId}
            />
          );
        })}
        {workspace.taskOrder.length === 0 ? (
          <div className={styles.emptyHint}>
            空工作区 —— niri 的「永远存在的空工作区」。
            <br />
            按 <b>n</b> 新建第一列，或 ⌘/Ctrl+K 打开命令面板。
          </div>
        ) : null}
        <button
          type="button"
          className={styles.newColumn}
          data-testid="strip-new"
          aria-label="新建任务列（追加到条带末尾）"
          onClick={() => createTaskAtFocus()}
        >
          ＋
        </button>
      </div>
    </section>
  );
}
