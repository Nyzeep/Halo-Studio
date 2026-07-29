"""只读审查到当前工作树编辑器的保守跳转。"""

from __future__ import annotations

from PySide6.QtCore import QObject, Signal, Slot

from .diffparse import first_target_line
from .paths import review_path_to_editor_path


class ReviewJumpViewModel(QObject):
    changed = Signal()
    openInEditorRequested = Signal(str, int)

    def __init__(self, review_vm, workspace_vm, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._review = review_vm
        self._workspace = workspace_vm
        self._current: tuple[str, str, str, bool] | None = None
        if review_vm is not None and hasattr(review_vm, "bundleChanged"):
            review_vm.bundleChanged.connect(self.changed)
        if workspace_vm is not None and hasattr(workspace_vm, "statusChanged"):
            workspace_vm.statusChanged.connect(self.changed)

    @Slot(str, str, str, bool, result="QVariantMap")
    def describe(self, path: str, change: str, diff: str, truncated: bool) -> dict:
        if str(change) == "deleted":
            return _result("", -1, False, "文件已在交付中删除")
        editor_path = review_path_to_editor_path(
            path,
            str(getattr(self._workspace, "realPath", "")),
            str(getattr(self._workspace, "gitRoot", "")),
        )
        if not editor_path:
            return _result("", -1, False, "该文件位于当前打开的子目录之外")
        line = -1 if bool(truncated) else first_target_line(diff)
        try:
            version = int(getattr(self._review, "evidenceVersion", 0) or 0)
        except (TypeError, ValueError):
            version = 0
        return _result(editor_path, line, True, f"定位基于证据版本 v{version}，文件此后再编辑可能已漂移")

    @Slot(str, str, str, bool)
    def openFile(self, path: str, change: str, diff: str, truncated: bool) -> None:  # noqa: N802
        info = self.describe(path, change, diff, truncated)
        if info["canOpen"]:
            self.openInEditorRequested.emit(info["editorPath"], info["editorLine"])

    @Slot(str, str, str, bool)
    def setCurrentFile(self, path: str, change: str, diff: str, truncated: bool) -> None:  # noqa: N802
        self._current = (path, change, diff, bool(truncated))
        self.changed.emit()

    @Slot()
    def openCurrent(self) -> None:  # noqa: N802
        if self._current is not None:
            self.openFile(*self._current)


def _result(editor_path: str, line: int, can_open: bool, reason: str) -> dict:
    return {
        "editorPath": editor_path,
        "editorLine": int(line),
        "canOpen": bool(can_open),
        "reason": reason,
    }
