"""命令面板和快速打开共用的确定性模糊匹配器。"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ScoredItem:
    score: int
    matched_indices: list[int]
    matched_on: str


@dataclass(frozen=True)
class _State:
    score: int
    indices: tuple[int, ...]
    run_length: int


def fuzzy_score(query: str, target: str) -> tuple[int, list[int]]:
    """按序匹配一个查询段，返回分数和目标字符串索引。"""
    if not query or not target:
        return 0, []
    states: list[dict[tuple[int, int], _State]] = []
    for query_index, query_char in enumerate(query):
        current: dict[tuple[int, int], _State] = {}
        for target_index, target_char in enumerate(target):
            if not _same_char(query_char, target_char):
                continue
            if query_index == 0:
                state = _State(
                    _character_score(query_char, target, target_index, False, 1),
                    (target_index,),
                    1,
                )
                _keep_best(current, (target_index, 1), state)
                continue
            for (previous_index, previous_run), previous in states[-1].items():
                if previous_index >= target_index:
                    continue
                continuous = target_index == previous_index + 1
                run_length = previous_run + 1 if continuous else 1
                candidate = _State(
                    previous.score
                    + _character_score(query_char, target, target_index, continuous, run_length),
                    previous.indices + (target_index,),
                    run_length,
                )
                _keep_best(current, (target_index, run_length), candidate)
        if not current:
            return 0, []
        states.append(current)
    winner = max(states[-1].values(), key=lambda item: (item.score, tuple(-index for index in item.indices)))
    return winner.score, list(winner.indices)


def fuzzy_match(query: str, target: str) -> tuple[int, list[int]]:
    """对空白分段的查询分别打分，所有段均需命中。"""
    parts = [part for part in str(query).split() if part]
    if not parts:
        return 0, []
    score = 0
    indices: set[int] = set()
    for part in parts:
        part_score, part_indices = fuzzy_score(part, target)
        if not part_indices:
            return 0, []
        score += part_score
        indices.update(part_indices)
    return score, sorted(indices)


def char_bag(text: str) -> int:
    """ASCII 预过滤签名；未编码字符保持零位以避免 CJK 假阴性。"""
    value = 0
    for char in str(text).casefold():
        if "a" <= char <= "z":
            value |= 1 << (ord(char) - ord("a"))
        elif "0" <= char <= "9":
            value |= 1 << 26
        elif char == "-":
            value |= 1 << 27
        elif char == "_":
            value |= 1 << 28
        elif char == ".":
            value |= 1 << 29
    return value


def bag_is_subset(query_bag: int, target_bag: int) -> bool:
    return not bool(query_bag & ~target_bag)


def score_file_candidate(query: str, rel_path: str) -> ScoredItem | None:
    query = str(query).strip()
    rel_path = str(rel_path).replace("\\", "/")
    basename = rel_path.rsplit("/", 1)[-1]
    if not query:
        return ScoredItem(0, [], "basename")
    if "/" in query or "\\" in query:
        score, indices = fuzzy_match(query, rel_path)
        return ScoredItem(score, indices, "path") if indices else None
    if basename.casefold() == query.casefold():
        return ScoredItem(1 << 18, list(range(len(basename))), "basename")
    if basename.casefold().startswith(query.casefold()):
        return ScoredItem((1 << 17) + round(len(query) / max(1, len(basename)) * 100), list(range(len(query))), "basename")
    score, indices = fuzzy_match(query, basename)
    return ScoredItem((1 << 16) + score, indices, "basename") if indices else None


def score_command_candidate(query: str, title_with_category: str, command_id: str) -> ScoredItem | None:
    label_score, label_indices = fuzzy_match(query, title_with_category)
    id_score, id_indices = fuzzy_match(query, command_id)
    if not label_indices and not id_indices:
        return None
    if id_score > label_score:
        return ScoredItem(id_score, id_indices, "id")
    return ScoredItem(label_score, label_indices, "label")


def _keep_best(store: dict[tuple[int, int], _State], key: tuple[int, int], candidate: _State) -> None:
    current = store.get(key)
    if current is None or candidate.score > current.score or (
        candidate.score == current.score and candidate.indices < current.indices
    ):
        store[key] = candidate


def _same_char(query_char: str, target_char: str) -> bool:
    return _normalized(query_char) == _normalized(target_char)


def _normalized(char: str) -> str:
    return "/" if char in {"/", "\\"} else char.casefold()


def _character_score(query_char: str, target: str, index: int, continuous: bool, run_length: int) -> int:
    target_char = target[index]
    score = 1
    if query_char == target_char or ({query_char, target_char} <= {"/", "\\"}):
        score += 1
    if continuous:
        score += 6 if run_length - 1 <= 3 else 3
    if index == 0:
        return score + 8
    previous = target[index - 1]
    if previous in {"/", "\\"}:
        score += 5
    elif previous in {"_", "-", ".", " ", "'", '"', ":"}:
        score += 4
    elif not continuous and target_char.isupper() and previous.islower():
        score += 2
    return score
