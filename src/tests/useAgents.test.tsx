import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgents } from "../renderer/hooks/useAgents";
import type { AgentInfo } from "../shared/agents";

const missingClaude: AgentInfo = {
  id: "claude-code",
  name: "Claude Code",
  command: "claude",
  status: "missing",
  version: null,
  installHint: "install claude",
  modes: ["terminal", "mcp", "config-only"]
};

const readyClaude: AgentInfo = {
  ...missingClaude,
  status: "ready",
  version: "1.0.0",
  installHint: ""
};

describe("useAgents", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("exposes refreshDiscovery to re-run agent detection", async () => {
    const detectAll = vi.fn().mockResolvedValueOnce([missingClaude]).mockResolvedValueOnce([readyClaude]);
    window.halo = {
      agents: {
        detectAll
      }
    } as unknown as typeof window.halo;

    const { result } = renderHook(() => useAgents());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.agents).toEqual([missingClaude]);

    await act(async () => {
      await result.current.refreshDiscovery();
    });

    expect(detectAll).toHaveBeenCalledTimes(2);
    expect(result.current.agents).toEqual([readyClaude]);
  });

  it("stops loading when agent discovery fails", async () => {
    const detectAll = vi.fn().mockRejectedValue(new Error("spawn EPERM"));
    window.halo = {
      agents: {
        detectAll
      }
    } as unknown as typeof window.halo;

    const { result } = renderHook(() => useAgents());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.agents).toEqual([]);
  });

  it("keeps refreshDiscovery from leaking detection errors", async () => {
    const detectAll = vi.fn().mockResolvedValueOnce([readyClaude]).mockRejectedValueOnce(new Error("spawn EPERM"));
    window.halo = {
      agents: {
        detectAll
      }
    } as unknown as typeof window.halo;

    const { result } = renderHook(() => useAgents());

    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await expect(result.current.refreshDiscovery()).resolves.toBeUndefined();
    });

    expect(result.current.agents).toEqual([]);
    expect(result.current.loading).toBe(false);
  });
});
