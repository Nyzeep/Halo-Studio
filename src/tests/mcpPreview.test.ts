import { describe, expect, it } from "vitest";
import { createMcpConfigPreviews } from "../main/mcp/configPreview";
import type { McpServerConfig } from "../shared/mcp";

const filesystemServer: McpServerConfig = {
  id: "filesystem",
  displayName: "Filesystem",
  transport: "stdio",
  command: "npx",
  args: ["-y", "@modelcontextprotocol/server-filesystem", "D:\\Halo Studio"],
  env: {
    HALO_SCOPE: "workspace"
  },
  enabled: true,
  targetAgents: ["claude-code", "codex-cli", "opencode", "pi"]
};

describe("MCP config preview", () => {
  it("generates a Codex TOML preview", () => {
    const previews = createMcpConfigPreviews(filesystemServer);
    const codex = previews.find((preview) => preview.agentId === "codex-cli");

    expect(codex?.targetPath).toBe("~/.codex/config.toml");
    expect(codex?.language).toBe("toml");
    expect(codex?.content).toContain("[mcp_servers.filesystem]");
    expect(codex?.content).toContain('command = "npx"');
  });

  it("generates JSON previews for Claude, OpenCode, and Pi", () => {
    const previews = createMcpConfigPreviews(filesystemServer);

    expect(previews).toHaveLength(4);
    expect(previews.find((preview) => preview.agentId === "claude-code")?.content).toContain("\"filesystem\"");
    expect(previews.find((preview) => preview.agentId === "opencode")?.content).toContain("\"mcp\"");
    expect(previews.find((preview) => preview.agentId === "pi")?.targetPath).toBe("~/.pi/mcp.json");
  });
});
