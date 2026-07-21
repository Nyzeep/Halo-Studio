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
    return Promise.all(this.adapters.map((adapter) => adapter.detect(this.probe)));
  }
}

export function createAgentRegistry(probe: CommandProbe = { commandExists, readVersion }) {
  return new AgentRegistry(createAgentAdapters(), probe);
}
