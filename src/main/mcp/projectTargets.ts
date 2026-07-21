import path from "node:path";
import type { AgentId } from "../../shared/agents.js";
import type { RealConfigWritePlan } from "../../shared/config.js";
import type { McpConfigPreview } from "../../shared/mcp.js";
import { planRealConfigWrite } from "../config/writeGuard.js";

const projectMcpTargets: Record<AgentId, string> = {
  "claude-code": ".mcp.json",
  "codex-cli": path.join(".codex", "config.toml"),
  opencode: "opencode.json",
  pi: path.join(".pi", "mcp.json")
};

export function createProjectMcpWritePlan(workspaceRoot: string, preview: McpConfigPreview): RealConfigWritePlan {
  return planRealConfigWrite({
    workspaceRoot,
    targetPath: path.join(workspaceRoot, projectMcpTargets[preview.agentId]),
    nextContent: preview.content,
    reason: `${preview.agentName} 项目 MCP 配置`
  });
}
