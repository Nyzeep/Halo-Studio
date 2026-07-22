from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True, slots=True)
class AgentProfile:
    id: str
    name: str
    provider: str
    transport: str
    command: str = ""
    capabilities: tuple[str, ...] = ()
    commands: tuple[str, ...] = ()
    command_specs: tuple[SlashCommand, ...] = ()

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "provider": self.provider,
            "transport": self.transport,
            "command": self.command,
            "capabilities": list(self.capabilities),
            "commands": list(self.commands),
        }


@dataclass(frozen=True, slots=True)
class SlashCommand:
    name: str
    description: str
    agent_id: str
    arguments: tuple[str, ...] = ()


@dataclass(frozen=True, slots=True)
class CompletionCandidate:
    name: str
    score: int
    description: str = ""
    agent_id: str = ""
    kind: str = "command"
    insert_text: str = ""
    command_name: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "score": self.score,
            "description": self.description,
            "agentId": self.agent_id,
            "kind": self.kind,
            "insertText": self.insert_text or self.name,
            "commandName": self.command_name,
        }


@dataclass(frozen=True, slots=True)
class WorkflowEvent:
    run_id: str
    agent_id: str
    seq: int
    kind: str
    title: str
    body: str = ""
    role: str = ""
    payload: tuple[tuple[str, Any], ...] = field(default_factory=tuple)

    def to_dict(self) -> dict[str, Any]:
        return {
            "runId": self.run_id,
            "agentId": self.agent_id,
            "seq": self.seq,
            "kind": self.kind,
            "title": self.title,
            "body": self.body,
            "role": self.role,
            "payload": dict(self.payload),
        }
