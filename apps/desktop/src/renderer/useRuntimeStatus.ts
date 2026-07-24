import { useCallback, useEffect, useRef, useState } from "react";
import type { RuntimeBinding } from "@halo-studio/contracts";
import { publicRequestMessage, unwrapEnvelope, type WorkbenchApi } from "./api.js";

export interface RuntimeStatusState {
  readonly bindings: readonly RuntimeBinding[];
  readonly loading: boolean;
  readonly error: string | undefined;
  refresh(workspaceId?: string): Promise<void>;
  startOpenCode(workspaceId: string): Promise<void>;
}

export function useRuntimeStatus(api: WorkbenchApi | undefined, workspaceId: string | undefined): RuntimeStatusState {
  const [bindings, setBindings] = useState<readonly RuntimeBinding[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const requestVersion = useRef(0);

  const refresh = useCallback(async (targetWorkspaceId = workspaceId): Promise<void> => {
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    if (api === undefined || targetWorkspaceId === undefined) {
      if (version === requestVersion.current) {
        setBindings([]);
        setLoading(false);
      }
      return;
    }
    setLoading(true);
    try {
      const nextBindings = unwrapEnvelope(await api.runtime.probe({ workspaceId: targetWorkspaceId }));
      if (version === requestVersion.current) {
        setBindings(nextBindings);
        setError(undefined);
      }
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [api, workspaceId]);

  useEffect(() => { void refresh(); }, [refresh]);

  const startOpenCode = useCallback(async (targetWorkspaceId: string): Promise<void> => {
    if (api === undefined) {
      setError("桌面桥接不可用。");
      return;
    }
    setLoading(true);
    try {
      const started = unwrapEnvelope(await api.runtime.start({ workspaceId: targetWorkspaceId, agentKind: "opencode" }));
      setBindings((current) => [
        ...current.filter((binding) => binding.agentKind !== "opencode"),
        started,
      ]);
      setError(undefined);
    } catch (requestError) {
      setError(publicRequestMessage(requestError));
    } finally {
      setLoading(false);
    }
  }, [api]);

  return { bindings, loading, error, refresh, startOpenCode };
}
