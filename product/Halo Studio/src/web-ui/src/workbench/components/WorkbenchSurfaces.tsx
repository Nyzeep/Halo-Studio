/**
 * WorkbenchSurfaces — Git panel / settings side containers (M5; issue #57).
 *
 * 本票只交付导航容器占位：Git 面板保留 ADR-0055–0063 用户驱动语义的对接位，
 * 设置保留模型/凭据（ADR-0064）、执行器默认、诊断导出与主题分区位。
 * 真实内容在后续票接入。
 */

import { useWorkbenchUiStore } from '../state/workbenchUiStore';
import styles from './WorkbenchSurfaces.module.css';

export function WorkbenchSurfaces() {
  const surface = useWorkbenchUiStore(s => s.openSurface);
  if (surface === 'none') return null;

  const title = surface === 'git' ? 'Git 面板' : '设置';
  const body = surface === 'git'
    ? 'Git 面板容器占位 —— ADR-0055–0063 的用户驱动语义在后续票对接。'
    : '设置容器占位 —— 模型/凭据（ADR-0064）、执行器默认、诊断导出与主题分区在后续票接入。';

  return (
    <aside className={styles.surface} data-testid="workbench-surface" data-surface={surface} aria-label={title}>
      <header className={styles.header}>
        <span className={styles.title}>{title}</span>
        <button
          type="button"
          className={styles.close}
          aria-label="关闭"
          onClick={() => useWorkbenchUiStore.getState().setOpenSurface('none')}
        >
          ✕
        </button>
      </header>
      <p className={styles.body}>{body}</p>
    </aside>
  );
}
