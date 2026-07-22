from __future__ import annotations

from pathlib import Path
from typing import Any, Protocol

from .completion import complete_commands, default_commands
from .demo_runtime import run_demo_agents
from .models import AgentProfile, WorkflowEvent
from .plugin_registry import PluginRegistry


class RuntimeProvider(Protocol):
    def timeline_events(self, agent_count: int = 4) -> tuple[WorkflowEvent, ...]:
        ...


class DemoRuntimeProvider:
    def timeline_events(self, agent_count: int = 4) -> tuple[WorkflowEvent, ...]:
        return run_demo_agents(agent_count)


class IpcRuntimeProvider:
    def __init__(self, ipc_client: Any | None) -> None:
        self._ipc_client = ipc_client

    def timeline_events(self, agent_count: int = 4) -> tuple[WorkflowEvent, ...]:
        if self._ipc_client is None:
            return ()

        cached_events = getattr(self._ipc_client, "cached_events", None)
        if cached_events is None:
            return ()

        raw_events = cached_events()
        return tuple(
            _workflow_event_from_ipc(event)
            for event in raw_events
            if event.get("type") == "runtimeEvent"
        )


DEFAULT_AGENTS = (
    AgentProfile(
        id="codex-cli",
        name="Codex CLI",
        provider="openai",
        transport="pty",
        command="codex",
        capabilities=("code", "review", "planning"),
        commands=("/codex", "/review", "/plan"),
    ),
    AgentProfile(
        id="claude-code",
        name="Claude Code",
        provider="anthropic",
        transport="pty",
        command="claude",
        capabilities=("code", "analysis"),
        commands=("/claude",),
    ),
    AgentProfile(
        id="opencode",
        name="OpenCode",
        provider="community",
        transport="pty",
        command="opencode",
        capabilities=("code", "shell"),
        commands=("/opencode",),
    ),
    AgentProfile(
        id="pi",
        name="Pi",
        provider="inflection",
        transport="pty",
        command="pi",
        capabilities=("conversation", "briefing"),
        commands=("/pi",),
    ),
)


class AppController:
    def __init__(
        self,
        project_root: str | Path | None = None,
        *,
        runtime_mode: str = "demo",
        ipc_client: Any | None = None,
    ) -> None:
        self.project_root = Path(project_root) if project_root else _default_project_root()
        if runtime_mode not in {"demo", "ipc"}:
            raise ValueError("runtime_mode must be 'demo' or 'ipc'")
        self.runtime_mode = runtime_mode
        self._runtime: RuntimeProvider = (
            IpcRuntimeProvider(ipc_client)
            if runtime_mode == "ipc"
            else DemoRuntimeProvider()
        )
        loaded_agents = PluginRegistry(self.project_root).load_agents()
        self._agents = tuple(loaded_agents) if loaded_agents else DEFAULT_AGENTS

    def list_agents(self) -> list[dict[str, Any]]:
        return [agent.to_dict() for agent in self._agents]

    def command_catalog(self) -> list[dict[str, Any]]:
        return [
            {
                "name": command.name,
                "description": command.description,
                "agentId": command.agent_id,
                "arguments": list(command.arguments),
            }
            for command in self._command_catalog()
        ]

    def complete(
        self,
        query: str,
        *,
        current_agent_id: str | None = None,
        recent: tuple[str, ...] = (),
        favorites: tuple[str, ...] = ("/codex",),
    ) -> list[dict[str, Any]]:
        return [
            candidate.to_dict()
            for candidate in complete_commands(
                query,
                self._command_catalog(),
                current_agent_id=current_agent_id,
                recent=recent,
                favorites=favorites,
            )
        ]

    def timeline_events(self, agent_count: int = 4) -> list[dict[str, Any]]:
        return [event.to_dict() for event in self._runtime.timeline_events(agent_count)]

    def inspector_state(self) -> dict[str, Any]:
        events = self.timeline_events(4)
        return {
            "selectedRunId": events[0]["runId"] if events else "",
            "eventCount": len(events),
            "activeAgentCount": len(self._agents),
            "debugDrawerOpen": False,
        }

    def _command_catalog(self):
        commands = list(default_commands())
        known_names = {command.name for command in commands}
        for agent in self._agents:
            for command in agent.command_specs:
                if command.name not in known_names:
                    commands.append(command)
                    known_names.add(command.name)
        return tuple(commands)


def create_qml_controller(
    project_root: str | Path | None = None,
    *,
    runtime_mode: str = "demo",
    ipc_client: Any | None = None,
):
    from PySide6.QtCore import Property, QObject, Signal, Slot

    base = AppController(
        project_root,
        runtime_mode=runtime_mode,
        ipc_client=ipc_client,
    )

    class QmlAppController(QObject):
        agentsChanged = Signal()
        eventsChanged = Signal()

        @Property("QVariantList", notify=agentsChanged)
        def agents(self):
            return base.list_agents()

        @Property("QVariantList", notify=eventsChanged)
        def events(self):
            return base.timeline_events(4)

        @Slot(str, str, result="QVariantList")
        def complete(self, query: str, current_agent_id: str):
            return base.complete(query, current_agent_id=current_agent_id)

        @Slot(int, result="QVariantList")
        def timelineEvents(self, agent_count: int):
            return base.timeline_events(agent_count)

        @Slot(result="QVariantMap")
        def inspectorState(self):
            return base.inspector_state()

    return QmlAppController()


def _workflow_event_from_ipc(event: dict[str, Any]) -> WorkflowEvent:
    run_id = str(event.get("runId", ""))
    agent_id = str(event.get("agentId", ""))
    kind = str(event.get("kind", "runtime.event"))
    message = str(event.get("message", ""))
    seq = int(event.get("seq", 0))

    return WorkflowEvent(
        run_id=run_id,
        agent_id=agent_id,
        seq=seq,
        kind=kind,
        title=f"{kind} - {agent_id} - {run_id}",
        body=message,
        role=_role_for_runtime_kind(kind),
        payload=(("phase", "ipc"),),
    )


def _role_for_runtime_kind(kind: str) -> str:
    if kind == "message.delta":
        return "assistant"
    if kind.startswith("tool."):
        return "tool"
    if kind == "run.state":
        return "system"
    return "assistant"


def _default_project_root() -> Path:
    return Path(__file__).resolve().parents[3]
