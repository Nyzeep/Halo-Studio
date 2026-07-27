"""任务创建草稿的显式文件与选区上下文。"""

from __future__ import annotations

from PySide6.QtCore import Property, QAbstractListModel, QModelIndex, QObject, Qt, Signal, Slot

from .paths import normalize_relative

_TEXT_MAX_BYTES = 8 * 1024
_TEXT_MAX_LINES = 200


class TaskContextFilesModel(QAbstractListModel):
    RelPathRole = int(Qt.ItemDataRole.UserRole) + 1

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._paths: list[str] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._paths)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not 0 <= index.row() < len(self._paths):
            return None
        path = self._paths[index.row()]
        return path if role in (Qt.ItemDataRole.DisplayRole, self.RelPathRole) else None

    def roleNames(self):  # noqa: N802
        return {self.RelPathRole: b"relPath"}

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        return {"relPath": self._paths[row]} if 0 <= row < len(self._paths) else {}

    def paths(self) -> list[str]:
        return list(self._paths)

    def add(self, path: str) -> bool:
        if path in self._paths:
            return False
        row = len(self._paths)
        self.beginInsertRows(QModelIndex(), row, row)
        self._paths.append(path)
        self.endInsertRows()
        return True

    def remove(self, path: str) -> bool:
        try:
            row = self._paths.index(path)
        except ValueError:
            return False
        self.beginRemoveRows(QModelIndex(), row, row)
        self._paths.pop(row)
        self.endRemoveRows()
        return True

    def clear(self) -> bool:
        if not self._paths:
            return False
        self.beginResetModel()
        self._paths.clear()
        self.endResetModel()
        return True


class TaskContextViewModel(QObject):
    filesChanged = Signal()
    hintChanged = Signal()
    notesBlockAppended = Signal(str)
    draftCleared = Signal()

    def __init__(self, editor_service, parent: QObject | None = None, *, client=None) -> None:
        super().__init__(parent)
        self._editor = editor_service
        self._files = TaskContextFilesModel(self)
        self._hint = ""
        if client is not None:
            client.subscribe("workspace.changed", self._on_workspace_changed)

    @Slot(str, result=bool)
    def addFile(self, path: str) -> bool:  # noqa: N802
        clean = normalize_relative(path)
        if not clean:
            self._set_hint("只能加入当前工作区内的文件")
            return False
        added = self._files.add(clean)
        if added:
            self.filesChanged.emit()
        return added

    @Slot(str, result=bool)
    def removeFile(self, path: str) -> bool:  # noqa: N802
        removed = self._files.remove(normalize_relative(path))
        if removed:
            self.filesChanged.emit()
        return removed

    @Slot(result="QVariantList")
    def filesList(self) -> list[str]:  # noqa: N802
        return self._files.paths()

    @Slot(result=bool)
    def addActiveEditorSelection(self) -> bool:  # noqa: N802
        selection = dict(getattr(self._editor, "currentSelection", {}) or {})
        path = normalize_relative(str(selection.get("path") or getattr(self._editor, "activeFilePath", "")))
        if not path:
            self._set_hint("没有可加入任务上下文的活动文件")
            return False
        self.addFile(path)
        if not bool(selection.get("hasSelection", False)):
            self._set_hint("")
            return True

        start = max(1, int(selection.get("startLine") or 1))
        end = max(start, int(selection.get("endLine") or start))
        text = str(selection.get("text") or "")
        too_long = (
            bool(selection.get("textTruncated", False))
            or len(text.encode("utf-8")) > _TEXT_MAX_BYTES
            or end - start + 1 > _TEXT_MAX_LINES
            or text.count("\n") + 1 > _TEXT_MAX_LINES
        )
        if too_long:
            block = f"--- 选区 {path} 第 {start}-{end} 行（内容过长未附原文，请按行号查阅）---"
            self._set_hint("选区过长，已仅附带文件与行号")
        else:
            block = f"--- 选区 {path} 第 {start}-{end} 行 ---\n{text}\n--- 选区结束 ---"
            self._set_hint("")
        self.notesBlockAppended.emit(block)
        return True

    @Slot(result=bool)
    def addActiveFile(self) -> bool:  # noqa: N802
        return self.addFile(str(getattr(self._editor, "activeFilePath", "")))

    @Slot()
    def clear(self) -> None:
        if self._files.clear():
            self.filesChanged.emit()
        self._set_hint("")
        self.draftCleared.emit()

    def _on_workspace_changed(self, _envelope: dict) -> None:
        self.clear()

    def _set_hint(self, hint: str) -> None:
        if hint != self._hint:
            self._hint = hint
            self.hintChanged.emit()

    files = Property(QObject, lambda self: self._files, constant=True)
    fileCount = Property(int, lambda self: len(self._files.paths()), notify=filesChanged)
    hint = Property(str, lambda self: self._hint, notify=hintChanged)
