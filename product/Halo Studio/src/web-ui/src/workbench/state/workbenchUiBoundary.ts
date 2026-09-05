/**
 * Compile-time boundary assertion for the dual-store split (M5, ADR-0076;
 * issue #57 acceptance #4).
 *
 * If WorkbenchUiState ever grows a key that carries runtime facts,
 * credentials or delivery evidence, `LeakedFactKeys` stops being `never` and
 * the exported constant below fails to type-check.
 */

import type { WorkbenchUiState } from './workbenchUiStore';

/**
 * Key vocabulary that indicates fact leakage. `taskOrder`/`tasks` are listed
 * too: the UI store may reference task/workspace IDS, never ordering arrays
 * or task bodies (those belong to the Runtime projection).
 */
type WorkbenchFactKey =
  | 'messages'
  | 'activities'
  | 'pendingOperation'
  | 'deliveryReview'
  | 'evidence'
  | 'workingTreeFingerprint'
  | 'diffPreview'
  | 'credential'
  | 'sequence'
  | 'workspaces'
  | 'workspaceOrder'
  | 'taskOrder'
  | 'tasks'
  | 'eventBuffer';

type LeakedFactKeys = Extract<keyof WorkbenchUiState, WorkbenchFactKey>;
type UiStoreCarriesNoFacts = [LeakedFactKeys] extends [never] ? true : false;

export const WORKBENCH_UI_STORE_BOUNDARY: UiStoreCarriesNoFacts = true;
