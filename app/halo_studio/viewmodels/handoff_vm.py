"""HandoffViewModel：交接包预览与创建。

交接包按契约构造上即不含完整对话、原始工具日志、凭据或配置文件；本层只展示。
"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal, Slot

from .base import BaseViewModel


class HandoffViewModel(BaseViewModel):
    packageChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._task_id = ""
        self._selected_files: list | None = None
        self._reset_package_fields()

    def _reset_package_fields(self) -> None:
        self._handoff_id = ""
        self._source_agent = ""
        self._target_agent = ""
        self._goal = ""
        self._summary = ""
        self._selected_changes: list = []
        self._verification_status = ""
        self._verification_detail = ""
        self._created_at = ""

    # ---- 命令 ----

    @Slot(str, "QVariantList")
    def preview(self, task_id: str, selected_files=None) -> None:
        """selected_files 为 None 时按契约默认全部关联文件。"""
        self._clear_error()
        self._task_id = task_id
        self._selected_files = None if selected_files is None else list(selected_files)
        self._client.request(
            "handoff.preview",
            {"task_id": task_id, "selected_files": self._selected_files},
            self._on_package_ok,
            self._set_error,
        )

    @Slot(str)
    def create(self, target_agent: str) -> None:
        self._clear_error()
        self._client.request(
            "handoff.create",
            {
                "task_id": self._task_id,
                "target_agent": target_agent,
                "selected_files": self._selected_files,
            },
            self._on_create_ok,
            self._set_error,
        )

    # ---- 回调 ----

    def _on_package_ok(self, result: dict) -> None:
        self._apply_package((result or {}).get("package") or {})

    def _on_create_ok(self, result: dict) -> None:
        result = result or {}
        package = dict(result.get("package") or {})
        handoff_id = result.get("handoff_id")
        if handoff_id and not package.get("handoff_id"):
            package["handoff_id"] = handoff_id
        self._apply_package(package)

    def _apply_package(self, package: dict) -> None:
        self._handoff_id = str(package.get("handoff_id") or "")
        self._source_agent = str(package.get("source_agent") or "")
        self._target_agent = str(package.get("target_agent") or "")
        self._goal = str(package.get("goal") or "")
        self._summary = str(package.get("summary") or "")
        self._selected_changes = [
            {"path": str(c.get("path") or ""), "diff": str(c.get("diff") or "")}
            for c in (package.get("selected_changes") or [])
        ]
        verification = package.get("verification") or {}
        self._verification_status = str(verification.get("status") or "")
        self._verification_detail = str(verification.get("detail") or "")
        self._created_at = str(package.get("created_at") or "")
        self.packageChanged.emit()

    # ---- 属性 ----

    def _get_task_id(self) -> str:
        return self._task_id

    def _get_handoff_id(self) -> str:
        return self._handoff_id

    def _get_source_agent(self) -> str:
        return self._source_agent

    def _get_target_agent(self) -> str:
        return self._target_agent

    def _get_goal(self) -> str:
        return self._goal

    def _get_summary(self) -> str:
        return self._summary

    def _get_selected_changes(self) -> list:
        return [dict(c) for c in self._selected_changes]

    def _get_verification_status(self) -> str:
        return self._verification_status

    def _get_verification_detail(self) -> str:
        return self._verification_detail

    def _get_created_at(self) -> str:
        return self._created_at

    taskId = Property(str, _get_task_id, notify=packageChanged)
    handoffId = Property(str, _get_handoff_id, notify=packageChanged)
    sourceAgent = Property(str, _get_source_agent, notify=packageChanged)
    targetAgent = Property(str, _get_target_agent, notify=packageChanged)
    goal = Property(str, _get_goal, notify=packageChanged)
    summary = Property(str, _get_summary, notify=packageChanged)
    selectedChanges = Property("QVariantList", _get_selected_changes, notify=packageChanged)
    verificationStatus = Property(str, _get_verification_status, notify=packageChanged)
    verificationDetail = Property(str, _get_verification_detail, notify=packageChanged)
    createdAt = Property(str, _get_created_at, notify=packageChanged)
