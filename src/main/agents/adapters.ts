import type { AgentAdapter, AgentAdapterDefinition, CommandProbe } from "./types.js";

const definitions: AgentAdapterDefinition[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    command: "claude",
    versionArgs: ["--version"],
    installHint: "未检测到 Claude Code，请先安装并确认 claude 命令在 PATH 中。",
    modes: ["terminal", "mcp", "config-only"]
  },
  {
    id: "codex-cli",
    name: "Codex CLI",
    command: "codex",
    versionArgs: ["--version"],
    installHint: "未检测到 Codex CLI，请先安装并确认 codex 命令在 PATH 中。",
    modes: ["terminal", "mcp", "config-only"]
  },
  {
    id: "opencode",
    name: "OpenCode",
    command: "opencode",
    versionArgs: ["--version"],
    installHint: "未检测到 OpenCode，请先安装并确认 opencode 命令在 PATH 中。",
    modes: ["terminal", "mcp", "config-only"]
  },
  {
    id: "pi",
    name: "Pi",
    command: "pi",
    versionArgs: ["--version"],
    installHint: "未检测到 Pi，请先安装并确认 pi 命令在 PATH 中。",
    modes: ["terminal", "rpc", "mcp", "config-only"]
  }
];

export function createAgentAdapters(): AgentAdapter[] {
  return definitions.map((definition) => ({
    definition,
    async detect(probe: CommandProbe) {
      const exists = await probe.commandExists(definition.command);
      if (!exists) {
        return {
          id: definition.id,
          name: definition.name,
          command: definition.command,
          status: "missing",
          version: null,
          installHint: definition.installHint,
          modes: definition.modes
        };
      }

      return {
        id: definition.id,
        name: definition.name,
        command: definition.command,
        status: "ready",
        version: await probe.readVersion(definition.command, definition.versionArgs),
        installHint: "",
        modes: definition.modes
      };
    }
  }));
}
