/**
 * Strip workbench view root (M5, ADR-0076; issue #57).
 *
 * Mounted by AppLayout as the managed-mode main view when the workbench gate
 * allows it (./workbenchGate). Starts the M5 mock runtime driver once; the
 * real Tauri driver later replaces the driver, not this view.
 */

import { useEffect } from 'react';

import { WorkbenchShell } from './components/WorkbenchShell';
import { ensureRuntimeStarted } from './state/workbenchController';

export default function StripWorkbenchView() {
  useEffect(() => {
    ensureRuntimeStarted();
  }, []);

  return <WorkbenchShell />;
}
