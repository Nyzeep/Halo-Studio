"""TaskViewModel：Agent 任务创建表单、生命周期命令与当前 TaskStatus 展示。

任务只携带用户显式提供的内容（TaskSpec），绝不自动附带完整工作区或历史。
"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal, Slot

from .base import BaseViewModel


class TaskViewModel(BaseViewModel):
    formChanged = Signal()
    taskChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        # 创建表单字段（用户显式输入）
        self._agent = "pi"
        self._config_id = ""
        self._title = ""
        self._instructions = ""
        self._files: list = []
        self._base_diff = ""
        self._notes = ""
        self._handoff_id = ""
        # 当前 TaskStatus
        self._reset_task_fields()
        client.subscribe("task.state", self._on_task_state)
        client.subscribe("task.cancelled", self._on_task_cancelled)
        client.subscribe("task.manual_edit", self._on_manual_edit)

    def _reset_task_fields(self) -> None:
        self._task_id = ""
        self._task_agent = ""
        self._task_title = ""
        self._state = ""
        self._attribution = ""
        self._cancel_mode = ""
        self._latest_evidence_version = 0
        self._baseline_head = ""
        self._created_at = ""
        self._ended_at = ""

    # ---- 命令 ----

    @Slot()
    def create(self) -> None:
        self._clear_error()
        spec = {
            "agent": self._agent,
            "config_id": self._config_id,
            "title": self._title,
            "instructions": self._instructions,
            "files": list(self._files),
            "base_diff": self._base_diff or None,
            "notes": self._notes or None,
            "handoff_id": self._handoff_id or None,
        }
        self._client.request("task.create", spec, self._on_task_ok, self._set_error)

    @Slot()
    def cancel(self) -> None:
        self._clear_error()
        # result 仅为 {"accepted": true}，最终取消方式经 task.cancelled 事件到达
        self._client.request("task.cancel", {"task_id": self._task_id}, None, self._set_error)

    @Slot(str)
    def markManualEdit(self, note: str) -> None:  # noqa: N802
        self._clear_error()

        def on_ok(result: dict) -> None:
            attribution = result.get("attribution")
            if attribution and attribution != self._attribution:
                self._attribution = str(attribution)
                self.taskChanged.emit()

        self._client.request(
            "task.mark_manual_edit", {"task_id": self._task_id, "note": note}, on_ok, self._set_error
        )

    @Slot(str)
    def markVerificationNotRun(self, note: str) -> None:  # noqa: N802
        self._clear_error()
        self._client.request(
            "task.mark_verification",
            {"task_id": self._task_id, "status": "not_run", "note": note},
            None,
            self._set_error,
        )

    @Slot()
    def refresh(self) -> None:
        self._client.request("task.status", {}, self._on_task_ok, self._set_error)

    # ---- 回调 ----

    def _on_task_ok(self, result: dict) -> None:
        task = result.get("task")
        if isinstance(task, dict):
            self._apply_task(task)

    def _on_task_state(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        task = payload.get("task")
        if not isinstance(task, dict):
            return
        # 首期单任务：无当前任务时接受任何任务状态（重连恢复）；有任务时只接受同 id
        if self._task_id and task.get("task_id") != self._task_id:
            return
        self._apply_task(task)

    def _on_task_cancelled(self, envelope: dict) -> None:
        if self._task_id and envelope.get("task_id") not in (None, self._task_id):
            return
        mode = (envelope.get("payload") or {}).get("mode")
        if mode and mode != self._cancel_mode:
            self._cancel_mode = str(mode)
            self.taskChanged.emit()

    def _on_manual_edit(self, envelope: dict) -> None:
        if self._task_id and envelope.get("task_id") not in (None, self._task_id):
            return
        if self._attribution != "mixed":
            self._attribution = "mixed"
            self.taskChanged.emit()

    def _apply_task(self, task: dict) -> None:
        self._task_id = str(task.get("task_id") or "")
        self._task_agent = str(task.get("agent") or "")
        self._task_title = str(task.get("title") or "")
        self._state = str(task.get("state") or "")
        self._attribution = str(task.get("attribution") or "")
        self._cancel_mode = str(task.get("cancel_mode") or "")
        self._latest_evidence_version = int(task.get("latest_evidence_version") or 0)
        baseline = task.get("baseline") or {}
        self._baseline_head = str(baseline.get("head") or "")
        self._created_at = str(task.get("created_at") or "")
        self._ended_at = str(task.get("ended_at") or "")
        self.taskChanged.emit()

    # ---- 表单属性（可读写）----

    def _get_agent(self) -> str:
        return self._agent

    def _set_agent(self, value: str) -> None:
        if value != self._agent:
            self._agent = value
            self.formChanged.emit()

    def _get_config_id(self) -> str:
        return self._config_id

    def _set_config_id(self, value: str) -> None:
        if value != self._config_id:
            self._config_id = value
            self.formChanged.emit()

    def _get_title(self) -> str:
        return self._title

    def _set_title(self, value: str) -> None:
        if value != self._title:
            self._title = value
            self.formChanged.emit()

    def _get_instructions(self) -> str:
        return self._instructions

    def _set_instructions(self, value: str) -> None:
        if value != self._instructions:
            self._instructions = value
            self.formChanged.emit()

    def _get_files(self) -> list:
        return list(self._files)

    def _set_files(self, value) -> None:
        value = list(value or [])
        if value != self._files:
            self._files = value
            self.formChanged.emit()

    def _get_base_diff(self) -> str:
        return self._base_diff

    def _set_base_diff(self, value: str) -> None:
        if value != self._base_diff:
            self._base_diff = value
            self.formChanged.emit()

    def _get_notes(self) -> str:
        return self._notes

    def _set_notes(self, value: str) -> None:
        if value != self._notes:
            self._notes = value
            self.formChanged.emit()

    def _get_handoff_id(self) -> str:
        return self._handoff_id

    def _set_handoff_id(self, value: str) -> None:
        if value != self._handoff_id:
            self._handoff_id = value
            self.formChanged.emit()

    agent = Property(str, _get_agent, _set_agent, notify=formChanged)
    configId = Property(str, _get_config_id, _set_config_id, notify=formChanged)
    title = Property(str, _get_title, _set_title, notify=formChanged)
    instructions = Property(str, _get_instructions, _set_instructions, notify=formChanged)
    files = Property("QVariantList", _get_files, _set_files, notify=formChanged)
    baseDiff = Property(str, _get_base_diff, _set_base_diff, notify=formChanged)
    notes = Property(str, _get_notes, _set_notes, notify=formChanged)
    handoffId = Property(str, _get_handoff_id, _set_handoff_id, notify=formChanged)

    # ---- 当前 TaskStatus 属性（只读）----

    def _get_task_id(self) -> str:
        return self._task_id

    def _get_task_agent(self) -> str:
        return self._task_agent

    def _get_task_title(self) -> str:
        return self._task_title

    def _get_state(self) -> str:
        return self._state

    def _get_attribution(self) -> str:
        return self._attribution

    def _get_cancel_mode(self) -> str:
        return self._cancel_mode

    def _get_latest_evidence_version(self) -> int:
        return self._latest_evidence_version

    def _get_baseline_head(self) -> str:
        return self._baseline_head

    def _get_created_at(self) -> str:
        return self._created_at

    def _get_ended_at(self) -> str:
        return self._ended_at

    taskId = Property(str, _get_task_id, notify=taskChanged)
    taskAgent = Property(str, _get_task_agent, notify=taskChanged)
    taskTitle = Property(str, _get_task_title, notify=taskChanged)
    state = Property(str, _get_state, notify=taskChanged)
    attribution = Property(str, _get_attribution, notify=taskChanged)
    cancelMode = Property(str, _get_cancel_mode, notify=taskChanged)
    latestEvidenceVersion = Property(int, _get_latest_evidence_version, notify=taskChanged)
    baselineHead = Property(str, _get_baseline_head, notify=taskChanged)
    createdAt = Property(str, _get_created_at, notify=taskChanged)
    endedAt = Property(str, _get_ended_at, notify=taskChanged)
