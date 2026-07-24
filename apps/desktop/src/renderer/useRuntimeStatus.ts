import { useCallback, useEffect, useRef, useState } from "react";
import type { AgentKind, RuntimeBinding } from "@halo-studio/contracts";
import { publicRequestMessage, unwrapEnvelope, type WorkbenchApi } from "./api.js";

export interface RuntimeStatusState {
  readonly bindings: readonly RuntimeBinding[];
  readonly loading: boolean;
  readonly error: string | undefined;
  refresh(workspaceId?: string): Promise<void>;
  startOpenCode(workspaceId: string): Promise<void>;
  startPi(workspaceId: string): Promise<void>;
  stopPi(workspaceId: string): Promise<void>;
  retryPi(workspaceId: string): Promise<void>;
}

function replaceBinding(
  bindings: readonly RuntimeBinding[],
  next: RuntimeBinding,
): readonly RuntimeBinding[] {
  return [
    ...bindings.filter((binding) => binding.agentKind !== next.agentKind),
    next,
  ];
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

  const synchronizeAfterFailure = useCallback(async (
    targetWorkspaceId: string,
    version: number,
  ): Promise<void> => {
    if (api === undefined) return;
    try {
      const latestBindings = unwrapEnvelope(await api.runtime.snapshot({ workspaceId: targetWorkspaceId }));
      if (version === requestVersion.current) setBindings(latestBindings);
    } catch {
      // The original public action error is more useful than a failed refresh.
    }
  }, [api]);

  const runRuntimeAction = useCallback(async (
    targetWorkspaceId: string,
    agentKind: AgentKind,
    action: "start" | "stop",
  ): Promise<void> => {
    if (api === undefined) {
      setError("桌面桥接不可用。");
      return;
    }
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setLoading(true);
    try {
      const binding = action === "start"
        ? unwrapEnvelope(await api.runtime.start({ workspaceId: targetWorkspaceId, agentKind }))
        : unwrapEnvelope(await api.runtime.stop({ workspaceId: targetWorkspaceId, agentKind }));
      if (version === requestVersion.current) {
        setBindings((current) => replaceBinding(current, binding));
        setError(undefined);
      }
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
      await synchronizeAfterFailure(targetWorkspaceId, version);
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [api, synchronizeAfterFailure]);

  const startOpenCode = useCallback(
    async (targetWorkspaceId: string): Promise<void> => runRuntimeAction(targetWorkspaceId, "opencode", "start"),
    [runRuntimeAction],
  );
  const startPi = useCallback(
    async (targetWorkspaceId: string): Promise<void> => runRuntimeAction(targetWorkspaceId, "pi", "start"),
    [runRuntimeAction],
  );
  const stopPi = useCallback(
    async (targetWorkspaceId: string): Promise<void> => runRuntimeAction(targetWorkspaceId, "pi", "stop"),
    [runRuntimeAction],
  );
  const retryPi = useCallback(async (targetWorkspaceId: string): Promise<void> => {
    if (api === undefined) {
      setError("桌面桥接不可用。");
      return;
    }
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setLoading(true);
    try {
      // Main owns the lifecycle. Stopping first is only defensive: it lets the
      // service release a failed Pi instance before creating a fresh one.
      await unwrapEnvelope(await api.runtime.stop({ workspaceId: targetWorkspaceId, agentKind: "pi" }));
      const started = unwrapEnvelope(await api.runtime.start({ workspaceId: targetWorkspaceId, agentKind: "pi" }));
      if (version === requestVersion.current) {
        setBindings((current) => replaceBinding(current, started));
        setError(undefined);
      }
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
      await synchronizeAfterFailure(targetWorkspaceId, version);
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [api, synchronizeAfterFailure]);

  return {
    bindings,
    loading,
    error,
    refresh,
    startOpenCode,
    startPi,
    stopPi,
    retryPi,
  };
}
