"""命令可用性的最小平面上下文。"""

from __future__ import annotations

import logging

from PySide6.QtCore import Property, QObject, Signal, Slot

_LOG = logging.getLogger(__name__)
_KNOWN_KEYS = {"hasWorkspace", "hasActiveEditor", "taskRunning"}


class WhenContext(QObject):
    changed = Signal()

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._values = {key: False for key in _KNOWN_KEYS}
        self._warned_unknown: set[str] = set()

    @Slot(str, bool)
    def set_key(self, key: str, value: bool) -> None:
        if key not in _KNOWN_KEYS:
            raise ValueError(f"未知 when 上下文键：{key}")
        value = bool(value)
        if self._values[key] != value:
            self._values[key] = value
            self.changed.emit()

    def evaluate(self, expr: str | None) -> bool:
        if not expr or not str(expr).strip():
            return True
        for raw_term in str(expr).split("&&"):
            term = raw_term.strip()
            negated = term.startswith("!")
            key = term[1:].strip() if negated else term
            if not key or key not in _KNOWN_KEYS:
                if key not in self._warned_unknown:
                    self._warned_unknown.add(key)
                    _LOG.warning("未知 when 上下文键：%s", key)
                return False
            value = self._values[key]
            if (not value if negated else value) is False:
                return False
        return True

    hasWorkspace = Property(bool, lambda self: self._values["hasWorkspace"], notify=changed)
    hasActiveEditor = Property(bool, lambda self: self._values["hasActiveEditor"], notify=changed)
    taskRunning = Property(bool, lambda self: self._values["taskRunning"], notify=changed)
