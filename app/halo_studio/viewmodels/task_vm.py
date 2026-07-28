"""TaskViewModel：Agent 任务创建表单、生命周期命令与当前 TaskStatus 展示。

任务只携带用户显式提供的内容（TaskSpec），绝不自动附带完整工作区或历史。
"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal, Slot

from .base import BaseViewModel


_TERMINAL_TASK_STATES = frozenset(
    {"accepted", "rejected", "cancelled", "failed", "interrupted"}
)


class TaskViewModel(BaseViewModel):
    formChanged = Signal()
    taskChanged = Signal()
    taskCreated = Signal()
    sessionChanged = Signal()
    actionRequestsChanged = Signal()

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
        # 活动会话记录仅来自 task.snapshot / task.session_message；不复用运行轨迹。
        self._session_messages: list[dict] = []
        self._snapshot_session_messages: list[dict] = []
        self._session_events: dict[int, dict] = {}
        self._session_snapshot_seq = 0
        # 操作请求也只保留在活动任务内存中；不保留开发者的澄清回答。
        self._action_requests: list[dict] = []
        self._resolved_action_request_ids: set[str] = set()
        self._action_resolution_blocked = False
        # 当前 TaskStatus
        self._reset_task_fields()
        client.subscribe("task.state", self._on_task_state)
        client.subscribe("task.session_message", self._on_session_message)
        client.subscribe("task.action_request", self._on_task_action_request)
        client.subscribe("task.action_resolved", self._on_task_action_resolved)
        client.subscribe("task.cancelled", self._on_task_cancelled)
        client.subscribe("task.manual_edit", self._on_manual_edit)
        client.subscribe("workspace.changed", self._on_workspace_changed)

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
        self._reset_session_messages()

    def _reset_session_messages(self) -> None:
        had_messages = bool(self._session_messages)
        had_action_requests = bool(self._action_requests)
        was_action_resolution_blocked = self._action_resolution_blocked
        self._session_messages = []
        self._snapshot_session_messages = []
        self._session_events = {}
        self._session_snapshot_seq = 0
        self._action_requests = []
        self._resolved_action_request_ids = set()
        self._action_resolution_blocked = False
        if had_messages:
            self.sessionChanged.emit()
        if had_action_requests or was_action_resolution_blocked:
            self.actionRequestsChanged.emit()

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
        self._client.request("task.create", spec, self._on_create_ok, self._set_error)

    @Slot()
    def cancel(self) -> None:
        self._clear_error()
        if not self._task_id:
            self._set_error({"code": "TASK_NOT_FOUND", "message": "当前没有可取消的任务"})
            return
        # result 仅为 {"accepted": true}，最终取消方式经 task.cancelled 事件到达
        self._set_action_resolution_blocked(True)
        self._client.request(
            "task.cancel",
            {"task_id": self._task_id},
            self._on_cancel_ok,
            self._on_cancel_error,
        )

    @Slot(str)
    def allowOnce(self, request_id: str) -> None:  # noqa: N802
        """只向当前权限请求发送一次性允许。"""
        self._submit_action_decision(request_id, "permission", "allow_once", None)

    @Slot(str)
    def rejectAction(self, request_id: str) -> None:  # noqa: N802
        """拒绝当前权限或澄清请求。"""
        self._submit_action_decision(request_id, None, "reject", None)

    @Slot(str, str)
    def answerClarification(self, request_id: str, answer: str) -> None:  # noqa: N802
        """回答当前澄清请求；回答仅用于本次 IPC 调用。"""
        self._submit_action_decision(request_id, "clarification", "answer", answer)

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
            if self._task_id:
                self._request_session_snapshot(self._task_id)
        elif self._task_id:
            self._reset_task_fields()
            self.taskChanged.emit()

    def _on_create_ok(self, result: dict) -> None:
        self._on_task_ok(result)
        if isinstance((result or {}).get("task"), dict):
            self.taskCreated.emit()

    def _on_task_state(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        task = payload.get("task")
        if not isinstance(task, dict):
            return
        # 首期单任务：无当前任务时接受任何任务状态（重连恢复）；有任务时只接受同 id
        if self._task_id and task.get("task_id") != self._task_id:
            return
        previous_state = self._state
        self._apply_task(task)
        # 仅在 Sidecar 收到匹配请求的真实 Agent 反馈后才会离开 awaiting_action。
        # 这时候请求已不再是可决议的当前记录，可安全从卡片列表移除。
        if previous_state == "awaiting_action" and self._state != "awaiting_action":
            self._set_action_requests([])

    def _on_session_message(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        event_task_id = str(envelope.get("task_id") or payload.get("task_id") or "")
        if not self._task_id or event_task_id != self._task_id:
            return
        seq = envelope.get("seq")
        if not isinstance(seq, int) or seq <= self._session_snapshot_seq or seq in self._session_events:
            return
        message = self._normalize_session_message(payload)
        if message is None:
            return
        self._session_events[seq] = message
        self._sync_session_messages()

    def _on_task_action_request(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        event_task_id = str(envelope.get("task_id") or payload.get("task_id") or "")
        if (
            not self._task_id
            or event_task_id != self._task_id
            or self._state in _TERMINAL_TASK_STATES
        ):
            return
        action_request = self._normalize_action_request(payload)
        if action_request is None:
            return
        if action_request["request_id"] in self._resolved_action_request_ids:
            # 迟到的重复事件不能复活已被原生 Agent 确认的请求。
            return

        action_requests = list(self._action_requests)
        for index, current in enumerate(action_requests):
            if current["request_id"] != action_request["request_id"]:
                continue
            # 重复的旧事件不能撤销已运送的一次性决议。
            if current["decision_sent"]:
                action_request["decision_sent"] = True
            action_requests[index] = action_request
            self._set_action_requests(action_requests)
            return
        action_requests.append(action_request)
        self._set_action_requests(action_requests)

    def _on_task_action_resolved(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        event_task_id = str(envelope.get("task_id") or payload.get("task_id") or "")
        request_id = payload.get("request_id")
        if (
            not self._task_id
            or event_task_id != self._task_id
            or not isinstance(request_id, str)
            or not request_id
        ):
            return

        action_requests = [dict(item) for item in self._action_requests]
        for index, action_request in enumerate(action_requests):
            if action_request["request_id"] != request_id:
                continue
            # 只接受 Sidecar 已确认送达的一次性决议的真实回执。
            if action_request["decision_sent"]:
                self._resolved_action_request_ids.add(request_id)
                del action_requests[index]
                self._set_action_requests(action_requests)
            return

    def _on_task_cancelled(self, envelope: dict) -> None:
        event_task_id = str(envelope.get("task_id") or self._task_id)
        if self._task_id and event_task_id != self._task_id:
            return
        mode = (envelope.get("payload") or {}).get("mode")
        if mode and mode != self._cancel_mode:
            self._cancel_mode = str(mode)
            self.taskChanged.emit()
        if event_task_id:
            self._refresh_terminal_task(event_task_id)

    def _on_cancel_ok(self, result: dict) -> None:
        if (result or {}).get("accepted") is True:
            return
        # 取消请求已发出时，Sidecar 可能已经清空请求并进入取消流程；即使响应
        # 没有确认接受，也不能由 UI 重新开放一次性决议。
        self._set_error({"code": "TASK_RUNNING", "message": "取消请求未被接受"})

    def _on_cancel_error(self, error: dict) -> None:
        self._set_error(error)

    def _on_manual_edit(self, envelope: dict) -> None:
        if self._task_id and envelope.get("task_id") not in (None, self._task_id):
            return
        if self._attribution != "mixed":
            self._attribution = "mixed"
            self.taskChanged.emit()

    def _on_workspace_changed(self, _envelope: dict) -> None:
        if not self._task_id:
            return
        self._reset_task_fields()
        self.taskChanged.emit()

    def _request_session_snapshot(self, expected_task_id: str, after_seq: int = 0) -> None:
        self._client.request(
            "task.snapshot",
            {"after_seq": after_seq},
            lambda snapshot: self._on_session_snapshot(snapshot, expected_task_id),
            lambda error: self._on_session_snapshot_error(error, expected_task_id, after_seq),
        )

    def _on_session_snapshot(self, snapshot: dict, expected_task_id: str) -> None:
        task = (snapshot or {}).get("task")
        if not isinstance(task, dict) or str(task.get("task_id") or "") != expected_task_id:
            return
        self.applySnapshot(snapshot)

    def _on_session_snapshot_error(self, error: dict, expected_task_id: str, after_seq: int) -> None:
        details = error.get("details") or {}
        oldest = details.get("oldest_available_seq") if isinstance(details, dict) else None
        if (
            error.get("code") == "EVENT_GAP"
            and isinstance(oldest, int)
            and oldest > 0
            and after_seq < oldest
        ):
            self._request_session_snapshot(expected_task_id, oldest - 1)
            return
        self._set_error(error)

    def _refresh_terminal_task(self, task_id: str) -> None:
        def on_ok(result: dict) -> None:
            task = (result or {}).get("task")
            if (
                isinstance(task, dict)
                and self._task_id == task_id
                and str(task.get("task_id") or "") == task_id
            ):
                self._apply_task(task)

        self._client.request("task.status", {"task_id": task_id}, on_ok)

    @Slot("QVariantMap")
    def applySnapshot(self, snapshot: dict) -> None:  # noqa: N802
        """由 task.snapshot 重建活动会话记录，事件仅补充快照之后的记录。"""
        snapshot = snapshot or {}
        task = snapshot.get("task")
        if not isinstance(task, dict):
            if self._task_id:
                self._reset_task_fields()
                self.taskChanged.emit()
            return
        snapshot_task_id = str(task.get("task_id") or "")
        if self._task_id and snapshot_task_id != self._task_id:
            return

        last_seq = snapshot.get("last_seq")
        if not isinstance(last_seq, int):
            last_seq = 0
        # 迟到快照不能覆盖已经消费到的较新会话事件。
        if last_seq < self._session_snapshot_seq:
            return

        messages = snapshot.get("session_messages")
        if not isinstance(messages, list):
            return

        action_requests = snapshot.get("action_requests")
        if not isinstance(action_requests, list):
            action_requests = []

        # task.status / task.state 是任务状态事实来源；快照只负责恢复会话记录，
        # 因而不得用迟到快照回退已显示的生命周期状态。
        if not self._task_id:
            self._apply_task(task)
        snapshot_messages = []
        snapshot_action_requests = []
        snapshot_is_active = str(task.get("state") or "") not in _TERMINAL_TASK_STATES
        can_restore_action_requests = (
            self._state == "awaiting_action" and not self._action_resolution_blocked
        )
        if snapshot_is_active:
            snapshot_messages = [
                normalized
                for item in messages
                if (normalized := self._normalize_session_message(item)) is not None
            ]
            if can_restore_action_requests:
                snapshot_action_requests = [
                    normalized
                    for item in action_requests
                    if (normalized := self._normalize_action_request(item)) is not None
                    and normalized["request_id"] not in self._resolved_action_request_ids
                ]
                # 旧快照不能撤销匹配请求的本地决议锁；只有后续真实 Agent
                # 事件才能从 awaiting_action 推进并移除该卡片。
                locally_sent = {
                    item["request_id"]
                    for item in self._action_requests
                    if item["decision_sent"]
                }
                for item in snapshot_action_requests:
                    if item["request_id"] in locally_sent:
                        item["decision_sent"] = True
        covered_event_seqs = self._session_events_covered_by_snapshot(snapshot_messages, last_seq)
        self._snapshot_session_messages = snapshot_messages
        self._session_snapshot_seq = last_seq
        self._session_events = {
            seq: message
            for seq, message in self._session_events.items()
            if seq not in covered_event_seqs
        }
        self._sync_session_messages()
        if can_restore_action_requests and snapshot_is_active:
            self._set_action_requests(snapshot_action_requests)

    def _normalize_action_request(self, value: object) -> dict | None:
        if not isinstance(value, dict):
            return None
        request_id = value.get("request_id")
        kind = value.get("kind")
        prompt = value.get("prompt")
        if (
            not isinstance(request_id, str)
            or not request_id
            or kind not in {"permission", "clarification"}
            or not isinstance(prompt, str)
            or not prompt
        ):
            return None
        return {
            "request_id": request_id,
            "kind": kind,
            # Sidecar 在 IPC 边界之前已对 prompt 做脱敏和限长；视图模型不补报原生日志。
            "prompt": prompt,
            "decision_sent": value.get("decision_sent") is True,
        }

    def _set_action_requests(self, action_requests: list[dict]) -> None:
        if action_requests == self._action_requests:
            return
        self._action_requests = [dict(item) for item in action_requests]
        self.actionRequestsChanged.emit()

    def _set_action_resolution_blocked(self, blocked: bool) -> None:
        if self._action_resolution_blocked == blocked:
            return
        self._action_resolution_blocked = blocked
        self.actionRequestsChanged.emit()

    def _submit_action_decision(
        self,
        request_id: str,
        expected_kind: str | None,
        decision: str,
        answer: str | None,
    ) -> None:
        self._clear_error()
        request_id = str(request_id or "")
        if self._action_resolution_blocked:
            self._set_error(
                {"code": "ACTION_REQUEST_NOT_PENDING", "message": "任务正在取消，不能再提交操作请求"}
            )
            return
        if self._state != "awaiting_action":
            self._set_error(
                {"code": "ACTION_REQUEST_NOT_PENDING", "message": "当前任务没有等待决议的操作请求"}
            )
            return
        action_request = next(
            (item for item in self._action_requests if item["request_id"] == request_id), None
        )
        if action_request is None:
            self._set_error(
                {"code": "ACTION_REQUEST_NOT_FOUND", "message": "当前任务没有匹配的操作请求"}
            )
            return
        if expected_kind is not None and action_request["kind"] != expected_kind:
            self._set_error(
                {"code": "INVALID_PARAMS", "message": "该操作请求不接受此决议"}
            )
            return
        if action_request["decision_sent"]:
            self._set_error(
                {"code": "ACTION_REQUEST_ALREADY_RESOLVED", "message": "该操作请求已经提交过一次决定"}
            )
            return
        if decision == "answer":
            answer = str(answer or "").strip()
            if not answer:
                self._set_error({"code": "INVALID_PARAMS", "message": "澄清回答不能为空"})
                return

        # 在请求还在飞行时立即禁止重复点击；任务状态不在这里推进。
        updated = [dict(item) for item in self._action_requests]
        for item in updated:
            if item["request_id"] == request_id:
                item["decision_sent"] = True
                break
        self._set_action_requests(updated)
        self._client.request(
            "task.resolve_action",
            {
                "task_id": self._task_id,
                "request_id": request_id,
                "decision": decision,
                "answer": answer if decision == "answer" else None,
            },
            lambda result: self._on_action_decision_ok(result, request_id),
            lambda error: self._on_action_decision_error(error, request_id),
        )

    def _on_action_decision_ok(self, result: dict, request_id: str) -> None:
        if (result or {}).get("accepted") is True:
            # accepted 只代表决议已送达。它不是任务状态转换，状态仍等真实 Agent 事件。
            return
        self._restore_action_request(request_id)
        self._set_error(
            {"code": "ACTION_REQUEST_NOT_PENDING", "message": "操作请求未能提交给 Agent"}
        )

    def _on_action_decision_error(self, error: dict, request_id: str) -> None:
        # 送达不确定时 Sidecar 已失败关闭；卡片只能等待终态事件清理，不能重新开放。
        if error.get("code") != "ACTION_REQUEST_NOT_PENDING":
            self._restore_action_request(request_id)
        self._set_error(error)

    def _restore_action_request(self, request_id: str) -> None:
        updated = [dict(item) for item in self._action_requests]
        for item in updated:
            if item["request_id"] == request_id:
                item["decision_sent"] = False
                break
        self._set_action_requests(updated)

    def _normalize_session_message(self, value: object) -> dict | None:
        if not isinstance(value, dict):
            return None
        role = str(value.get("role") or "")
        # IPC 现用 agent；兼容已发布的 assistant 名称但统一成界面角色。
        if role == "assistant":
            role = "agent"
        if role not in {"user", "agent"}:
            return None
        text = value.get("text")
        if not isinstance(text, str) or not text:
            return None
        return {
            "role": role,
            "text": text,
            "truncated": value.get("truncated") is True,
        }

    def _session_events_covered_by_snapshot(self, messages: list[dict], last_seq: int) -> set[int]:
        """识别快照已包含的在途事件，避免事件和快照交错时重复显示。"""
        covered = {seq for seq in self._session_events if seq <= last_seq}
        previous = self._snapshot_session_messages
        if messages[:len(previous)] != previous:
            return covered

        appended = messages[len(previous):]
        appended_index = 0
        for seq, message in sorted(self._session_events.items()):
            if appended_index >= len(appended):
                break
            if message == appended[appended_index]:
                covered.add(seq)
                appended_index += 1
        return covered

    def _sync_session_messages(self) -> None:
        messages = [
            *self._snapshot_session_messages,
            *(message for _, message in sorted(self._session_events.items())),
        ]
        if messages == self._session_messages:
            return
        self._session_messages = messages
        self.sessionChanged.emit()

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
        if self._state in _TERMINAL_TASK_STATES:
            self._reset_session_messages()
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

    def _get_session_messages(self) -> list:
        return [dict(message) for message in self._session_messages]

    def _get_action_requests(self) -> list:
        return [dict(action_request) for action_request in self._action_requests]

    def _get_action_resolution_blocked(self) -> bool:
        return self._action_resolution_blocked

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
    sessionMessages = Property("QVariantList", _get_session_messages, notify=sessionChanged)
    actionRequests = Property("QVariantList", _get_action_requests, notify=actionRequestsChanged)
    actionResolutionBlocked = Property(bool, _get_action_resolution_blocked, notify=actionRequestsChanged)
