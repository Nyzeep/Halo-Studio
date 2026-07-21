import { describe, expect, it } from "vitest";
import { createAgentRegistry } from "../main/agents/registry";

describe("Agent Registry", () => {
  it("registers the four supported agents", () => {
    const registry = createAgentRegistry();

    expect(registry.list().map((agent) => agent.id)).toEqual([
      "claude-code",
      "codex-cli",
      "opencode",
      "pi"
    ]);
  });

  it("reports a clear missing state when commands are not found", async () => {
    const registry = createAgentRegistry({
      commandExists: async () => false,
      readVersion: async () => null
    });

    const agents = await registry.detectAll();

    expect(agents).toHaveLength(4);
    expect(agents.every((agent) => agent.status === "missing")).toBe(true);
    expect(agents[0]?.installHint).toContain("未检测到");
  });
});
