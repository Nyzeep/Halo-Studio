import { createWorkbenchRuntimeRequestId } from './client';
import { workbenchRuntimeStore } from './store';
import type {
  WorkbenchRuntimeIntent,
  WorkbenchRuntimeIntentReceipt,
} from './types';

/**
 * UI actions enter the runtime through one infrastructure seam. Components
 * provide an intent; request identity and the authoritative store remain
 * outside the scene layer.
 */
export const submitWorkbenchRuntimeIntent = (
  intent: WorkbenchRuntimeIntent,
): Promise<WorkbenchRuntimeIntentReceipt> => workbenchRuntimeStore.getState().submitIntent({
  requestId: createWorkbenchRuntimeRequestId(`workbench-${intent.type}`),
  intent,
});
