"""WorkspaceViewModel：受信任 Git 工作区的打开 / 信任 / 撤销 / 关闭与状态展示。"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal, Slot

from .base import BaseViewModel


class WorkspaceViewModel(BaseViewModel):
    statusChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._reset_fields()
        client.subscribe("workspace.changed", self._on_changed)

    def _reset_fields(self) -> None:
        self._active = False
        self._workspace_id = ""
        self._real_path = ""
        self._git_root = ""
        self._root_commit = ""
        self._trust = "untrusted"
        self._identity_changed = False

    # ---- 命令 ----

    @Slot(str)
    def open(self, path: str) -> None:
        self._clear_error()
        self._client.request("workspace.open", {"path": path}, self._on_status_ok, self._set_error)

    @Slot()
    def trust(self) -> None:
        self._decide("trust")

    @Slot()
    def revoke(self) -> None:
        self._decide("revoke")

    def _decide(self, decision: str) -> None:
        self._clear_error()
        self._client.request(
            "workspace.trust",
            {"workspace_id": self._workspace_id, "decision": decision},
            self._on_status_ok,
            self._set_error,
        )

    @Slot()
    def close(self) -> None:
        self._clear_error()
        self._client.request("workspace.close", {}, self._on_closed_ok, self._set_error)

    @Slot()
    def refresh(self) -> None:
        self._client.request("workspace.status", {}, self._on_status_ok, self._set_error)

    # ---- 回调 ----

    def _on_status_ok(self, result: dict) -> None:
        self._apply_status(result or {})

    def _on_closed_ok(self, _result: dict) -> None:
        self._apply_status({"active": False})

    def _on_changed(self, envelope: dict) -> None:
        self._apply_status(envelope.get("payload") or {})

    def _apply_status(self, status: dict) -> None:
        if not status.get("active"):
            self._reset_fields()
        else:
            self._active = True
            self._workspace_id = str(status.get("workspace_id") or "")
            self._real_path = str(status.get("real_path") or "")
            self._git_root = str(status.get("git_root") or "")
            self._root_commit = str(status.get("root_commit") or "")
            self._trust = str(status.get("trust") or "untrusted")
            self._identity_changed = bool(status.get("identity_changed", False))
        self.statusChanged.emit()

    # ---- 属性 ----

    def _get_active(self) -> bool:
        return self._active

    def _get_workspace_id(self) -> str:
        return self._workspace_id

    def _get_real_path(self) -> str:
        return self._real_path

    def _get_git_root(self) -> str:
        return self._git_root

    def _get_root_commit(self) -> str:
        return self._root_commit

    def _get_trust(self) -> str:
        return self._trust

    def _get_identity_changed(self) -> bool:
        return self._identity_changed

    active = Property(bool, _get_active, notify=statusChanged)
    workspaceId = Property(str, _get_workspace_id, notify=statusChanged)
    realPath = Property(str, _get_real_path, notify=statusChanged)
    gitRoot = Property(str, _get_git_root, notify=statusChanged)
    rootCommit = Property(str, _get_root_commit, notify=statusChanged)
    trustState = Property(str, _get_trust, notify=statusChanged)
    identityChanged = Property(bool, _get_identity_changed, notify=statusChanged)
