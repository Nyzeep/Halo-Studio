"""把最新交付证据投影为资源管理器和编辑器标签的文件级徽章。"""

from __future__ import annotations

from PySide6.QtCore import QObject

from halo_studio.viewmodels.explorer_viewmodel import Decoration

from .latest_review import LatestReviewLifecycle
from .paths import review_path_to_editor_path

_CHANGE_DECORATIONS = {
    "modified": ("M", "decorationModifiedForeground"),
    "added": ("A", "decorationAddedForeground"),
    "deleted": ("D", "decorationDeletedForeground"),
    "renamed": ("R", "decorationModifiedForeground"),
}
class BaselineBadgeController(QObject):
    def __init__(self, client, explorer_vm, editor_service, workspace_vm, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._explorer = explorer_vm
        self._editor = editor_service
        self._workspace = workspace_vm
        self._latest = LatestReviewLifecycle(client, self.apply_bundle, self._clear_projection)
        if hasattr(workspace_vm, "statusChanged"):
            workspace_vm.statusChanged.connect(self._on_workspace_status_changed)

    def sync_task(self, task_id: str, state: str, evidence_version: int = 0) -> None:
        self._latest.sync_task(task_id, state, evidence_version)

    def apply_bundle(self, bundle: dict) -> None:
        if bundle.get("is_latest") is False:
            self._clear_projection()
            return
        version = int(bundle.get("evidence_version") or 0)
        decorations: dict[str, Decoration] = {}
        paths: list[str] = []
        for item in list(bundle.get("files") or []):
            path = review_path_to_editor_path(
                str(item.get("path") or ""),
                str(getattr(self._workspace, "realPath", "")),
                str(getattr(self._workspace, "gitRoot", "")),
            )
            if not path:
                continue
            letter, token = _CHANGE_DECORATIONS.get(str(item.get("change") or ""), ("M", "decorationModifiedForeground"))
            decorations[path] = Decoration(
                letter=letter,
                color_token=token,
                tooltip=f"任务基线以来已变更（证据 v{version}）",
                bubble=True,
            )
            paths.append(path)
        self._explorer.model.set_decorations(decorations)
        self._editor.setBaselineChangedPaths(paths)

    def clear(self) -> None:
        self._latest.clear()

    def _clear_projection(self) -> None:
        self._explorer.model.set_decorations({})
        self._editor.setBaselineChangedPaths([])

    def _on_workspace_status_changed(self) -> None:
        if not bool(getattr(self._workspace, "active", False)):
            self.clear()
