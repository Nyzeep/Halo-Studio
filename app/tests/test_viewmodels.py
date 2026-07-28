"""视图模型层行为测试：内存 FakeClient（记录请求、可注入响应与事件）。

覆盖：AppViewModel 三属性、Runtime 独立状态、Trace 顺序与快照恢复、
Review 只读性、Task 状态流转、错误文案透传、Config 凭据红线等关键行为。
"""

from __future__ import annotations

import dataclasses

import pytest
from PySide6.QtCore import QCoreApplication

from halo_studio.viewmodels import (
    AppViewModel,
    ConfigViewModel,
    HandoffViewModel,
    HistoryViewModel,
    ReviewViewModel,
    RuntimeViewModel,
    TaskViewModel,
    TraceViewModel,
    WorkspaceViewModel,
)


@dataclasses.dataclass
class FakeRequest:
    method: str
    params: dict
    on_ok: object
    on_err: object


class FakeClient:
    """契约形状的内存测试替身：记录请求，按需注入响应与事件。"""

    def __init__(self) -> None:
        self.requests: list[FakeRequest] = []
        self._subs: dict[str, list] = {}
        self._auto_seq = 0

    # -- 视图模型消费的两个能力 --

    def request(self, method, params, on_ok=None, on_err=None):
        self.requests.append(FakeRequest(method, dict(params), on_ok, on_err))

    def subscribe(self, event, handler):
        self._subs.setdefault(event, []).append(handler)

    # -- 测试辅助 --

    def last(self) -> FakeRequest:
        assert self.requests, "尚未发出任何请求"
        return self.requests[-1]

    def ok(self, result, index=-1):
        req = self.requests[index]
        if req.on_ok is not None:
            req.on_ok(result)

    def err(self, code, message, details=None, index=-1):
        req = self.requests[index]
        if req.on_err is not None:
            req.on_err({"code": code, "message": message, "details": details or {}})

    def emit_event(self, event, payload, seq=None, task_id=None, ts="2026-07-26T08:00:00Z"):
        if seq is None:
            self._auto_seq += 1
            seq = self._auto_seq
        envelope = {
            "v": 1,
            "kind": "event",
            "seq": seq,
            "ts": ts,
            "task_id": task_id,
            "event": event,
            "payload": payload,
        }
        for handler in list(self._subs.get(event, [])):
            handler(envelope)


@pytest.fixture(scope="session")
def core_app():
    app = QCoreApplication.instance()
    if app is None:
        app = QCoreApplication([])
    return app


@pytest.fixture()
def client():
    return FakeClient()


# ---------------------------------------------------------------- AppViewModel


class TestAppViewModel:
    def test_initial_state(self, core_app, client):
        vm = AppViewModel(client)
        assert vm.sidecarConnected is False
        assert vm.protocolVersion == 0
        assert vm.unavailableReason != ""

    def test_sidecar_ready_sets_three_properties(self, core_app, client):
        vm = AppViewModel(client)
        seen = []
        vm.sidecarConnectedChanged.connect(lambda: seen.append("connected"))
        vm.protocolVersionChanged.connect(lambda: seen.append("version"))
        client.emit_event("sidecar.state", {"state": "ready", "protocol_version": 1})
        assert vm.sidecarConnected is True
        assert vm.protocolVersion == 1
        assert vm.unavailableReason == ""
        assert "connected" in seen and "version" in seen

    def test_disconnect_reason_passthrough(self, core_app, client):
        vm = AppViewModel(client)
        client.emit_event("sidecar.state", {"state": "ready", "protocol_version": 1})
        client.emit_event("client.disconnected", {"reason": "Sidecar 进程已退出（exit code 1）"})
        assert vm.sidecarConnected is False
        assert vm.unavailableReason == "Sidecar 进程已退出（exit code 1）"
        # 协议版本保留最后一次协商结果供 UI 常显
        assert vm.protocolVersion == 1


# ---------------------------------------------------------- WorkspaceViewModel


WS_STATUS = {
    "active": True,
    "workspace_id": "ws-1",
    "real_path": "D:\\repo",
    "git_root": "D:\\repo",
    "root_commit": "abc123",
    "trust": "untrusted",
    "identity_changed": False,
}


class TestWorkspaceViewModel:
    def test_open_sends_contract_request_and_applies_status(self, core_app, client):
        vm = WorkspaceViewModel(client)
        vm.open("D:\\repo")
        req = client.last()
        assert req.method == "workspace.open"
        assert req.params == {"path": "D:\\repo"}
        client.ok(WS_STATUS)
        assert vm.active is True
        assert vm.workspaceId == "ws-1"
        assert vm.realPath == "D:\\repo"
        assert vm.trustState == "untrusted"
        assert vm.identityChanged is False

    def test_not_git_error_message_passthrough(self, core_app, client):
        vm = WorkspaceViewModel(client)
        vm.open("D:\\not a repo")
        message = "该目录不是 Git 仓库，请选择包含 .git 的工作区。"
        client.err("WORKSPACE_NOT_GIT", message)
        assert vm.errorCode == "WORKSPACE_NOT_GIT"
        assert vm.errorMessage == message
        assert vm.active is False

    def test_trust_revoke_close_request_shapes(self, core_app, client):
        vm = WorkspaceViewModel(client)
        vm.open("D:\\repo")
        client.ok(WS_STATUS)

        vm.trust()
        assert client.last().method == "workspace.trust"
        assert client.last().params == {"workspace_id": "ws-1", "decision": "trust"}
        client.ok({**WS_STATUS, "trust": "trusted"})
        assert vm.trustState == "trusted"

        vm.revoke()
        assert client.last().params == {"workspace_id": "ws-1", "decision": "revoke"}
        client.ok(WS_STATUS)
        assert vm.trustState == "untrusted"

        vm.close()
        assert client.last().method == "workspace.close"
        assert client.last().params == {}
        client.ok({"closed": True})
        assert vm.active is False
        assert vm.workspaceId == ""

    def test_workspace_changed_event_and_identity_changed(self, core_app, client):
        vm = WorkspaceViewModel(client)
        client.emit_event(
            "workspace.changed",
            {**WS_STATUS, "trust": "untrusted", "identity_changed": True},
        )
        assert vm.active is True
        assert vm.identityChanged is True
        assert vm.trustState == "untrusted"


