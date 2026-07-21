import type { AgentInfo } from "../../shared/agents.js";
import { createAgentAdapters } from "./adapters.js";
import { commandExists, readVersion } from "./detect.js";
import type { AgentAdapter, CommandProbe } from "./types.js";

export class AgentRegistry {
  constructor(
    private readonly adapters: AgentAdapter[],
    private readonly probe: CommandProbe
  ) {}

  list(): AgentInfo[] {
    return this.adapters.map(({ definition }) => ({
      id: definition.id,
      name: definition.name,
      command: definition.command,
      status: "missing",
      version: null,
      installHint: definition.installHint,
      modes: definition.modes
    }));
  }

  async detectAll(): Promise<AgentInfo[]> {
    return Promise.all(
      this.adapters.map(async (adapter) => {
        try {
          return await adapter.detect(this.probe);
        } catch (error) {
          return {
            id: adapter.definition.id,
            name: adapter.definition.name,
            command: adapter.definition.command,
            status: "error",
            version: null,
            installHint: `Agent detection failed: ${formatProbeError(error)}`,
            modes: adapter.definition.modes
          };
        }
      })
    );
  }
}

function formatProbeError(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

export function createAgentRegistry(probe: CommandProbe = { commandExists, readVersion }) {
  return new AgentRegistry(createAgentAdapters(), probe);
}
