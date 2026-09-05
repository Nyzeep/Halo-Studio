/**
 * WorkbenchShell — the strip workbench root surface (M5, ADR-0076; issue #57).
 *
 * Owns the global keyboard set (←→ 焦点、n 新建、o Overview、1..9 工作区、
 * Esc 退层、⌘/Ctrl+K 命令面板) and composes rail + strip/overview +
 * palette + side surfaces. All cross-store interaction goes through
 * workbenchController functions — the shell never wires the stores directly.
 */

import { useEffect } from 'react';

import {
  confirmOverviewSelection,
  createTaskAtFocus,
  ensureWorkspaceFocus,
  handleEscape,
  moveFocus,
  moveOverviewSelection,
  selectWorkspaceByIndex,
  toggleOverview,
} from '../state/workbenchController';
import { useWorkbenchRuntimeStore } from '../state/workbenchRuntimeStore';
import { useWorkbenchUiStore } from '../state/workbenchUiStore';
import { CommandPalette } from './CommandPalette';
import { Overview } from './Overview';
import { TaskStrip } from './TaskStrip';
import { WorkbenchSurfaces } from './WorkbenchSurfaces';
import { WorkspaceRail } from './WorkspaceRail';
import styles from './WorkbenchShell.module.css';

export function WorkbenchShell() {
  const workspaceOrder = useWorkbenchRuntimeStore(s => s.workspaceOrder);
  const workspaces = useWorkbenchRuntimeStore(s => s.workspaces);
  const runtimePhase = useWorkbenchRuntimeStore(s => s.runtimePhase);
  const activeWorkspaceId = useWorkbenchUiStore(s => s.activeWorkspaceId);
  const focusedTaskId = useWorkbenchUiStore(s =>
    (s.activeWorkspaceId ? s.focusedTaskIdByWorkspace[s.activeWorkspaceId] ?? null : null));
  const overviewOpen = useWorkbenchUiStore(s => s.overviewOpen);

  // Auto-select the first projected workspace and keep focus on a real task.
  useEffect(() => {
    if (workspaceOrder.length === 0) return;
    const ui = useWorkbenchUiStore.getState();
    const target = !ui.activeWorkspaceId || !workspaces[ui.activeWorkspaceId]
      ? workspaceOrder[0]
      : ui.activeWorkspaceId;
    if (target !== ui.activeWorkspaceId) ui.setActiveWorkspace(target);
    ensureWorkspaceFocus(target);
  }, [workspaceOrder, workspaces]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        const ui = useWorkbenchUiStore.getState();
        ui.setPaletteOpen(!ui.paletteOpen);
        return;
      }
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (
        target
        && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)
      ) {
        return;
      }
      if (useWorkbenchUiStore.getState().paletteOpen) return; // palette owns plain keys

      const key = event.key;
      if (overviewOpen) {
        if (key === 'ArrowLeft') {
          event.preventDefault();
          moveOverviewSelection(-1);
        } else if (key === 'ArrowRight') {
          event.preventDefault();
          moveOverviewSelection(1);
        } else if (key === 'Enter') {
          event.preventDefault();
          confirmOverviewSelection();
        } else if (key === 'Escape' || key === 'o' || key === 'O') {
          event.preventDefault();
          toggleOverview(false);
        } else if (/^[1-9]$/.test(key)) {
          event.preventDefault();
          selectWorkspaceByIndex(Number(key) - 1);
        }
        return;
      }

      if (key === 'ArrowLeft') {
        event.preventDefault();
        moveFocus(-1);
      } else if (key === 'ArrowRight') {
        event.preventDefault();
        moveFocus(1);
      } else if (key === 'n' || key === 'N') {
        event.preventDefault();
        createTaskAtFocus();
      } else if (key === 'o' || key === 'O') {
        event.preventDefault();
        toggleOverview(true);
      } else if (key === 'Escape') {
        handleEscape();
      } else if (/^[1-9]$/.test(key)) {
        event.preventDefault();
        selectWorkspaceByIndex(Number(key) - 1);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [overviewOpen]);

  const activeWorkspace = activeWorkspaceId ? workspaces[activeWorkspaceId] : undefined;
  const focusIndex = activeWorkspace && focusedTaskId
    ? activeWorkspace.taskOrder.indexOf(focusedTaskId) + 1
    : 0;
  const hud = overviewOpen
    ? `Overview · ${workspaceOrder.length} 个工作区`
    : activeWorkspace
      ? `工作区 ${activeWorkspace.displayName} · 列 ${activeWorkspace.taskOrder.length ? `${focusIndex}/${activeWorkspace.taskOrder.length}` : '0'} · 焦点 ${focusedTaskId ? activeWorkspace.tasks[focusedTaskId]?.title ?? '—' : '（空条带）'}`
      : '等待 Runtime 投影…';

  return (
    <div className={styles.shell} data-testid="workbench-shell">
      <header className={styles.topbar}>
        <span className={styles.title}>Halo Studio · 条带工作台</span>
        <span className={styles.hud} data-testid="workbench-hud">{hud}</span>
        <span className={styles.link} data-testid="workbench-link">Runtime {runtimePhase}</span>
      </header>
      <div className={styles.body}>
        <WorkspaceRail />
        {overviewOpen ? <Overview /> : <TaskStrip />}
      </div>
      <WorkbenchSurfaces />
      <CommandPalette />
    </div>
  );
}