# ------------------------------------------------------------- ConfigViewModel


LAUNCH_CONFIG = {
    "config_id": "cfg-1",
    "name": "Pi + GPT",
    "agent": "pi",
    "executable_path": "C:\\tools\\pi\\pi.exe",
    "model": "gpt-5",
    "thinking_level": "medium",
    "credential_ref": "halo/pi/openai",
    "created_at": "2026-07-26T08:00:00Z",
    "updated_at": "2026-07-26T08:00:00Z",
}


class TestConfigViewModel:
    def test_refresh_populates_model_roles(self, core_app, client):
        vm = ConfigViewModel(client)
        vm.refresh()
        assert client.last().method == "config.list"
        client.ok({"configs": [LAUNCH_CONFIG]})
        model = vm.configs
        assert model.rowCount() == 1
        idx = model.index(0, 0)
        assert model.data(idx, model.ConfigIdRole) == "cfg-1"
        assert model.data(idx, model.NameRole) == "Pi + GPT"
        assert model.data(idx, model.AgentRole) == "pi"
        assert model.data(idx, model.CredentialRefRole) == "halo/pi/openai"

    def test_save_whitelists_fields_and_never_holds_secret(self, core_app, client):
        vm = ConfigViewModel(client)
        secret = "sk-plain-secret-123456"
        vm.save(
            {
                "config_id": "cfg-existing",
                "name": "Pi + GPT",
                "agent": "pi",
                "executable_path": "C:\\tools\\pi\\pi.exe",
                "model": "gpt-5",
                "thinking_level": "low",
                "credential_ref": "halo/pi/openai",
                # 恶意/误传字段：必须在进入请求前被丢弃
                "api_key": secret,
                "credential_plaintext": secret,
                "password": secret,
            }
        )
        req = client.last()
        assert req.method == "config.save"
        assert "api_key" not in req.params
        assert "credential_plaintext" not in req.params
        assert "password" not in req.params
        assert "config_id" not in req.params
        assert secret not in repr(req.params)
        # 服务端返回的配置进入模型后同样不含明文
        client.ok({"config": {**LAUNCH_CONFIG, "api_key": secret}})
        model = vm.configs
        assert model.rowCount() == 1
        assert secret not in repr(model.get(0))

    def test_delete_removes_row(self, core_app, client):
        vm = ConfigViewModel(client)
        vm.refresh()
        client.ok({"configs": [LAUNCH_CONFIG]})
        vm.delete("cfg-1")
        assert client.last().method == "config.delete"
        assert client.last().params == {"config_id": "cfg-1"}
        client.ok({"deleted": True})
        assert vm.configs.rowCount() == 0

    def test_credential_check_updates_properties(self, core_app, client):
        vm = ConfigViewModel(client)
        vm.credentialCheck("halo/pi/openai")
        req = client.last()
        assert req.method == "config.credential_check"
        assert req.params == {"credential_ref": "halo/pi/openai"}
        client.ok({"exists": True, "store_available": True})
        assert vm.credentialCheckedRef == "halo/pi/openai"
        assert vm.credentialExists is True
        assert vm.credentialStoreAvailable is True

    def test_credential_store_unavailable_error_passthrough(self, core_app, client):
        vm = ConfigViewModel(client)
        vm.save({"name": "x", "agent": "pi", "credential_ref": "halo/pi/openai"})
        message = "操作系统凭据存储不可用，已拒绝保存（不会回退到明文文件）。"
        client.err("CREDENTIAL_STORE_UNAVAILABLE", message)
        assert vm.errorCode == "CREDENTIAL_STORE_UNAVAILABLE"
        assert vm.errorMessage == message


# ------------------------------------------------------------ RuntimeViewModel


