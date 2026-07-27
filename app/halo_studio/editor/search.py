"""当前文档的轻量查找与替换控制器。"""

from __future__ import annotations

import re
from typing import Callable

from PySide6.QtCore import Property, QObject, Signal, Slot

from .constants import MATCH_COUNT_CAP


class SearchController(QObject):
    changed = Signal()
    matchSelected = Signal(int, int)

    def __init__(self, active_document: Callable[[], object | None], parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._active_document = active_document
        self._active = False
        self._replace_visible = False
        self._query = ""
        self._replace_text = ""
        self._use_regex = False
        self._case_sensitive = False
        self._whole_word = False
        self._matches: list[re.Match] = []
        self._match_count = -1
        self._current_index = 0
        self._regex_error = ""

    @Slot(bool)
    def open(self, with_replace: bool = False) -> None:
        self._active = True
        self._replace_visible = bool(with_replace)
        self._recalculate()

    @Slot()
    def close(self) -> None:
        self._active = False
        self._current_index = 0
        self.changed.emit()

    @Slot(str)
    def setQuery(self, query: str) -> None:  # noqa: N802
        self._query = str(query)
        self._recalculate()

    @Slot(str)
    def setReplaceText(self, text: str) -> None:  # noqa: N802
        self._replace_text = str(text)
        self.changed.emit()

    @Slot(bool)
    def setUseRegex(self, value: bool) -> None:  # noqa: N802
        self._use_regex = bool(value)
        self._recalculate()

    @Slot(bool)
    def setCaseSensitive(self, value: bool) -> None:  # noqa: N802
        self._case_sensitive = bool(value)
        self._recalculate()

    @Slot(bool)
    def setWholeWord(self, value: bool) -> None:  # noqa: N802
        self._whole_word = bool(value)
        self._recalculate()

    @Slot()
    def findNext(self) -> None:  # noqa: N802
        if not self._matches:
            return
        self._current_index = self._current_index % len(self._matches) + 1
        self._emit_current_match()

    @Slot()
    def findPrevious(self) -> None:  # noqa: N802
        if not self._matches:
            return
        self._current_index = (self._current_index - 2) % len(self._matches) + 1
        self._emit_current_match()

    @Slot()
    def replaceCurrent(self) -> None:  # noqa: N802
        doc = self._active_document()
        if doc is None or not self._matches or doc.readOnly:
            return
        match = self._matches[max(0, self._current_index - 1)]
        replacement = match.expand(self._replace_text) if self._use_regex else self._replace_text
        doc.setText(doc.text[:match.start()] + replacement + doc.text[match.end():])
        self._recalculate()

    @Slot()
    def replaceAll(self) -> None:  # noqa: N802
        doc = self._active_document()
        if doc is None or not self._matches or doc.readOnly:
            return
        pattern = self._compiled_pattern()
        if pattern is None:
            return
        replacement = self._replace_text if self._use_regex else lambda _match: self._replace_text
        doc.setText(pattern.sub(replacement, doc.text))
        self._recalculate()

    @Slot(int, int, result="QVariantList")
    def visibleMatches(self, from_pos: int, to_pos: int) -> list[dict]:  # noqa: N802
        return [
            {"start": match.start(), "length": match.end() - match.start()}
            for match in self._matches
            if match.end() >= from_pos and match.start() <= to_pos
        ]

    def refresh(self) -> None:
        if self._active:
            self._recalculate()

    def _compiled_pattern(self):
        if not self._query:
            self._regex_error = ""
            return None
        source = self._query if self._use_regex else re.escape(self._query)
        if self._whole_word:
            source = rf"\b(?:{source})\b"
        flags = 0 if self._case_sensitive else re.IGNORECASE
        try:
            self._regex_error = ""
            return re.compile(source, flags)
        except re.error as exc:
            self._regex_error = f"正则表达式无效：{exc}"
            return None

    def _recalculate(self) -> None:
        pattern = self._compiled_pattern()
        doc = self._active_document()
        if pattern is None or doc is None:
            self._matches = []
            self._match_count = 0 if self._query else -1
            self._current_index = 0
            self.changed.emit()
            return
        matches = list(pattern.finditer(doc.text))
        self._matches = matches[:MATCH_COUNT_CAP]
        self._match_count = MATCH_COUNT_CAP if len(matches) > MATCH_COUNT_CAP else len(matches)
        self._current_index = 1 if self._matches else 0
        self.changed.emit()
        self._emit_current_match()

    def _emit_current_match(self) -> None:
        if self._current_index and self._current_index <= len(self._matches):
            match = self._matches[self._current_index - 1]
            self.matchSelected.emit(match.start(), match.end() - match.start())

    active = Property(bool, lambda self: self._active, notify=changed)
    replaceVisible = Property(bool, lambda self: self._replace_visible, notify=changed)
    query = Property(str, lambda self: self._query, notify=changed)
    replaceText = Property(str, lambda self: self._replace_text, notify=changed)
    useRegex = Property(bool, lambda self: self._use_regex, notify=changed)
    caseSensitive = Property(bool, lambda self: self._case_sensitive, notify=changed)
    wholeWord = Property(bool, lambda self: self._whole_word, notify=changed)
    matchCount = Property(int, lambda self: self._match_count, notify=changed)
    currentIndex = Property(int, lambda self: self._current_index, notify=changed)
    regexError = Property(str, lambda self: self._regex_error, notify=changed)
