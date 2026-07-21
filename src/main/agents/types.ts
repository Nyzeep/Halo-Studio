import type { AgentId, AgentInfo, AgentIntegrationMode } from "../../shared/agents.js";

export interface CommandProbe {
  commandExists(command: string): Promise<boolean>;
  readVersion(command: string, args: string[]): Promise<string | null>;
}

export interface AgentAdapterDefinition {
  id: AgentId;
  name: string;
  command: string;
  versionArgs: string[];
  installHint: string;
  modes: AgentIntegrationMode[];
}

export interface AgentAdapter {
  definition: AgentAdapterDefinition;
  detect(probe: CommandProbe): Promise<AgentInfo>;
}
