import { useCallback, useEffect, useState } from "react";
import type { Workspace } from "@halo-studio/contracts";
import { publicRequestMessage, unwrapEnvelope, type WorkbenchApi } from "./api.js";

export interface WorkspaceState {
  readonly workspace: Workspace | undefined;
  readonly loading: boolean;
  readonly error: string | undefined;
  refresh(): Promise<void>;
  openFolder(): Promise<Workspace | undefined>;
  trustWorkspace(): Promise<Workspace | undefined>;
}

export function useWorkspace(api: WorkbenchApi | undefined): WorkspaceState {
  const [workspace, setWorkspace] = useState<Workspace>();
  const [loading, setLoading] = useState(api !== undefined);
  const [error, setError] = useState<string>();

  const refresh = useCallback(async (): Promise<void> => {
    if (api === undefined) {
      setLoading(false);
      setError("桌面桥接不可用。");
      return;
    }
    setLoading(true);
    try {
      const workspaces = unwrapEnvelope(await api.workspace.snapshot({}));
      setWorkspace(workspaces[0]);
      setError(undefined);
    } catch (requestError) {
      setError(publicRequestMessage(requestError));
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => { void refresh(); }, [refresh]);

  const openFolder = useCallback(async (): Promise<Workspace | undefined> => {
    if (api === undefined) {
      setError("桌面桥接不可用。");
      return undefined;
    }
    setLoading(true);
    try {
      const candidate = unwrapEnvelope(await api.workspace.pick({}));
      if (candidate === null) return undefined;
      const opened = unwrapEnvelope(await api.workspace.open({ selectionId: candidate.selectionId }));
      setWorkspace(opened);
      setError(undefined);
      return opened;
    } catch (requestError) {
      setError(publicRequestMessage(requestError));
      return undefined;
    } finally {
      setLoading(false);
    }
  }, [api]);

  const trustWorkspace = useCallback(async (): Promise<Workspace | undefined> => {
    if (api === undefined || workspace === undefined) return undefined;
    setLoading(true);
    try {
      const trusted = unwrapEnvelope(await api.workspace.setTrust({
        workspaceId: workspace.id,
        trustState: "trusted",
      }));
      setWorkspace(trusted);
      setError(undefined);
      return trusted;
    } catch (requestError) {
      setError(publicRequestMessage(requestError));
      return undefined;
    } finally {
      setLoading(false);
    }
  }, [api, workspace]);

  return { workspace, loading, error, refresh, openFolder, trustWorkspace };
}
