export type AgentId = "claude-code" | "codex-cli" | "opencode" | "pi";

export type AgentStatus = "ready" | "missing" | "error";

export type AgentIntegrationMode = "terminal" | "rpc" | "mcp" | "config-only";

export interface AgentInfo {
  id: AgentId;
  name: string;
  command: string;
  status: AgentStatus;
  version: string | null;
  installHint: string;
  modes: AgentIntegrationMode[];
}

export interface WorkspaceInfo {
  id: string;
  name: string;
  path: string;
}

export interface TerminalSessionInfo {
  id: string;
  agentId: AgentId;
  title: string;
  cwd: string;
  status: "starting" | "running" | "stopped" | "failed";
  createdAt: string;
}
