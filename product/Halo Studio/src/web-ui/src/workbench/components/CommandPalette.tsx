/**
 * CommandPalette — Spotlight-style command surface (M5, ADR-0076 surface 3;
 * ADR-0030 evolution entry).
 *
 * 任务创建（执行器选择【仅】出现在创建处，ADR-0078）、工作区跳转、会话内
 * 命令占位、Git 面板/设置入口、旧视图回退。⌘/Ctrl+K 由 WorkbenchShell 注册。
 */

import { useEffect, useMemo, useRef, useState } from 'react';

import {
  createTaskAtFocus,
  jumpToTask,
  openSurface,
  switchToLegacyWorkbenchView,
} from '../state/workbenchController';
import {
  WORKBENCH_EXECUTORS,
  WORKBENCH_EXECUTOR_IDS,
} from '../state/workbenchExecutors';
import { useWorkbenchRuntimeStore } from '../state/workbenchRuntimeStore';
import { useWorkbenchUiStore } from '../state/workbenchUiStore';
import styles from './CommandPalette.module.css';

interface PaletteCommand {
  id: string;
  title: string;
  hint?: string;
  run: () => void;
}

export function CommandPalette() {
  const open = useWorkbenchUiStore(s => s.paletteOpen);
  const setPaletteOpen = useWorkbenchUiStore(s => s.setPaletteOpen);
  const workspaceOrder = useWorkbenchRuntimeStore(s => s.workspaceOrder);
  const workspaces = useWorkbenchRuntimeStore(s => s.workspaces);

  const [query, setQuery] = useState('');
  const [step, setStep] = useState<'root' | 'executor'>('root');
  const [pendingTitle, setPendingTitle] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const [note, setNote] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) {
      setQuery('');
      setStep('root');
      setPendingTitle('');
      setActiveIndex(0);
      setNote(null);
      inputRef.current?.focus();
    }
  }, [open]);

  const commands = useMemo<PaletteCommand[]>(() => {
    if (step === 'executor') {
      return WORKBENCH_EXECUTOR_IDS.map(executorId => {
        const profile = WORKBENCH_EXECUTORS[executorId];
        return {
          id: `executor:${executorId}`,
          title: profile.displayName,
          hint: profile.capabilityNotes[0] ?? `${profile.transport} · 一次性批准流程`,
          run: () => {
            createTaskAtFocus({ title: pendingTitle, executor: executorId });
            setPaletteOpen(false);
          },
        };
      });
    }
    const root: PaletteCommand[] = [];
    root.push({
      id: 'task.create',
      title: query.trim() ? `新建任务：${query.trim()}` : '新建任务（选择执行器）',
      hint: '执行器选择仅在任务创建处（ADR-0078）',
      run: () => {
        setPendingTitle(query.trim());
        setStep('executor');
        setActiveIndex(0);
      },
    });
    for (const workspaceId of workspaceOrder) {
      const workspace = workspaces[workspaceId];
      if (!workspace) continue;
      root.push({
        id: `workspace:${workspaceId}`,
        title: `跳转到工作区：${workspace.displayName}`,
        hint: workspace.branch,
        run: () => {
          jumpToTask(workspaceId, workspace.taskOrder[0] ?? null);
          setPaletteOpen(false);
        },
      });
    }
    root.push(
      {
        id: 'surface.git',
        title: '打开 Git 面板',
        hint: '容器占位（ADR-0055–0063 语义对接后续票）',
        run: () => {
          openSurface('git');
          setPaletteOpen(false);
        },
      },
      {
        id: 'surface.settings',
        title: '打开设置',
        hint: '容器占位（模型/凭据、执行器默认、诊断导出、主题）',
        run: () => {
          openSurface('settings');
          setPaletteOpen(false);
        },
      },
      {
        id: 'session.commands',
        title: '会话内命令（/steer、/follow-up …）',
        hint: 'ADR-0030 演进占位',
        run: () => setNote('会话内命令为占位入口（ADR-0030 演进），将在后续票接入执行链。'),
      },
      {
        id: 'view.legacy',
        title: '回退经典视图（重新加载）',
        hint: '旧工作台保留，可随时回退',
        run: () => switchToLegacyWorkbenchView(),
      },
    );
    return root;
  }, [step, query, pendingTitle, workspaceOrder, workspaces, setPaletteOpen]);

  const normalizedQuery = query.trim().toLowerCase();
  const filtered = commands.filter(command =>
    `${command.title} ${command.hint ?? ''}`.toLowerCase().includes(normalizedQuery));
  const clampedIndex = Math.min(activeIndex, Math.max(0, filtered.length - 1));

  if (!open) return null;

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActiveIndex(Math.min(clampedIndex + 1, filtered.length - 1));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveIndex(Math.max(clampedIndex - 1, 0));
    } else if (event.key === 'Enter') {
      event.preventDefault();
      filtered[clampedIndex]?.run();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      setPaletteOpen(false);
    }
  };

  return (
    <div
      className={styles.overlay}
      data-testid="command-palette"
      role="dialog"
      aria-modal="true"
      aria-label="命令面板"
      onMouseDown={event => {
        if (event.target === event.currentTarget) setPaletteOpen(false);
      }}
    >
      <div className={styles.panel}>
        <input
          ref={inputRef}
          className={styles.input}
          value={query}
          placeholder={step === 'executor'
            ? '选择执行器（↑↓ 移动，Enter 创建）'
            : '输入命令或任务描述（⌘/Ctrl+K）'}
          onChange={event => {
            setQuery(event.target.value);
            setActiveIndex(0);
          }}
          onKeyDown={handleKeyDown}
          data-testid="command-palette-input"
        />
        <ul className={styles.list}>
          {filtered.map((command, index) => (
            <li key={command.id}>
              <button
                type="button"
                className={index === clampedIndex ? `${styles.item} ${styles.itemActive}` : styles.item}
                data-testid="command-palette-item"
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => command.run()}
              >
                <span className={styles.itemTitle}>{command.title}</span>
                {command.hint ? <span className={styles.itemHint}>{command.hint}</span> : null}
              </button>
            </li>
          ))}
          {filtered.length === 0 ? <li className={styles.itemHint}>无匹配命令</li> : null}
        </ul>
        {note ? <p className={styles.note} data-testid="command-palette-note">{note}</p> : null}
      </div>
    </div>
  );
}
