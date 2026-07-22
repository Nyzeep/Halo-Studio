from __future__ import annotations

from collections.abc import Iterable, Sequence

from .models import CompletionCandidate, SlashCommand


PREFIX_SCORE = 40
FUZZY_SCORE = 20
CURRENT_AGENT_SCORE = 20
RECENT_SCORE = 10
FAVORITE_SCORE = 10


def default_commands() -> tuple[SlashCommand, ...]:
    return (
        SlashCommand(
            name="/codex",
            description="Run Codex CLI in the current workspace",
            agent_id="codex-cli",
            arguments=("--continue", "--model", "--sandbox", "--full-auto"),
        ),
        SlashCommand(
            name="/review",
            description="Ask Codex CLI to review the current changes",
            agent_id="codex-cli",
            arguments=("--branch", "--changes", "--summary"),
        ),
        SlashCommand(
            name="/claude",
            description="Start a Claude Code session",
            agent_id="claude-code",
            arguments=("--continue", "--model", "--permission-mode"),
        ),
        SlashCommand(
            name="/opencode",
            description="Open an OpenCode workflow",
            agent_id="opencode",
            arguments=("--session", "--model"),
        ),
        SlashCommand(
            name="/pi",
            description="Start a Pi assistant workflow",
            agent_id="pi",
            arguments=("--persona", "--brief"),
        ),
        SlashCommand(
            name="/plan",
            description="Draft a multi-agent workflow plan",
            agent_id="codex-cli",
            arguments=("--tasks", "--dry-run"),
        ),
    )


def complete_commands(
    query: str,
    commands: Sequence[SlashCommand] | None = None,
    *,
    current_agent_id: str | None = None,
    recent: Iterable[str] = (),
    favorites: Iterable[str] = (),
) -> list[CompletionCandidate]:
    catalog = tuple(commands or default_commands())
    normalized_query = query.lstrip()
    recent_set = set(recent)
    favorite_set = set(favorites)

    first_space = _first_whitespace(normalized_query)
    if first_space >= 0:
        command_name = normalized_query[:first_space]
        argument_query = normalized_query[first_space + 1:]
        command = next(
            (item for item in catalog if item.name == command_name),
            None,
        )
        if command is not None:
            return _complete_arguments(
                command,
                argument_query.strip(),
                current_agent_id=current_agent_id,
                recent=recent_set,
                favorites=favorite_set,
            )

    candidates: list[CompletionCandidate] = []
    for command in catalog:
        score = _score_command(
            normalized_query,
            command,
            current_agent_id=current_agent_id,
            recent=recent_set,
            favorites=favorite_set,
        )
        if score > 0:
            candidates.append(
                CompletionCandidate(
                    name=command.name,
                    score=score,
                    description=command.description,
                    agent_id=command.agent_id,
                    kind="command",
                    insert_text=command.name,
                    command_name=command.name,
                )
            )

    return sorted(candidates, key=lambda item: (-item.score, item.name))


def _first_whitespace(value: str) -> int:
    for index, character in enumerate(value):
        if character.isspace():
            return index
    return -1


def _complete_arguments(
    command: SlashCommand,
    argument_query: str,
    *,
    current_agent_id: str | None,
    recent: set[str],
    favorites: set[str],
) -> list[CompletionCandidate]:
    candidates: list[CompletionCandidate] = []
    for argument in command.arguments:
        match_score = _score_text(argument_query, argument)
        if match_score == 0:
            continue

        score = match_score
        if command.agent_id == current_agent_id:
            score += CURRENT_AGENT_SCORE
        if argument in recent or f"{command.name} {argument}" in recent:
            score += RECENT_SCORE
        if argument in favorites or f"{command.name} {argument}" in favorites:
            score += FAVORITE_SCORE

        candidates.append(
            CompletionCandidate(
                name=argument,
                score=score,
                description=f"{command.name} option",
                agent_id=command.agent_id,
                kind="argument",
                insert_text=argument,
                command_name=command.name,
            )
        )
    return sorted(candidates, key=lambda item: (-item.score, item.name))


def _score_command(
    query: str,
    command: SlashCommand,
    *,
    current_agent_id: str | None,
    recent: set[str],
    favorites: set[str],
) -> int:
    match_score = _score_text(query, command.name)
    if match_score == 0:
        return 0

    score = match_score
    if command.agent_id == current_agent_id:
        score += CURRENT_AGENT_SCORE
    if command.name in recent:
        score += RECENT_SCORE
    if command.name in favorites:
        score += FAVORITE_SCORE
    return score


def _score_text(query: str, value: str) -> int:
    if query in ("", "/"):
        return FUZZY_SCORE

    query_lower = query.lower()
    value_lower = value.lower()
    if value_lower.startswith(query_lower):
        return PREFIX_SCORE
    if _is_fuzzy_continuity(query_lower, value_lower):
        return FUZZY_SCORE
    return 0


def _is_fuzzy_continuity(query: str, value: str) -> bool:
    if not query:
        return True

    search_from = 0
    for character in query:
        found_at = value.find(character, search_from)
        if found_at < 0:
            return False
        search_from = found_at + 1
    return True
