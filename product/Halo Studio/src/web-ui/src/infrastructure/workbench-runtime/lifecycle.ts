import { isHaloLocalCodingScope, isTauriRuntime } from '@/infrastructure/runtime';
import { workspaceManager } from '@/infrastructure/services/business/workspaceManager';
import { WorkspaceKind } from '@/shared/types';

import { createWorkbenchRuntimeRequestId } from './client';
import { workbenchRuntimeStore } from './store';

/**
 * Finish the active Halo Workbench generation before the legacy workspace
 * projection changes. Non-Halo and browser callers are intentionally no-ops.
 */
export const submitWorkbenchRuntimeCloseIntent = async (): Promise<void> => {
  if (!(isHaloLocalCodingScope() && isTauriRuntime())) {
    return;
  }

  const activeWorkspace = workspaceManager.getState().currentWorkspace;
  if (!activeWorkspace || activeWorkspace.workspaceKind === WorkspaceKind.Remote) {
    return;
  }

  await workbenchRuntimeStore.getState().submitIntent({
    requestId: createWorkbenchRuntimeRequestId('close-workspace'),
    intent: { type: 'closeWorkspace' },
  });
};
