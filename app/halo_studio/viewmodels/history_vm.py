"""HistoryViewModel：本地交付历史（任务 + 决定）列表。"""

from __future__ import annotations

from PySide6.QtCore import (
    Property,
    QAbstractListModel,
    QModelIndex,
    QObject,
    Qt,
    Signal,
    Slot,
)

from .base import BaseViewModel


class HistoryTaskListModel(QAbstractListModel):
    TaskIdRole = int(Qt.ItemDataRole.UserRole) + 1
    AgentRole = TaskIdRole + 1
    TitleRole = TaskIdRole + 2
    StateRole = TaskIdRole + 3
    AttributionRole = TaskIdRole + 4
    CancelModeRole = TaskIdRole + 5
    LatestEvidenceVersionRole = TaskIdRole + 6
    CreatedAtRole = TaskIdRole + 7
    EndedAtRole = TaskIdRole + 8

    _ROLE_KEYS = {
        TaskIdRole: "task_id",
        AgentRole: "agent",
        TitleRole: "title",
        StateRole: "state",
        AttributionRole: "attribution",
        CancelModeRole: "cancel_mode",
        LatestEvidenceVersionRole: "latest_evidence_version",
        CreatedAtRole: "created_at",
        EndedAtRole: "ended_at",
    }

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._tasks: list[dict] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._tasks)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._tasks)):
            return None
        task = self._tasks[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return task.get("title")
        key = self._ROLE_KEYS.get(role)
        return task.get(key) if key else None

    def roleNames(self):  # noqa: N802
        return {
            self.TaskIdRole: b"taskId",
            self.AgentRole: b"agent",
            self.TitleRole: b"title",
            self.StateRole: b"state",
            self.AttributionRole: b"attribution",
            self.CancelModeRole: b"cancelMode",
            self.LatestEvidenceVersionRole: b"latestEvidenceVersion",
            self.CreatedAtRole: b"createdAt",
            self.EndedAtRole: b"endedAt",
        }

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if 0 <= row < len(self._tasks):
            return dict(self._tasks[row])
        return {}

    def reset_with(self, tasks: list[dict]) -> None:
        self.beginResetModel()
        self._tasks = [dict(t) for t in tasks]
        self.endResetModel()


class HistoryViewModel(BaseViewModel):
    historyChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._tasks_model = HistoryTaskListModel(self)
        self._decisions: list[dict] = []

    @Slot()
    @Slot(int)
    def list(self, limit: int = 50) -> None:
        self._clear_error()
        self._client.request("history.list", {"limit": int(limit)}, self._on_list_ok, self._set_error)

    def _on_list_ok(self, result: dict) -> None:
        result = result or {}
        self._tasks_model.reset_with(list(result.get("tasks") or []))
        self._decisions = [dict(d) for d in (result.get("decisions") or [])]
        self.historyChanged.emit()

    # ---- 属性 ----

    def _get_tasks_model(self) -> QObject:
        return self._tasks_model

    def _get_decisions(self) -> list:
        return [dict(d) for d in self._decisions]

    tasks = Property(QObject, _get_tasks_model, constant=True)
    decisions = Property("QVariantList", _get_decisions, notify=historyChanged)
