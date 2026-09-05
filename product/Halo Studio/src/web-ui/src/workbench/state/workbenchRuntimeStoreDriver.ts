/**
 * Driver seam for the strip workbench Runtime store (M5, ADR-0080; issue #57).
 *
 * The store is a pure projection: every mutation enters through
 * `ingestEvent`. A driver is the only producer of events. The mock driver
 * (./workbenchRuntimeMockDriver) emits the same event vocabulary the future
 * Tauri driver will, so swapping drivers never rewrites the store or the UI.
 */

import type { WorkbenchProjectionEvent, WorkbenchRuntimeLinkKind } from './workbenchProjectionTypes';

/** The store side of the seam: a driver may only push events. */
export interface WorkbenchRuntimeDriverHost {
  ingestEvent: (event: WorkbenchProjectionEvent) => void;
}

export interface WorkbenchRuntimeDriver {
  /** Transport identity; matches WorkbenchRuntimeLinkStatus.kind. */
  readonly kind: WorkbenchRuntimeLinkKind;
  start: (host: WorkbenchRuntimeDriverHost) => void;
  stop: () => void;
}
