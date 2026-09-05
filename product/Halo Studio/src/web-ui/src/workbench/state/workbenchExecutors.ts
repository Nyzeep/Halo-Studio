/**
 * Managed executor registry (M5, ADR-0078; issue #57).
 *
 * The two real production adapters. This module is pure configuration — no
 * runtime facts, no credentials, no evidence — so both stores (Runtime and
 * UI) may import it. The UI must degrade honestly per the capability profile:
 * a capability an executor does not provide is never rendered as available.
 */

export type WorkbenchExecutorId = 'pi-rpc' | 'dsh-acp';

export interface WorkbenchExecutorProfile {
  id: WorkbenchExecutorId;
  displayName: string;
  /** Adapter identity as reported through the Runtime seam (ADR-0078). */
  adapterIdentity: string;
  transport: string;
  capabilities: {
    /** Mid-run steering (pi has it, DSH acp does not — never fake it). */
    steer: boolean;
    /** One-time approval flow (ADR-0012) is a hard requirement on both. */
    permissionFlow: 'allow-once';
  };
  /** Honest degradation notes surfaced verbatim in the task column. */
  capabilityNotes: string[];
}

export const WORKBENCH_EXECUTORS: Record<WorkbenchExecutorId, WorkbenchExecutorProfile> = {
  'pi-rpc': {
    id: 'pi-rpc',
    displayName: 'Pi RPC',
    adapterIdentity: 'pi-rpc-p0',
    transport: 'pi --mode rpc',
    capabilities: {
      steer: true,
      permissionFlow: 'allow-once',
    },
    capabilityNotes: [],
  },
  'dsh-acp': {
    id: 'dsh-acp',
    displayName: 'DSH acp',
    adapterIdentity: 'halo-dsh-acp-p0',
    transport: 'DSH acp profile',
    capabilities: {
      steer: false,
      permissionFlow: 'allow-once',
    },
    capabilityNotes: [
      'steer 不可用：DSH acp 档案未提供中途引导，能力如实降级',
    ],
  },
};

export const WORKBENCH_EXECUTOR_IDS: readonly WorkbenchExecutorId[] = [
  'pi-rpc',
  'dsh-acp',
];

export function getWorkbenchExecutor(id: WorkbenchExecutorId): WorkbenchExecutorProfile {
  return WORKBENCH_EXECUTORS[id];
}
