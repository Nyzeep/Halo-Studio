/**
 * Shared presentation vocabulary for the strip workbench (M5; issue #57).
 *
 * Maps the Runtime phase vocabulary onto DMS/MD3 token tones and zh labels.
 * The token layer has no MD3 "tertiary" role, so the prototype's tertiary
 * review tone maps onto the secondary container and "waiting" onto warning —
 * a documented, honest mapping rather than a lookalike token.
 */

import type { WorkbenchTaskPhase } from '../state/workbenchProjectionTypes';

export type WorkbenchPhaseTone = 'running' | 'waiting' | 'review' | 'accepted' | 'failed' | 'idle';

export interface WorkbenchPhasePresentation {
  label: string;
  tone: WorkbenchPhaseTone;
}

export const WORKBENCH_PHASE_PRESENTATION: Record<WorkbenchTaskPhase, WorkbenchPhasePresentation> = {
  creating: { label: '启动中', tone: 'running' },
  idle: { label: '空闲', tone: 'idle' },
  running: { label: '运行中', tone: 'running' },
  waitingDeveloper: { label: '等待开发者', tone: 'waiting' },
  reviewing: { label: '待审查', tone: 'review' },
  interrupted: { label: '已中断', tone: 'failed' },
  stopping: { label: '停止中', tone: 'idle' },
  ended: { label: '已结束', tone: 'accepted' },
  failed: { label: '失败', tone: 'failed' },
};

export const WORKBENCH_ACTIVITY_STATUS_LABEL: Record<WorkbenchActivityStatus, string> = {
  started: '运行中',
  updated: '运行中',
  completed: '完成',
  failed: '失败',
};

type WorkbenchActivityStatus = 'started' | 'updated' | 'completed' | 'failed';

/** Evidence freshness line (ADR-0050: freshness is always rendered, never hidden). */
export function formatEvidenceAge(capturedAtMs: number, nowMs: number): string {
  const deltaMinutes = Math.floor(Math.max(0, nowMs - capturedAtMs) / 60000);
  if (deltaMinutes < 1) return '刚刚';
  if (deltaMinutes < 60) return `${deltaMinutes} 分钟前`;
  const hours = Math.floor(deltaMinutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return `${Math.floor(hours / 24)} 天前`;
}
