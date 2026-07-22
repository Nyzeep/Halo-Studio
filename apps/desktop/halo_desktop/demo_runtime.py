from __future__ import annotations

from .models import WorkflowEvent


SUPPORTED_AGENT_COUNTS = {4, 16, 32}
AGENT_SEQUENCE = ("codex-cli", "claude-code", "opencode", "pi")
EVENT_KINDS = (
    "run.state",
    "message.created",
    "thinking.delta",
    "tool.started",
    "tool.completed",
    "message.completed",
    "token.updated",
)


def run_demo_agents(agent_count: int = 4) -> tuple[WorkflowEvent, ...]:
    if agent_count not in SUPPORTED_AGENT_COUNTS:
        raise ValueError("agent_count must be one of 4, 16, or 32")

    events: list[WorkflowEvent] = []
    for run_index in range(agent_count):
        run_id = f"run-{run_index + 1}"
        agent_id = AGENT_SEQUENCE[run_index % len(AGENT_SEQUENCE)]
        for seq, kind in enumerate(EVENT_KINDS, start=1):
            events.append(
                WorkflowEvent(
                    run_id=run_id,
                    agent_id=agent_id,
                    seq=seq,
                    kind=kind,
                    title=_event_title(kind, run_id, agent_id),
                    body=_event_body(kind),
                    role=_event_role(kind),
                    payload=(
                        ("phase", "demo"),
                        ("agentIndex", run_index + 1),
                    ),
                )
            )
    return tuple(events)


def _event_title(kind: str, run_id: str, agent_id: str) -> str:
    labels = {
        "run.state": "Run queued",
        "message.created": "User prompt received",
        "thinking.delta": "Planning next step",
        "tool.started": "Tool call started",
        "tool.completed": "Tool call completed",
        "message.completed": "Assistant response ready",
        "token.updated": "Usage updated",
    }
    return f"{labels[kind]} - {agent_id} - {run_id}"


def _event_body(kind: str) -> str:
    bodies = {
        "run.state": "The agent run entered the Phase 1 demo scheduler.",
        "message.created": "Draft workspace instruction captured for the run.",
        "thinking.delta": "The agent is selecting the next workflow action.",
        "tool.started": "A fake tool span opened for UI timing and grouping.",
        "tool.completed": "The fake tool span finished deterministically.",
        "message.completed": "The assistant summary is ready for inspection.",
        "token.updated": "Token totals were refreshed for the inspector.",
    }
    return bodies[kind]


def _event_role(kind: str) -> str:
    if kind == "message.created":
        return "user"
    if kind in {"message.completed", "token.updated"}:
        return "assistant"
    if kind.startswith("tool."):
        return "tool"
    if kind == "thinking.delta":
        return "thinking"
    return "system"
