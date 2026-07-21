import type { AgentId } from "../../shared/agents.js";
import type { McpConfigPreview, McpServerConfig } from "../../shared/mcp.js";

const agentNames: Record<AgentId, string> = {
  "claude-code": "Claude Code",
  "codex-cli": "Codex CLI",
  opencode: "OpenCode",
  pi: "Pi"
};

export function createMcpConfigPreviews(server: McpServerConfig): McpConfigPreview[] {
  return server.targetAgents.map((agentId) => {
    switch (agentId) {
      case "codex-cli":
        return createCodexPreview(server);
      case "claude-code":
        return createClaudePreview(server);
      case "opencode":
        return createOpenCodePreview(server);
      case "pi":
        return createPiPreview(server);
    }
  });
}

function createCodexPreview(server: McpServerConfig): McpConfigPreview {
  const lines = [
    `[mcp_servers.${server.id}]`,
    `command = ${toTomlString(server.command ?? "")}`,
    `args = ${toTomlArray(server.args ?? [])}`
  ];

  if (server.env && Object.keys(server.env).length > 0) {
    lines.push(`[mcp_servers.${server.id}.env]`);
    for (const [key, value] of Object.entries(server.env)) {
      lines.push(`${key} = ${toTomlString(value)}`);
    }
  }

  return {
    agentId: "codex-cli",
    agentName: agentNames["codex-cli"],
    targetPath: "~/.codex/config.toml",
    language: "toml",
    content: lines.join("\n")
  };
}

function createClaudePreview(server: McpServerConfig): McpConfigPreview {
  return {
    agentId: "claude-code",
    agentName: agentNames["claude-code"],
    targetPath: ".mcp.json",
    language: "json",
    content: stringifyJson({
      mcpServers: {
        [server.id]: createJsonServerConfig(server)
      }
    })
  };
}

function createOpenCodePreview(server: McpServerConfig): McpConfigPreview {
  return {
    agentId: "opencode",
    agentName: agentNames.opencode,
    targetPath: "opencode.json",
    language: "jsonc",
    content: stringifyJson({
      mcp: {
        [server.id]: createJsonServerConfig(server)
      }
    })
  };
}

function createPiPreview(server: McpServerConfig): McpConfigPreview {
  return {
    agentId: "pi",
    agentName: agentNames.pi,
    targetPath: "~/.pi/mcp.json",
    language: "json",
    content: stringifyJson({
      mcpServers: {
        [server.id]: createJsonServerConfig(server)
      }
    })
  };
}

function createJsonServerConfig(server: McpServerConfig) {
  if (server.transport === "stdio") {
    return {
      command: server.command ?? "",
      args: server.args ?? [],
      env: server.env ?? {}
    };
  }

  return {
    type: server.transport,
    url: server.url ?? "",
    headers: server.headers ?? {}
  };
}

function stringifyJson(value: unknown) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function toTomlArray(values: string[]) {
  return `[${values.map(toTomlString).join(", ")}]`;
}

function toTomlString(value: string) {
  return JSON.stringify(value);
}
