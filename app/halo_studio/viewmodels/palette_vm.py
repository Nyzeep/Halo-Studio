"""命令面板和快速打开的单浮层 ViewModel。"""

from __future__ import annotations

from typing import Iterable

from PySide6.QtCore import Property, QAbstractListModel, QModelIndex, QObject, QSettings, Qt, Signal, Slot

from halo_studio.commands.fuzzy import score_command_candidate, score_file_candidate


class PaletteResultsModel(QAbstractListModel):
    LabelRole = int(Qt.ItemDataRole.UserRole) + 1
    DescriptionRole = LabelRole + 1
    MatchedIndicesRole = LabelRole + 2
    MatchedOnRole = LabelRole + 3
    GroupRole = LabelRole + 4
    ItemKindRole = LabelRole + 5
    ItemIdRole = LabelRole + 6

    _ROLE_NAMES = {
        LabelRole: b"label",
        DescriptionRole: b"description",
        MatchedIndicesRole: b"matchedIndices",
        MatchedOnRole: b"matchedOn",
        GroupRole: b"group",
        ItemKindRole: b"itemKind",
        ItemIdRole: b"itemId",
    }

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._rows: list[dict] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._rows)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._rows)):
            return None
        row = self._rows[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return row["label"]
        key = {
            self.LabelRole: "label",
            self.DescriptionRole: "description",
            self.MatchedIndicesRole: "matchedIndices",
            self.MatchedOnRole: "matchedOn",
            self.GroupRole: "group",
            self.ItemKindRole: "itemKind",
            self.ItemIdRole: "itemId",
        }.get(role)
        return row.get(key) if key else None

    def roleNames(self):  # noqa: N802
        return dict(self._ROLE_NAMES)

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        return dict(self._rows[row]) if 0 <= row < len(self._rows) else {}

    def replace(self, rows: Iterable[dict]) -> None:
        self.beginResetModel()
        self._rows = [dict(row) for row in rows]
        self.endResetModel()


class PaletteViewModel(QObject):
    visibleChanged = Signal()
    busyChanged = Signal()
    queryChanged = Signal()
    selectedIndexChanged = Signal()
    hintChanged = Signal()

    def __init__(self, registry, file_index, editor, settings: QSettings | None = None, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._registry = registry
        self._index = file_index
        self._editor = editor
        self._settings = settings or QSettings()
        self._results = PaletteResultsModel(self)
        self._visible = False
        self._busy = False
        self._query = ""
        self._selected_index = -1
        self._hint = ""
        self._recent_commands = _read_list(self._settings, "palette/recentCommands", 20)
        self._recent_files = _read_list(self._settings, "palette/recentFiles", 30)
        file_index.refreshed.connect(self._on_index_refreshed)
        file_index.failed.connect(self._on_index_failed)
        file_index.busyChanged.connect(self._sync_busy)
        registry.commandExecuted.connect(self._record_command)
        registry.commandsChanged.connect(self._rebuild)

    @Slot(str)
    def open(self, prefill: str = "") -> None:
        self._visible = True
        self.visibleChanged.emit()
        self._set_query(prefill)
        if not self._is_command_mode:
            self._index.ensure_fresh()
        self._rebuild()

    @Slot()
    def close(self) -> None:
        if self._visible:
            self._visible = False
            self.visibleChanged.emit()

    @Slot(str)
    def setQuery(self, query: str) -> None:  # noqa: N802
        self._set_query(query)
        if self._visible and not self._is_command_mode:
            self._index.ensure_fresh()
        self._rebuild()

    @Slot(int)
    def moveSelection(self, delta: int) -> None:  # noqa: N802
        count = self._results.rowCount()
        if count == 0:
            self._set_selected(-1)
            return
        current = self._selected_index if self._selected_index >= 0 else 0
        self._set_selected((current + int(delta)) % count)

    @Slot()
    def acceptSelected(self) -> None:  # noqa: N802
        row = self._results.get(self._selected_index)
        if not row:
            return
        self.close()
        if row["itemKind"] == "command":
            self._registry.execute(row["itemId"])
            return
        callback = getattr(self._editor, "openFile", None)
        if callback is not None:
            callback(row["itemId"])
            self._record_file(row["itemId"])

    def _set_query(self, query: str) -> None:
        query = str(query)
        if query != self._query:
            self._query = query
            self.queryChanged.emit()

    @property
    def _is_command_mode(self) -> bool:
        return self._query.startswith(">")

    @property
    def _effective_query(self) -> str:
        return self._query[1:].strip() if self._is_command_mode else self._query.strip()

    def _rebuild(self) -> None:
        if self._is_command_mode:
            rows = self._command_rows()
        else:
            rows = self._file_rows()
        self._results.replace(rows)
        self._set_selected(0 if rows else -1)
        hint = ""
        if not rows and self._effective_query:
            hint = "无匹配结果"
        elif not self._is_command_mode and self._index.truncated:
            hint = "文件清单已截断"
        if hint != self._hint:
            self._hint = hint
            self.hintChanged.emit()

    def _command_rows(self) -> list[dict]:
        query = self._effective_query
        commands = [command for command in self._registry.snapshot() if self._registry.is_enabled(command.id)]
        if not query:
            recent = [command for command in commands if command.id in self._recent_commands]
            recent.sort(key=lambda command: self._recent_commands.index(command.id))
            remaining = [command for command in commands if command.id not in self._recent_commands]
            return [
                _command_row(command, "最近使用" if command in recent else "全部命令")
                for command in [*recent, *remaining]
            ]
        scored = []
        for command in commands:
            item = score_command_candidate(query, f"{command.category}: {command.title}", command.id)
            if item is not None:
                scored.append((item, command))
        scored.sort(key=lambda pair: (-pair[0].score, pair[1].title, pair[1].id))
        return [_command_row(command, "", item) for item, command in scored[:100]]

    def _file_rows(self) -> list[dict]:
        query = self._effective_query
        _generation, paths = self._index.snapshot()
        if not query:
            candidates = [path for path in self._recent_files if path in paths]
            return [_file_row(path, "最近打开") for path in candidates]
        scored = []
        for path in paths:
            item = score_file_candidate(query, path)
            if item is not None:
                scored.append((item, path))
        scored.sort(key=lambda pair: (-pair[0].score, len(pair[1].rsplit("/", 1)[-1]), len(pair[1]), pair[1].casefold()))
        return [_file_row(path, "", item) for item, path in scored[:100]]

    def _on_index_refreshed(self) -> None:
        self._sync_busy()
        if self._visible and not self._is_command_mode:
            self._rebuild()

    def _on_index_failed(self, message: str) -> None:
        if message != self._hint:
            self._hint = message
            self.hintChanged.emit()

    def _sync_busy(self) -> None:
        busy = self._index.busy
        if busy != self._busy:
            self._busy = busy
            self.busyChanged.emit()

    def _set_selected(self, value: int) -> None:
        if value != self._selected_index:
            self._selected_index = value
            self.selectedIndexChanged.emit()

    def _record_command(self, command_id: str) -> None:
        self._recent_commands = _promote(self._recent_commands, command_id, 20)
        self._settings.setValue("palette/recentCommands", self._recent_commands)

    def _record_file(self, path: str) -> None:
        self._recent_files = _promote(self._recent_files, path, 30)
        self._settings.setValue("palette/recentFiles", self._recent_files)

    visible = Property(bool, lambda self: self._visible, notify=visibleChanged)
    busy = Property(bool, lambda self: self._busy, notify=busyChanged)
    query = Property(str, lambda self: self._query, notify=queryChanged)
    hint = Property(str, lambda self: self._hint, notify=hintChanged)
    selectedIndex = Property(int, lambda self: self._selected_index, notify=selectedIndexChanged)
    results = Property(QObject, lambda self: self._results, constant=True)


def _command_row(command, group: str, scored=None) -> dict:
    label = f"{command.category}: {command.title}"
    return {
        "label": label,
        "description": command.shortcut or command.id,
        "matchedIndices": scored.matched_indices if scored and scored.matched_on == "label" else [],
        "matchedOn": scored.matched_on if scored else "label",
        "group": group,
        "itemKind": "command",
        "itemId": command.id,
    }


def _file_row(path: str, group: str, scored=None) -> dict:
    label = path.rsplit("/", 1)[-1]
    directory = path.rsplit("/", 1)[0] if "/" in path else ""
    return {
        "label": label,
        "description": directory,
        "matchedIndices": scored.matched_indices if scored and scored.matched_on == "basename" else [],
        "matchedOn": scored.matched_on if scored else "basename",
        "group": group,
        "itemKind": "file",
        "itemId": path,
    }


def _read_list(settings: QSettings, key: str, limit: int) -> list[str]:
    value = settings.value(key, [])
    if isinstance(value, str):
        value = [value]
    return [str(item) for item in (value or [])][:limit]


def _promote(values: list[str], value: str, limit: int) -> list[str]:
    return [value, *[item for item in values if item != value]][:limit]