class TestRuntimeViewModel:
    def test_initial_states_independent(self, core_app, client):
        vm = RuntimeViewModel(client)
        assert vm.piState == "not_probed"
        assert vm.opencodeState == "not_probed"
        assert vm.piReason == "" and vm.opencodeReason == ""

    def test_state_event_updates_only_target_agent(self, core_app, client):
        vm = RuntimeViewModel(client)
        client.emit_event(
            "runtime.state",
            {
                "agent": "pi",
                "state": "failed",
                "reason": "Pi 启动后未通过就绪检查",
                "recovery_hint": "请检查可执行文件路径与凭据引用",
                "version": None,
            },
        )
        assert vm.piState == "failed"
        assert vm.piReason == "Pi 启动后未通过就绪检查"
        assert vm.piRecoveryHint == "请检查可执行文件路径与凭据引用"
        # opencode 完全不受影响：绝无全局在线状态
        assert vm.opencodeState == "not_probed"
        assert vm.opencodeReason == ""

        client.emit_event(
            "runtime.state",
            {"agent": "opencode", "state": "ready", "reason": None, "recovery_hint": None, "version": "1.18.5"},
        )
        assert vm.opencodeState == "ready"
        assert vm.opencodeVersion == "1.18.5"
        # pi 保持失败态不被覆盖
        assert vm.piState == "failed"

    def test_status_refresh_applies_both(self, core_app, client):
        vm = RuntimeViewModel(client)
        vm.refresh()
        assert client.last().method == "runtime.status"
        client.ok(
            {
                "pi": {"state": "ready", "reason": None, "recovery_hint": None, "version": "1.4.0"},
                "opencode": {"state": "stopped", "reason": None, "recovery_hint": None, "version": None},
            }
        )
        assert vm.piState == "ready" and vm.piVersion == "1.4.0"
        assert vm.opencodeState == "stopped" and vm.opencodeVersion == ""

    def test_start_request_shape_and_error_passthrough(self, core_app, client):
        vm = RuntimeViewModel(client)
        vm.start("pi", "cfg-1")
        assert client.last().method == "runtime.start"
        assert client.last().params == {"agent": "pi", "config_id": "cfg-1"}
        message = "工作区尚未被信任，无法启动受管运行时。"
        client.err("WORKSPACE_NOT_TRUSTED", message)
        assert vm.errorMessage == message

    def test_probe_surfaces_compatible_and_unsupported_results(self, core_app, client):
        vm = RuntimeViewModel(client)
        vm.probe("opencode", "cfg-1")
        assert client.last().method == "runtime.probe"
        assert client.last().params == {"agent": "opencode", "config_id": "cfg-1"}
        client.ok({"agent": "opencode", "version": "1.18.5", "supported": True})
        assert vm.opencodeCompatibility == "supported"
        assert "1.18.5" in vm.opencodeProbeMessage

        vm.probe("opencode", "cfg-1")
        client.ok({"agent": "opencode", "version": "2.0.0", "supported": False})
        assert vm.opencodeCompatibility == "unsupported"
        assert "2.0.0" in vm.opencodeProbeMessage
        assert "安装稳定版 OpenCode 1.18.5 或更高的 1.x" in vm.opencodeProbeMessage


# --------------------------------------------------------------- TaskViewModel


TASK_STATUS = {
    "task_id": "task-1",
    "agent": "pi",
    "title": "修复登录超时",
    "state": "created",
    "attribution": "agent_only",
    "baseline": {"head": "abc123", "captured_at": "2026-07-26T08:00:00Z"},
    "created_at": "2026-07-26T08:00:00Z",
    "ended_at": None,
    "cancel_mode": None,
    "latest_evidence_version": 0,
}

SESSION_MESSAGES = [
    {"role": "user", "text": "排查并修复登录超时", "truncated": False},
    {"role": "agent", "text": "已完成首轮排查，敏感内容已隐藏。", "truncated": True},
]

ACTION_REQUESTS = [
    {
        "request_id": "permission-1",
        "kind": "permission",
        "prompt": "允许写入 src/auth.rs 吗？",
        "decision_sent": False,
    },
    {
        "request_id": "clarification-1",
        "kind": "clarification",
        "prompt": "应该使用本地配置还是测试配置？",
        "decision_sent": False,
    },
]


