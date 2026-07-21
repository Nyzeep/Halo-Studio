import path from "node:path";
import { describe, expect, it } from "vitest";
import { createProjectMcpWritePlan } from "../main/mcp/projectTargets";
import type { AgentId } from "../shared/agents";
import type { McpConfigPreview } from "../shared/mcp";

const workspaceRoot = path.resolve("D:\\Halo Studio");

function createPreview(agentId: AgentId): McpConfigPreview {
  return {
    agentId,
    agentName: agentId,
    targetPath: "ignored-by-project-targets",
    language: agentId === "codex-cli" ? "toml" : "json",
    content: `${agentId} config\n`
  };
}

describe("project MCP write targets", () => {
  it.each([
    ["claude-code", ".mcp.json"],
    ["codex-cli", path.join(".codex", "config.toml")],
    ["opencode", "opencode.json"],
    ["pi", path.join(".pi", "mcp.json")]
  ] satisfies Array<[AgentId, string]>)("plans %s writes inside %s", (agentId, relativeTarget) => {
    const preview = createPreview(agentId);

    const plan = createProjectMcpWritePlan(workspaceRoot, preview);

    expect(plan.allowed).toBe(true);
    expect(plan.normalizedTargetPath).toBe(path.join(workspaceRoot, relativeTarget));
    expect(plan.nextContent).toBe(preview.content);
    expect(plan.reason).toBe(`${preview.agentName} 项目 MCP 配置`);
    expect(plan.confirmationPhrase).toBe(`APPLY ${path.basename(relativeTarget)}`);
  });
});
