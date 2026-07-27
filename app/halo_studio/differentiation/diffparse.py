"""只读 unified diff 解析，用于审查跳转和归因 gutter。"""

from __future__ import annotations

import re

_HUNK = re.compile(
    r"^@@ -(?P<old_start>\d+)(?:,(?P<old_count>\d+))? "
    r"\+(?P<new_start>\d+)(?:,(?P<new_count>\d+))? @@(?: .*)?$"
)


def first_target_line(diff: str) -> int:
    """返回第一段 hunk 的新侧起始行；无法可靠解析时返回 ``-1``。"""
    for line in str(diff or "").splitlines():
        match = _HUNK.match(line)
        if match:
            return int(match.group("new_start"))
    return -1


def added_line_ranges(diff: str) -> list[tuple[int, int]] | None:
    """返回 unified diff 中新侧新增/替换的连续行区间。

    纯删除没有新侧行，使用其 hunk 新侧锚点标记一行，供 gutter 给出保守提示。
    空输入或不完整的 hunk 返回 ``None``，让调用方降级为文件级展示。
    """
    source = str(diff or "")
    if not source:
        return None

    ranges: list[tuple[int, int]] = []
    current_line: int | None = None
    added_start: int | None = None
    added_end: int | None = None
    saw_removed = False
    saw_added = False
    expected_old = 0
    expected_new = 0
    consumed_old = 0
    consumed_new = 0
    in_hunk = False
    saw_hunk = False

    def finish_added_range() -> None:
        nonlocal added_start, added_end
        if added_start is not None and added_end is not None:
            ranges.append((added_start, added_end))
        added_start = None
        added_end = None

    def finish_hunk() -> bool:
        nonlocal added_start, added_end, saw_removed, saw_added, in_hunk
        if not in_hunk:
            return True
        if consumed_old != expected_old or consumed_new != expected_new:
            return False
        finish_added_range()
        if not saw_added and saw_removed and current_line is not None:
            ranges.append((max(1, current_line), max(1, current_line)))
        added_start = None
        added_end = None
        saw_removed = False
        saw_added = False
        in_hunk = False
        return True

    for line in source.splitlines():
        match = _HUNK.match(line)
        if match:
            if not finish_hunk():
                return None
            current_line = int(match.group("new_start"))
            expected_old = int(match.group("old_count") or 1)
            expected_new = int(match.group("new_count") or 1)
            consumed_old = 0
            consumed_new = 0
            in_hunk = True
            saw_hunk = True
            continue
        if line.startswith("@@"):
            return None
        if not in_hunk:
            continue
        if line.startswith("\\ No newline at end of file"):
            continue
        if line.startswith("+"):
            if consumed_new >= expected_new:
                return None
            if added_start is None:
                added_start = current_line
            added_end = current_line
            saw_added = True
            consumed_new += 1
            current_line += 1
            continue
        if line.startswith("-"):
            if consumed_old >= expected_old:
                return None
            finish_added_range()
            saw_removed = True
            consumed_old += 1
            continue
        if line.startswith(" "):
            if consumed_old >= expected_old or consumed_new >= expected_new:
                return None
            finish_added_range()
            consumed_old += 1
            consumed_new += 1
            current_line += 1
            continue
        return None
    if not finish_hunk() or not saw_hunk:
        return None
    return ranges