class TestTaskViewModel:
    def _make_vm(self, client):
        vm = TaskViewModel(client)
        vm.agent = "pi"
        vm.configId = "cfg-1"
        vm.title = "修复登录超时"
        vm.instructions = "排查并修复登录超时"
        vm.files = ["src/auth.rs"]
        return vm

    def test_create_sends_spec_with_nulls(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        req = client.last()
        assert req.method == "task.create"
        assert req.params == {
            "agent": "pi",
            "config_id": "cfg-1",
            "title": "修复登录超时",
            "instructions": "排查并修复登录超时",
            "files": ["src/auth.rs"],
            "base_diff": None,
            "notes": None,
            "handoff_id": None,
        }

    def test_create_ok_applies_status_and_events_drive_transitions(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        assert vm.taskId == "task-1"
        assert vm.state == "created"
        assert vm.attribution == "agent_only"
        assert vm.baselineHead == "abc123"

        for state in ("running", "awaiting_action", "running", "finishing", "review_ready"):
            client.emit_event(
                "task.state",
                {"state": state, "task": {**TASK_STATUS, "state": state}},
                task_id="task-1",
            )
            assert vm.state == state

    def test_task_state_for_other_task_ignored(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.emit_event(
            "task.state",
            {"state": "failed", "task": {**TASK_STATUS, "task_id": "task-other", "state": "failed"}},
            task_id="task-other",
        )
        assert vm.state == "created"

    def test_mark_manual_edit_sets_mixed(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        vm.markManualEdit("我手工改了 auth.rs")
        req = client.last()
        assert req.method == "task.mark_manual_edit"
        assert req.params == {"task_id": "task-1", "note": "我手工改了 auth.rs"}
        client.ok({"attribution": "mixed"})
        assert vm.attribution == "mixed"

    def test_cancel_and_cancelled_event_mode(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        vm.cancel()
        assert client.last().method == "task.cancel"
        assert client.last().params == {"task_id": "task-1"}
        client.emit_event("task.cancelled", {"mode": "forced"}, task_id="task-1")
        assert vm.cancelMode == "forced"
        assert client.last().method == "task.status"
        assert client.last().params == {"task_id": "task-1"}
        client.ok(
            {
                "task": {
                    **TASK_STATUS,
                    "state": "cancelled",
                    "cancel_mode": "forced",
                    "latest_evidence_version": 1,
                }
            }
        )
        assert vm.state == "cancelled"
        assert vm.latestEvidenceVersion == 1

    def test_workspace_change_clears_the_previous_task(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "attribution": "mixed"}})

        client.emit_event("workspace.changed", {"active": True, "workspace_id": "workspace-2"})

        assert vm.taskId == ""
        assert vm.state == ""
        assert vm.attribution == ""
        assert vm.latestEvidenceVersion == 0

    def test_snapshot_rebuilds_managed_session_and_waits_for_developer(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "waiting_developer"}})

        snapshot_request = client.last()
        assert snapshot_request.method == "task.snapshot"
        assert snapshot_request.params == {"after_seq": 0}
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "waiting_developer"},
                "last_seq": 40,
                "events": [],
                "session_messages": SESSION_MESSAGES,
            }
        )

        assert vm.state == "waiting_developer"
        assert vm.sessionMessages == SESSION_MESSAGES
        assert [request.method for request in client.requests] == ["task.create", "task.snapshot"]

    def test_session_message_events_are_ordered_deduped_and_isolated_from_trace(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.ok(
            {
                "task": TASK_STATUS,
                "last_seq": 10,
                "events": [],
                "session_messages": SESSION_MESSAGES,
            }
        )

        client.emit_event(
            "task.session_message",
            {"role": "agent", "text": "第二条回复", "truncated": False},
            seq=12,
            task_id="task-1",
        )
        client.emit_event(
            "task.session_message",
            {"role": "user", "text": "补充说明", "truncated": False},
            seq=11,
            task_id="task-1",
        )
        # 已由快照覆盖的旧事件、重复 seq、非会话角色和其他任务不得改变活动会话记录。
        client.emit_event(
            "task.session_message",
            {"role": "agent", "text": "旧回复", "truncated": False},
            seq=10,
            task_id="task-1",
        )
        client.emit_event(
            "task.session_message",
            {"role": "agent", "text": "重复 seq", "truncated": False},
            seq=11,
            task_id="task-1",
        )
        client.emit_event(
            "task.session_message",
            {"role": "tool", "text": "原始工具输出", "truncated": False},
            seq=13,
            task_id="task-1",
        )
        client.emit_event(
            "task.session_message",
            {"role": "agent", "text": "其他任务", "truncated": False},
            seq=14,
            task_id="task-other",
        )

        assert vm.sessionMessages == [
            *SESSION_MESSAGES,
            {"role": "user", "text": "补充说明", "truncated": False},
            {"role": "agent", "text": "第二条回复", "truncated": False},
        ]

    def test_snapshot_coalesces_an_already_queued_session_event(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.ok({"task": TASK_STATUS, "last_seq": 0, "events": [], "session_messages": []})

        reply = {"role": "agent", "text": "已脱敏回复", "truncated": False}
        client.emit_event("task.session_message", reply, seq=11, task_id="task-1")
        # task.snapshot 读取事件缓冲后再复制活动会话记录时，可能带上这个尚未覆盖的事件。
        vm.applySnapshot(
            {
                "task": TASK_STATUS,
                "last_seq": 10,
                "events": [],
                "session_messages": [reply],
            }
        )

        assert vm.sessionMessages == [reply]

    def test_task_state_event_preserves_session_until_a_new_snapshot_replaces_it(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.ok(
            {
                "task": TASK_STATUS,
                "last_seq": 2,
                "events": [],
                "session_messages": SESSION_MESSAGES,
            }
        )

        client.emit_event(
            "task.state",
            {"state": "waiting_developer", "task": {**TASK_STATUS, "state": "waiting_developer"}},
            task_id="task-1",
        )

        assert vm.state == "waiting_developer"
        assert vm.sessionMessages == SESSION_MESSAGES

    def test_terminal_task_state_clears_active_session_record(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.ok(
            {
                "task": TASK_STATUS,
                "last_seq": 2,
                "events": [],
                "session_messages": SESSION_MESSAGES,
            }
        )

        client.emit_event(
            "task.state",
            {"state": "cancelled", "task": {**TASK_STATUS, "state": "cancelled"}},
            task_id="task-1",
        )

        assert vm.state == "cancelled"
        assert vm.sessionMessages == []

    def test_late_snapshot_does_not_regress_a_newer_task_state_event(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.ok({"task": TASK_STATUS, "last_seq": 0, "events": [], "session_messages": []})

        client.emit_event(
            "task.state",
            {"state": "waiting_developer", "task": {**TASK_STATUS, "state": "waiting_developer"}},
            task_id="task-1",
        )
        vm.applySnapshot(
            {
                "task": TASK_STATUS,
                "last_seq": 1,
                "events": [],
                "session_messages": SESSION_MESSAGES,
            }
        )

        assert vm.state == "waiting_developer"
        assert vm.sessionMessages == SESSION_MESSAGES

    def test_snapshot_restores_only_displayable_action_requests(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 40,
                "events": [],
                "session_messages": SESSION_MESSAGES,
                "action_requests": [
                    *ACTION_REQUESTS,
                    {"request_id": "unknown-1", "kind": "always", "prompt": "bad"},
                    {"request_id": "blank-1", "kind": "permission", "prompt": ""},
                    {"kind": "permission", "prompt": "missing id"},
                ],
            }
        )

        assert vm.actionRequests == ACTION_REQUESTS

    def test_action_request_events_are_current_task_only_and_keep_submitted_decision_locked(
        self, core_app, client
    ):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        client.ok(
            {
                "task": TASK_STATUS,
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": [],
            }
        )

        client.emit_event(
            "task.action_request",
            ACTION_REQUESTS[0],
            task_id="task-other",
        )
        assert vm.actionRequests == []

        client.emit_event("task.action_request", ACTION_REQUESTS[0], task_id="task-1")
        assert vm.actionRequests == [ACTION_REQUESTS[0]]

        client.emit_event(
            "task.state",
            {"state": "awaiting_action", "task": {**TASK_STATUS, "state": "awaiting_action"}},
            task_id="task-1",
        )
        vm.allowOnce("permission-1")
        client.emit_event("task.action_request", ACTION_REQUESTS[0], task_id="task-1")
        assert vm.actionRequests == [{**ACTION_REQUESTS[0], "decision_sent": True}]

    def test_permission_allow_once_is_one_time_and_waits_for_real_agent_feedback(
        self, core_app, client
    ):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )
        client.emit_event(
            "task.state",
            {"state": "awaiting_action", "task": {**TASK_STATUS, "state": "awaiting_action"}},
            task_id="task-1",
        )

        vm.allowOnce("permission-1")
        request = client.last()
        assert request.method == "task.resolve_action"
        assert request.params == {
            "task_id": "task-1",
            "request_id": "permission-1",
            "decision": "allow_once",
            "answer": None,
        }
        assert vm.actionRequests[0]["decision_sent"] is True

        # accepted 只表示决议已送达，不是任务已恢复。
        client.ok({"accepted": True})
        assert vm.state == "awaiting_action"
        assert vm.actionRequests[0]["decision_sent"] is True

        vm.allowOnce("permission-1")
        assert [request.method for request in client.requests].count("task.resolve_action") == 1
        assert vm.errorCode == "ACTION_REQUEST_ALREADY_RESOLVED"

        client.emit_event(
            "task.state",
            {"state": "running", "task": {**TASK_STATUS, "state": "running"}},
            task_id="task-1",
        )
        assert vm.state == "running"
        assert vm.actionRequests == []

    def test_action_resolved_removes_only_the_matching_submitted_request(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.allowOnce("permission-1")
        client.ok({"accepted": True})
        assert vm.actionRequests == [
            {**ACTION_REQUESTS[0], "decision_sent": True},
            ACTION_REQUESTS[1],
        ]

        # 只有当前任务的、已提交过决议的精确请求可以被真实 Agent 回执移除。
        client.emit_event(
            "task.action_resolved", {"request_id": "permission-1"}, task_id="task-other"
        )
        client.emit_event(
            "task.action_resolved", {"request_id": "permission-other"}, task_id="task-1"
        )
        client.emit_event(
            "task.action_resolved", {"request_id": "clarification-1"}, task_id="task-1"
        )
        assert vm.actionRequests == [
            {**ACTION_REQUESTS[0], "decision_sent": True},
            ACTION_REQUESTS[1],
        ]
        assert vm.state == "awaiting_action"

        client.emit_event(
            "task.action_resolved", {"request_id": "permission-1"}, task_id="task-1"
        )
        assert vm.actionRequests == [ACTION_REQUESTS[1]]
        # 任务阶段只能由随后 task.state 中的真实 Agent 反馈推进。
        assert vm.state == "awaiting_action"

    def test_reject_is_available_for_permission_and_clarification_only(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.rejectAction("permission-1")
        permission_request = client.last()
        assert permission_request.params == {
            "task_id": "task-1",
            "request_id": "permission-1",
            "decision": "reject",
            "answer": None,
        }

        vm.rejectAction("clarification-1")
        clarification_request = client.last()
        assert clarification_request.params == {
            "task_id": "task-1",
            "request_id": "clarification-1",
            "decision": "reject",
            "answer": None,
        }
        assert {request.params["decision"] for request in client.requests if request.method == "task.resolve_action"} == {"reject"}

    def test_clarification_answer_is_scoped_to_the_matching_request(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.answerClarification("clarification-1", "")
        assert vm.errorCode == "INVALID_PARAMS"
        assert [request.method for request in client.requests].count("task.resolve_action") == 0

        vm.answerClarification("permission-1", "请继续")
        assert vm.errorCode == "INVALID_PARAMS"
        assert [request.method for request in client.requests].count("task.resolve_action") == 0

        vm.answerClarification("clarification-1", "使用本地配置")
        request = client.last()
        assert request.params == {
            "task_id": "task-1",
            "request_id": "clarification-1",
            "decision": "answer",
            "answer": "使用本地配置",
        }
        assert vm.actionRequests[1]["decision_sent"] is True

    def test_action_decision_error_reenables_only_the_matching_request(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.allowOnce("permission-1")
        vm.answerClarification("clarification-1", "使用本地配置")
        client.err("ACTION_REQUEST_NOT_FOUND", "当前任务没有匹配的操作请求", index=-2)

        assert vm.errorCode == "ACTION_REQUEST_NOT_FOUND"
        assert vm.actionRequests == [
            ACTION_REQUESTS[0],
            {**ACTION_REQUESTS[1], "decision_sent": True},
        ]

    def test_uncertain_action_delivery_keeps_the_matching_card_locked(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.allowOnce("permission-1")
        client.err("ACTION_REQUEST_NOT_PENDING", "无法确认本次操作请求是否已送达")

        assert vm.errorCode == "ACTION_REQUEST_NOT_PENDING"
        assert vm.actionRequests == [
            {**ACTION_REQUESTS[0], "decision_sent": True},
            ACTION_REQUESTS[1],
        ]
        vm.allowOnce("permission-1")
        assert vm.errorCode == "ACTION_REQUEST_ALREADY_RESOLVED"
        assert [request.method for request in client.requests].count("task.resolve_action") == 1

    def test_delayed_snapshot_cannot_reenable_a_submitted_matching_request(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.allowOnce("permission-1")
        vm.applySnapshot(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 3,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        assert vm.actionRequests[0]["decision_sent"] is True
        assert vm.actionRequests[1]["decision_sent"] is False

    def test_delayed_awaiting_action_snapshot_cannot_restore_cards_after_cancellation(
        self, core_app, client
    ):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )
        assert vm.actionRequests == ACTION_REQUESTS

        client.emit_event(
            "task.state",
            {"state": "cancelled", "task": {**TASK_STATUS, "state": "cancelled"}},
            task_id="task-1",
        )
        assert vm.state == "cancelled"
        assert vm.actionRequests == []

        vm.applySnapshot(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 3,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        assert vm.state == "cancelled"
        assert vm.actionRequests == []

    def test_delayed_awaiting_action_snapshot_cannot_restore_cards_during_cancel_barrier(
        self, core_app, client
    ):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": [],
            }
        )

        vm.cancel()
        client.ok({"accepted": True})
        assert vm.actionResolutionBlocked is True
        assert vm.actionRequests == []

        vm.applySnapshot(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 3,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        assert vm.actionResolutionBlocked is True
        assert vm.actionRequests == []

    def test_delayed_snapshot_cannot_restore_a_card_removed_by_real_agent_feedback(
        self, core_app, client
    ):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.allowOnce("permission-1")
        client.ok({"accepted": True})
        client.emit_event(
            "task.action_resolved", {"request_id": "permission-1"}, task_id="task-1"
        )
        assert vm.state == "awaiting_action"
        assert vm.actionRequests == [ACTION_REQUESTS[1]]

        # 另一张卡仍待处理，任务阶段尚未变化；因此不能只用状态过滤旧快照。
        vm.applySnapshot(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 3,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        assert vm.actionRequests == [ACTION_REQUESTS[1]]

        client.emit_event("task.action_request", ACTION_REQUESTS[0], task_id="task-1")
        assert vm.actionRequests == [ACTION_REQUESTS[1]]

    @pytest.mark.parametrize("cancel_outcome", ("not_accepted", "error"))
    def test_cancellation_failure_keeps_action_cards_blocked(self, core_app, client, cancel_outcome):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.cancel()
        assert client.last().method == "task.cancel"
        assert vm.actionResolutionBlocked is True

        if cancel_outcome == "not_accepted":
            client.ok({"accepted": False})
        else:
            client.err("TASK_RUNNING", "取消请求被拒绝")
        assert vm.actionResolutionBlocked is True
        assert vm.errorCode == "TASK_RUNNING"

        vm.allowOnce("permission-1")
        assert vm.errorCode == "ACTION_REQUEST_NOT_PENDING"
        assert [request.method for request in client.requests].count("task.resolve_action") == 0

    def test_unknown_action_request_never_sends_a_decision(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": {**TASK_STATUS, "state": "awaiting_action"}})
        client.ok(
            {
                "task": {**TASK_STATUS, "state": "awaiting_action"},
                "last_seq": 2,
                "events": [],
                "session_messages": [],
                "action_requests": ACTION_REQUESTS,
            }
        )

        vm.rejectAction("permission-other")
        assert vm.errorCode == "ACTION_REQUEST_NOT_FOUND"
        assert [request.method for request in client.requests].count("task.resolve_action") == 0

    def test_mark_verification_not_run_params(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        client.ok({"task": TASK_STATUS})
        vm.markVerificationNotRun("本次未执行验证")
        req = client.last()
        assert req.method == "task.mark_verification"
        assert req.params == {"task_id": "task-1", "status": "not_run", "note": "本次未执行验证"}

    def test_error_message_passthrough(self, core_app, client):
        vm = self._make_vm(client)
        vm.create()
        message = "当前工作区已有正在运行的任务，请先等待其结束或取消。"
        client.err("TASK_ALREADY_RUNNING", message)
        assert vm.errorCode == "TASK_ALREADY_RUNNING"
        assert vm.errorMessage == message
        assert vm.taskId == ""


# -------------------------------------------------------------- TraceViewModel


class TestTraceViewModel:
    def test_snapshot_event_gap_rebuilds_from_oldest_available(self, core_app, client):
        vm = TraceViewModel(client)

        vm.refresh()
        req = client.last()
        assert req.method == "task.snapshot"
        assert req.params == {"after_seq": 0}

        client.err(
            "EVENT_GAP",
            "事件缓冲不足以覆盖请求的 after_seq，请整体重建视图",
            {"oldest_available_seq": 42},
        )
        retry = client.last()
        assert retry.method == "task.snapshot"
        assert retry.params == {"after_seq": 41}

        client.ok(
            {
                "task": None,
                "last_seq": 43,
                "events": [
                    {"seq": 42, "ts": "t", "task_id": None, "event": "task.phase",
                     "payload": {"phase": "editing", "detail": "正在编辑"}},
                    {"seq": 43, "ts": "t", "task_id": None, "event": "trace.item",
                     "payload": {"kind": "agent_note", "text": "已恢复"}},
                ],
            }
        )
        assert vm.count == 2
        assert vm.lastSeq == 43
        assert [vm.data(vm.index(i, 0), vm.TextRole) for i in range(2)] == ["正在编辑", "已恢复"]

    def test_snapshot_error_is_visible(self, core_app, client):
        vm = TraceViewModel(client)

        vm.refresh()
        client.err("TASK_NOT_FOUND", "当前没有可恢复的任务轨迹")

        assert vm.errorCode == "TASK_NOT_FOUND"
        assert vm.errorMessage == "当前没有可恢复的任务轨迹"

    def test_orders_by_seq_and_dedupes(self, core_app, client):
        vm = TraceViewModel(client)
        client.emit_event("trace.item", {"kind": "agent_note", "text": "三"}, seq=3)
        client.emit_event("trace.item", {"kind": "agent_note", "text": "一"}, seq=1)
        client.emit_event("trace.item", {"kind": "agent_note", "text": "二"}, seq=2)
        client.emit_event("trace.item", {"kind": "agent_note", "text": "重复"}, seq=2)
        assert vm.count == 3
        texts = [vm.data(vm.index(i, 0), vm.TextRole) for i in range(vm.rowCount())]
        assert texts == ["一", "二", "三"]
        seqs = [vm.data(vm.index(i, 0), vm.SeqRole) for i in range(vm.rowCount())]
        assert seqs == [1, 2, 3]
        assert vm.lastSeq == 3

    def test_normalizes_phase_action_verification(self, core_app, client):
        vm = TraceViewModel(client)
        client.emit_event("task.phase", {"phase": "planning", "detail": "正在规划"}, seq=1)
        client.emit_event(
            "task.action_request",
            {
                "request_id": "ar-1",
                "kind": "permission",
                "prompt": "允许写入文件？",
                "decision_sent": False,
                "channel": "native",
                "remote_session_id": "ses-private",
                "port": 4999,
            },
            seq=2,
        )
        client.emit_event(
            "task.verification", {"status": "passed", "detail": "测试通过", "source": "agent"}, seq=3
        )
        assert vm.count == 3
        kinds = [vm.data(vm.index(i, 0), vm.KindRole) for i in range(3)]
        assert kinds == ["phase", "action_request", "verification"]
        assert vm.data(vm.index(0, 0), vm.TextRole) == "正在规划"
        assert vm.data(vm.index(1, 0), vm.TextRole) == "允许写入文件？"
        assert vm.data(vm.index(2, 0), vm.TextRole) == "测试通过"
        detail = vm.data(vm.index(1, 0), vm.DetailRole)
        assert detail == {
            "request_id": "ar-1",
            "kind": "permission",
            "prompt": "允许写入文件？",
            "decision_sent": False,
        }

    def test_ignores_unrelated_events(self, core_app, client):
        vm = TraceViewModel(client)
        client.emit_event("task.state", {"state": "running", "task": {}}, seq=1)
        client.emit_event("runtime.state", {"agent": "pi", "state": "ready"}, seq=2)
        assert vm.count == 0

    def test_snapshot_restore_replaces_content(self, core_app, client):
        vm = TraceViewModel(client)
        client.emit_event("trace.item", {"kind": "agent_note", "text": "旧内容"}, seq=99)
        snapshot = {
            "task": None,
            "last_seq": 42,
            "events": [
                {"seq": 41, "ts": "t", "task_id": "task-1", "event": "trace.item",
                 "payload": {"kind": "phase", "text": "editing"}},
                {"seq": 40, "ts": "t", "task_id": "task-1", "event": "task.phase",
                 "payload": {"phase": "planning", "detail": "规划中"}},
                {"seq": 42, "ts": "t", "task_id": "task-1", "event": "task.state",
                 "payload": {"state": "running", "task": {}}},  # 非消费事件被过滤
                {"seq": 41, "ts": "t", "task_id": "task-1", "event": "trace.item",
                 "payload": {"kind": "phase", "text": "重复 seq"}},
            ],
        }
        vm.applySnapshot(snapshot)
        assert vm.count == 2
        assert [vm.data(vm.index(i, 0), vm.SeqRole) for i in range(2)] == [40, 41]
        assert vm.data(vm.index(0, 0), vm.TextRole) == "规划中"
        assert vm.lastSeq == 42
        # 恢复后继续按 seq 去重
        client.emit_event("trace.item", {"kind": "agent_note", "text": "again"}, seq=41)
        assert vm.count == 2


# ------------------------------------------------------------- ReviewViewModel


REVIEW_BUNDLE = {
    "task_id": "task-1",
    "evidence_version": 2,
    "is_latest": True,
    "outcome": "finished",
    "attribution": "mixed",
    "attribution_reasons": ["用户于 08:12 标记人工编辑"],
    "manual_edit_paths": ["src/auth.rs"],
    "summary": "修复了登录超时",
    "files": [
        {
            "path": "src/auth.rs",
            "change": "modified",
            "diff": "--- a\n+++ b",
            "truncated": False,
            "end_hash": "sha256:abc",
        },
        {"path": "src/big.rs", "change": "added", "diff": "…", "truncated": True},
    ],
    "verification": {"status": "passed", "detail": "cargo test 通过", "source": "agent"},
    "baseline_dirty_files": ["docs/x.md"],
}


class TestReviewViewModel:
    def test_load_populates_bundle_and_files_model(self, core_app, client):
        vm = ReviewViewModel(client)
        vm.load("task-1")
        assert client.last().method == "review.get"
        assert client.last().params == {"task_id": "task-1"}
        client.ok(REVIEW_BUNDLE)
        assert vm.taskId == "task-1"
        assert vm.evidenceVersion == 2
        assert vm.isLatest is True
        assert vm.outcome == "finished"
        assert vm.attribution == "mixed"
        assert vm.attributionReasons == ["用户于 08:12 标记人工编辑"]
        assert vm.manualEditPaths == ["src/auth.rs"]
        assert vm.baselineDirtyFiles == ["docs/x.md"]
        assert vm.verificationStatus == "passed"
        assert vm.verificationSource == "agent"
        model = vm.files
        assert model.rowCount() == 2
        idx = model.index(0, 0)
        assert model.data(idx, model.PathRole) == "src/auth.rs"
        assert model.data(idx, model.ChangeRole) == "modified"
        assert model.data(idx, model.DiffRole) == "--- a\n+++ b"
        assert model.data(idx, model.TruncatedRole) is False
        assert model.data(idx, model.EndHashRole) == "sha256:abc"
        assert model.roleNames()[model.EndHashRole] == b"endHash"
        assert model.data(model.index(1, 0), model.TruncatedRole) is True

    def test_readonly_no_write_api(self, core_app, client):
        vm = ReviewViewModel(client)
        forbidden = (
            "save", "saveFile", "save_file", "write", "writeFile", "write_file",
            "edit", "editFile", "applyPatch", "apply_patch", "setFileContent",
            "set_file_content", "revert", "rollback",
        )
        for name in forbidden:
            assert not hasattr(vm, name), f"审查视图模型不得提供写能力：{name}"
        # 文件模型：排除 Qt 基类自带的 submit/revert 槽，其余写命名一律不得存在
        model_forbidden = tuple(n for n in forbidden if n not in ("revert",))
        for name in model_forbidden:
            assert not hasattr(vm.files, name), f"审查文件模型不得提供写能力：{name}"
        # 模型本身只读：setData 拒绝写入，条目不带可编辑标志
        vm.load("task-1")
        client.ok(REVIEW_BUNDLE)
        idx = vm.files.index(0, 0)
        assert vm.files.setData(idx, "篡改", vm.files.DiffRole) is False
        from PySide6.QtCore import Qt

        assert not (vm.files.flags(idx) & Qt.ItemFlag.ItemIsEditable)

    def test_accept_reject_use_loaded_evidence_version(self, core_app, client):
        vm = ReviewViewModel(client)
        vm.load("task-1")
        client.ok(REVIEW_BUNDLE)

        vm.accept()
        assert client.last().method == "delivery.accept"
        assert client.last().params == {"task_id": "task-1", "evidence_version": 2}
        client.ok(
            {"decision": {"kind": "accepted", "task_id": "task-1", "evidence_version": 2,
                          "decided_at": "2026-07-26T09:00:00Z", "reason": None}}
        )
        assert vm.decisionKind == "accepted"
        assert vm.decidedAt == "2026-07-26T09:00:00Z"

        vm.reject("验证不充分")
        assert client.last().method == "delivery.reject"
        assert client.last().params == {
            "task_id": "task-1",
            "evidence_version": 2,
            "reason": "验证不充分",
        }

    def test_stale_evidence_error_passthrough(self, core_app, client):
        vm = ReviewViewModel(client)
        vm.load("task-1")
        client.ok(REVIEW_BUNDLE)
        vm.accept()
        message = "只有最新的交付证据版本可以被接受或拒绝。"
        client.err("EVIDENCE_NOT_LATEST", message)
        assert vm.errorCode == "EVIDENCE_NOT_LATEST"
        assert vm.errorMessage == message


# ------------------------------------------------------------ HandoffViewModel


HANDOFF_PACKAGE = {
    "handoff_id": None,
    "task_id": "task-1",
    "source_agent": "pi",
    "target_agent": None,
    "goal": "修复登录超时",
    "summary": "已修复并通过测试",
    "selected_changes": [{"path": "src/auth.rs", "diff": "--- a\n+++ b"}],
    "verification": {"status": "passed", "detail": "cargo test 通过"},
    "created_at": None,
}


class TestHandoffViewModel:
    def test_preview_and_create(self, core_app, client):
        vm = HandoffViewModel(client)
        vm.preview("task-1", ["src/auth.rs"])
        req = client.last()
        assert req.method == "handoff.preview"
        assert req.params == {"task_id": "task-1", "selected_files": ["src/auth.rs"]}
        client.ok({"package": HANDOFF_PACKAGE})
        assert vm.goal == "修复登录超时"
        assert vm.summary == "已修复并通过测试"
        assert vm.selectedChanges == [{"path": "src/auth.rs", "diff": "--- a\n+++ b"}]
        assert vm.verificationStatus == "passed"
        assert vm.handoffId == ""

        vm.create("opencode")
        req = client.last()
        assert req.method == "handoff.create"
        assert req.params == {
            "task_id": "task-1",
            "target_agent": "opencode",
            "selected_files": ["src/auth.rs"],
        }
        client.ok(
            {
                "handoff_id": "ho-1",
                "package": {**HANDOFF_PACKAGE, "handoff_id": "ho-1", "target_agent": "opencode",
                            "created_at": "2026-07-26T09:30:00Z"},
            }
        )
        assert vm.handoffId == "ho-1"
        assert vm.targetAgent == "opencode"
        assert vm.createdAt == "2026-07-26T09:30:00Z"

    def test_preview_default_all_files(self, core_app, client):
        vm = HandoffViewModel(client)
        vm.preview("task-1")
        assert client.last().params == {"task_id": "task-1", "selected_files": None}

    def test_still_running_error_passthrough(self, core_app, client):
        vm = HandoffViewModel(client)
        vm.preview("task-1")
        message = "任务仍在运行，交接只能在可审查交付结束后进行。"
        client.err("TASK_STILL_RUNNING", message)
        assert vm.errorCode == "TASK_STILL_RUNNING"
        assert vm.errorMessage == message


# ------------------------------------------------------------ HistoryViewModel


class TestHistoryViewModel:
    def test_list_populates_models(self, core_app, client):
        vm = HistoryViewModel(client)
        vm.list(10)
        assert client.last().method == "history.list"
        assert client.last().params == {"limit": 10}
        client.ok(
            {
                "tasks": [
                    {"task_id": "task-1", "agent": "pi", "title": "修复登录超时",
                     "state": "accepted", "attribution": "agent_only",
                     "created_at": "2026-07-26T08:00:00Z", "ended_at": "2026-07-26T09:00:00Z",
                     "cancel_mode": None, "latest_evidence_version": 2},
                ],
                "decisions": [
                    {"kind": "accepted", "task_id": "task-1", "evidence_version": 2,
                     "decided_at": "2026-07-26T09:00:00Z", "reason": None},
                ],
            }
        )
        model = vm.tasks
        assert model.rowCount() == 1
        idx = model.index(0, 0)
        assert model.data(idx, model.TaskIdRole) == "task-1"
        assert model.data(idx, model.StateRole) == "accepted"
        assert model.data(idx, model.LatestEvidenceVersionRole) == 2
        assert vm.decisions[0]["kind"] == "accepted"

    def test_default_limit(self, core_app, client):
        vm = HistoryViewModel(client)
        vm.list()
        assert client.last().params == {"limit": 50}
