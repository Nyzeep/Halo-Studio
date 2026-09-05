/**
 * WorkbenchUiStore — presentation/transient state for the strip workbench
 * (M5, ADR-0076/0077; issue #57).
 *
 * STRICT BOUNDARY (M5 acceptance #4): this store holds focus, overview,
 * palette, panel and gesture-transient state only. It must never hold runtime
 * facts, credentials or delivery evidence — not even derived copies. The
 * compile-time assertion lives in ./workbenchUiBoundary.ts and the runtime
 * mirror in the store test. Consequently this file imports nothing from
 * workbenchProjectionTypes / workbenchRuntimeStore.
 */

import { create } from 'zustand';

/** Openable side surfaces (Git panel / settings are container placeholders). */
export type WorkbenchSurfaceId = 'none' | 'git' | 'settings';

export interface WorkbenchUiState {
  /** Workspace the rail currently highlights; null until the first projection. */
  activeWorkspaceId: string | null;
  /** Focus cursor per workspace (niri: focus is per-strip). Ids only. */
  focusedTaskIdByWorkspace: Record<string, string | null>;
  overviewOpen: boolean;
  /** Overview keyboard/click selection — one task id across all strips. */
  overviewSelectedTaskId: string | null;
  /** Overview pagination cursor per workspace (极端列数分页). */
  overviewPageByWorkspace: Record<string, number>;
  paletteOpen: boolean;
  openSurface: WorkbenchSurfaceId;
  /** Gesture transient: horizontal strip offset, restored per workspace. */
  stripScrollLeftByWorkspace: Record<string, number>;
  setActiveWorkspace: (workspaceId: string | null) => void;
  setFocusedTask: (workspaceId: string, taskId: string | null) => void;
  setOverviewOpen: (open: boolean) => void;
  setOverviewSelection: (taskId: string | null) => void;
  setOverviewPage: (workspaceId: string, page: number) => void;
  setPaletteOpen: (open: boolean) => void;
  setOpenSurface: (surface: WorkbenchSurfaceId) => void;
  setStripScrollLeft: (workspaceId: string, scrollLeft: number) => void;
}

export function buildInitialUiState() {
  return {
    activeWorkspaceId: null,
    focusedTaskIdByWorkspace: {},
    overviewOpen: false,
    overviewSelectedTaskId: null,
    overviewPageByWorkspace: {},
    paletteOpen: false,
    openSurface: 'none' as WorkbenchSurfaceId,
    stripScrollLeftByWorkspace: {},
  };
}

export function createWorkbenchUiStore() {
  return create<WorkbenchUiState>()(set => ({
    ...buildInitialUiState(),
    setActiveWorkspace: workspaceId => {
      set({ activeWorkspaceId: workspaceId, overviewOpen: false });
    },
    setFocusedTask: (workspaceId, taskId) => {
      set(state => ({
        focusedTaskIdByWorkspace: { ...state.focusedTaskIdByWorkspace, [workspaceId]: taskId },
      }));
    },
    setOverviewOpen: open => {
      set(state => (state.overviewOpen === open ? state : { overviewOpen: open }));
    },
    setOverviewSelection: taskId => {
      set({ overviewSelectedTaskId: taskId });
    },
    setOverviewPage: (workspaceId, page) => {
      set(state => ({ overviewPageByWorkspace: { ...state.overviewPageByWorkspace, [workspaceId]: page } }));
    },
    setPaletteOpen: open => {
      set({ paletteOpen: open });
    },
    setOpenSurface: surface => {
      set({ openSurface: surface });
    },
    setStripScrollLeft: (workspaceId, scrollLeft) => {
      set(state => {
        if (state.stripScrollLeftByWorkspace[workspaceId] === scrollLeft) return state;
        return { stripScrollLeftByWorkspace: { ...state.stripScrollLeftByWorkspace, [workspaceId]: scrollLeft } };
      });
    },
  }));
}

/** App-level singleton. Tests create isolated stores via createWorkbenchUiStore(). */
export const useWorkbenchUiStore = createWorkbenchUiStore();

/** Resets the singleton's data fields in place (actions are preserved). */
export function resetWorkbenchUiStoreForTests(): void {
  useWorkbenchUiStore.setState(buildInitialUiState());
}
