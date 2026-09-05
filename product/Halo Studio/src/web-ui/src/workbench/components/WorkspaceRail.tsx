/**
 * WorkspaceRail — the left rail of the strip workbench (M5, ADR-0076).
 *
 * niri 转译：每个 Git 工作区是一条独立条带（每显示器独立条带），轨道底部恒有
 * 「新建」位 —— 「永远存在一个空工作区」的视觉承诺。
 */

import {
  createWorkspaceFromRail,
  selectWorkspaceByIndex,
} from '../state/workbenchController';
import { useWorkbenchRuntimeStore } from '../state/workbenchRuntimeStore';
import { useWorkbenchUiStore } from '../state/workbenchUiStore';
import styles from './WorkspaceRail.module.css';

export function WorkspaceRail() {
  const workspaceOrder = useWorkbenchRuntimeStore(s => s.workspaceOrder);
  const workspaces = useWorkbenchRuntimeStore(s => s.workspaces);
  const activeWorkspaceId = useWorkbenchUiStore(s => s.activeWorkspaceId);

  return (
    <nav className={styles.rail} aria-label="工作区轨" data-testid="workspace-rail">
      {workspaceOrder.map((workspaceId, index) => {
        const workspace = workspaces[workspaceId];
        if (!workspace) return null;
        const active = workspaceId === activeWorkspaceId;
        return (
          <button
            key={workspaceId}
            type="button"
            className={active ? `${styles.item} ${styles.itemActive}` : styles.item}
            data-testid="workspace-rail-item"
            data-workspace-id={workspaceId}
            data-active={active ? 'true' : 'false'}
            aria-label={`切换到工作区 ${workspace.displayName}`}
            onClick={() => selectWorkspaceByIndex(index)}
          >
            <span className={styles.itemName}>{workspace.displayName}</span>
            <span className={styles.itemMeta}>
              {workspace.branch} · {workspace.taskOrder.length} 列
            </span>
          </button>
        );
      })}
      <button
        type="button"
        className={styles.newSlot}
        data-testid="workspace-rail-new"
        onClick={() => createWorkspaceFromRail()}
      >
        <span className={styles.itemName}>＋ 新建工作区</span>
        <span className={styles.itemMeta}>这里永远存在一个空位</span>
      </button>
      <p className={styles.hint}>每个工作区 = 一条独立条带，互不溢出</p>
    </nav>
  );
}
