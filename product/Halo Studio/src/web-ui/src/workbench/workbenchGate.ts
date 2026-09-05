/**
 * Workbench view gate (M5, issue #57): decides whether the managed-mode main
 * view is the new strip workbench or the retained legacy workspace body.
 *
 * Default: strip workbench. Fallback: set `halo:workbench-view=legacy` in
 * sessionStorage (the command palette's 「回退经典视图」 does exactly that)
 * and reload — the legacy view stays fully functional for rollback.
 */

export const WORKBENCH_LEGACY_VIEW_STORAGE_KEY = 'halo:workbench-view';
export const WORKBENCH_LEGACY_VIEW_VALUE = 'legacy';

export function isStripWorkbenchEnabled(): boolean {
  try {
    return window.sessionStorage.getItem(WORKBENCH_LEGACY_VIEW_STORAGE_KEY) !== WORKBENCH_LEGACY_VIEW_VALUE;
  } catch {
    return true;
  }
}
