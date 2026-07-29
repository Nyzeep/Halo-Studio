"""消费 ``task.manual_edit`` 过程事件，提供会话内的人工介入事实。"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal

from .paths import normalize_relative


class ManualEditNotifier(QObject):
    changed = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._task_id = ""
        self._paths: set[str] = set()
        client.subscribe("task.manual_edit", self._on_manual_edit)
        client.subscribe("task.state", self._on_task_state)
        client.subscribe("workspace.changed", self._on_workspace_changed)

    def clear(self) -> None:
        if self._task_id or self._paths:
            self._task_id = ""
            self._paths.clear()
            self.changed.emit()

    def _on_manual_edit(self, envelope: dict) -> None:
        task_id = str((envelope or {}).get("task_id") or "")
        if task_id and self._task_id and task_id != self._task_id:
            self._paths.clear()
        if task_id:
            self._task_id = task_id
        path = normalize_relative(str(((envelope or {}).get("payload") or {}).get("path") or ""))
        if path and path not in self._paths:
            self._paths.add(path)
            self.changed.emit()

    def _on_task_state(self, envelope: dict) -> None:
        payload = (envelope or {}).get("payload") or {}
        state = str(payload.get("state") or "")
        task = payload.get("task") or {}
        task_id = str((envelope or {}).get("task_id") or task.get("task_id") or "")
        if state == "created" and task_id and task_id != self._task_id:
            self._task_id = task_id
            if self._paths:
                self._paths.clear()
            self.changed.emit()

    def _on_workspace_changed(self, envelope: dict) -> None:
        self.clear()

    def _get_paths(self) -> list[str]:
        return sorted(self._paths, key=str.casefold)

    def _get_count(self) -> int:
        return len(self._paths)

    manualEditPaths = Property("QVariantList", _get_paths, notify=changed)
    manualEditCount = Property(int, _get_count, notify=changed)
