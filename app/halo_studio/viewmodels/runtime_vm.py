"""RuntimeViewModel：Pi 与 OpenCode 两组完全独立的运行时健康状态。

契约红线：两个受管应用的状态绝不合并为“全局在线”；本类只按 agent 维度更新。
"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal, Slot

from .base import BaseViewModel

_AGENTS = ("pi", "opencode")
_INITIAL = {"state": "not_probed", "reason": "", "recovery_hint": "", "version": ""}
_PROBE_INITIAL = {"status": "not_checked", "message": ""}


class RuntimeViewModel(BaseViewModel):
    piChanged = Signal()
    opencodeChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._info: dict[str, dict] = {agent: dict(_INITIAL) for agent in _AGENTS}
        self._probe: dict[str, dict] = {agent: dict(_PROBE_INITIAL) for agent in _AGENTS}
        client.subscribe("runtime.state", self._on_state_event)

    # ---- 命令 ----

    @Slot(str, str)
    def probe(self, agent: str, config_id: str) -> None:
        self._clear_error()

        def on_ok(result: dict, target: str = agent) -> None:
            version = result.get("version")
            supported = result.get("supported") is True
            if version:
                self._apply(target, {"version": version})
            if supported:
                message = f"兼容性检查通过：{version or '已检测版本'}"
                status = "supported"
            else:
                message = (
                    f"兼容性检查不通过：{version or '未返回有效版本'}。"
                    "请安装稳定版 OpenCode 1.18.5 或更高的 1.x 版本后重新检查"
                )
                status = "unsupported"
            self._apply_probe(target, status, message)

        def on_error(error: dict, target: str = agent) -> None:
            message = str(error.get("message") or "兼容性检查失败")
            if target == "opencode":
                message += "。请确认 OpenCode 可执行文件有效后重新检查"
            self._apply_probe(target, "failed", message)
            self._set_error(error)

        self._client.request(
            "runtime.probe", {"agent": agent, "config_id": config_id}, on_ok, on_error
        )

    @Slot(str, str)
    def start(self, agent: str, config_id: str) -> None:
        self._clear_error()
        self._client.request(
            "runtime.start",
            {"agent": agent, "config_id": config_id},
            lambda result, target=agent: self._on_state_result(target, result),
            self._set_error,
        )

    @Slot(str)
    def stop(self, agent: str) -> None:
        self._clear_error()
        self._client.request(
            "runtime.stop",
            {"agent": agent},
            lambda result, target=agent: self._on_state_result(target, result),
            self._set_error,
        )

    @Slot()
    def refresh(self) -> None:
        self._client.request("runtime.status", {}, self._on_status_ok, self._set_error)

    # ---- 回调 ----

    def _on_state_result(self, agent: str, result: dict) -> None:
        state = result.get("state")
        if isinstance(state, dict):
            self._apply(agent, state)
        elif isinstance(state, str):
            self._apply(agent, {"state": state})

    def _on_status_ok(self, result: dict) -> None:
        for agent in _AGENTS:
            info = result.get(agent)
            if isinstance(info, dict):
                self._apply(agent, info)

    def _on_state_event(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        agent = payload.get("agent")
        if agent in _AGENTS:
            self._apply(agent, payload)

    def _apply(self, agent: str, info: dict) -> None:
        slot = self._info[agent]
        for key in ("state", "reason", "recovery_hint", "version"):
            if key in info:
                value = info.get(key)
                slot[key] = "" if value is None else str(value)
        self._notify_agent_changed(agent)

    def _apply_probe(self, agent: str, status: str, message: str) -> None:
        self._probe[agent] = {"status": status, "message": message}
        self._notify_agent_changed(agent)

    def _notify_agent_changed(self, agent: str) -> None:
        if agent == "pi":
            self.piChanged.emit()
        else:
            self.opencodeChanged.emit()

    # ---- 属性（pi 与 opencode 各自独立，绝不派生全局状态）----

    def _get_pi_state(self) -> str:
        return self._info["pi"]["state"]

    def _get_pi_reason(self) -> str:
        return self._info["pi"]["reason"]

    def _get_pi_recovery_hint(self) -> str:
        return self._info["pi"]["recovery_hint"]

    def _get_pi_version(self) -> str:
        return self._info["pi"]["version"]

    def _get_pi_compatibility(self) -> str:
        return self._probe["pi"]["status"]

    def _get_pi_probe_message(self) -> str:
        return self._probe["pi"]["message"]

    def _get_oc_state(self) -> str:
        return self._info["opencode"]["state"]

    def _get_oc_reason(self) -> str:
        return self._info["opencode"]["reason"]

    def _get_oc_recovery_hint(self) -> str:
        return self._info["opencode"]["recovery_hint"]

    def _get_oc_version(self) -> str:
        return self._info["opencode"]["version"]

    def _get_oc_compatibility(self) -> str:
        return self._probe["opencode"]["status"]

    def _get_oc_probe_message(self) -> str:
        return self._probe["opencode"]["message"]

    piState = Property(str, _get_pi_state, notify=piChanged)
    piReason = Property(str, _get_pi_reason, notify=piChanged)
    piRecoveryHint = Property(str, _get_pi_recovery_hint, notify=piChanged)
    piVersion = Property(str, _get_pi_version, notify=piChanged)
    piCompatibility = Property(str, _get_pi_compatibility, notify=piChanged)
    piProbeMessage = Property(str, _get_pi_probe_message, notify=piChanged)
    opencodeState = Property(str, _get_oc_state, notify=opencodeChanged)
    opencodeReason = Property(str, _get_oc_reason, notify=opencodeChanged)
    opencodeRecoveryHint = Property(str, _get_oc_recovery_hint, notify=opencodeChanged)
    opencodeVersion = Property(str, _get_oc_version, notify=opencodeChanged)
    opencodeCompatibility = Property(str, _get_oc_compatibility, notify=opencodeChanged)
    opencodeProbeMessage = Property(str, _get_oc_probe_message, notify=opencodeChanged)
