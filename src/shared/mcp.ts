import type { AgentId } from "./agents.js";

export type McpTransport = "stdio" | "sse" | "http";

export interface McpServerConfig {
  id: string;
  displayName: string;
  transport: McpTransport;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled: boolean;
  targetAgents: AgentId[];
}

export interface McpConfigPreview {
  agentId: AgentId;
  agentName: string;
  targetPath: string;
  language: "json" | "jsonc" | "toml";
  content: string;
}
