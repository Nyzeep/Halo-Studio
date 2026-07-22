from __future__ import annotations

from pathlib import Path
import tomllib

from .models import AgentProfile, SlashCommand


class PluginRegistry:
    def __init__(self, project_root: str | Path | None = None) -> None:
        self.project_root = Path(project_root) if project_root else _default_project_root()

    def load_agents(self) -> list[AgentProfile]:
        manifest_root = self.project_root / "plugins" / "agents"
        if not manifest_root.exists():
            return []

        agents: list[AgentProfile] = []
        for manifest_path in sorted(manifest_root.glob("*/agent.toml")):
            agents.append(_read_agent_manifest(manifest_path))
        return agents


def _read_agent_manifest(path: Path) -> AgentProfile:
    with path.open("rb") as manifest_file:
        raw = tomllib.load(manifest_file)

    command_specs = _command_specs(str(raw["id"]), raw.get("commands", ()))

    return AgentProfile(
        id=str(raw["id"]),
        name=str(raw["name"]),
        provider=str(raw["provider"]),
        transport=str(raw.get("transport", "pty")),
        command=str(raw.get("command", "")),
        capabilities=tuple(str(item) for item in raw.get("capabilities", ())),
        commands=tuple(command.name for command in command_specs),
        command_specs=command_specs,
    )


def _command_specs(agent_id: str, raw_commands: object) -> tuple[SlashCommand, ...]:
    if not isinstance(raw_commands, list):
        return ()

    specs: list[SlashCommand] = []
    for command in raw_commands:
        if isinstance(command, str):
            specs.append(
                SlashCommand(
                    name=command,
                    description=f"{agent_id} command",
                    agent_id=agent_id,
                )
            )
        elif isinstance(command, dict) and "name" in command:
            specs.append(
                SlashCommand(
                    name=str(command["name"]),
                    description=str(command.get("description", f"{agent_id} command")),
                    agent_id=agent_id,
                    arguments=tuple(str(item) for item in command.get("args", ())),
                )
            )
    return tuple(specs)


def _default_project_root() -> Path:
    return Path(__file__).resolve().parents[3]
