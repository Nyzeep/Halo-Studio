"""AppViewModel：Sidecar 连接状态、协议版本与不可用原因（issue 01 要求 UI 常显三者）。"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal

from .base import BaseViewModel


class AppViewModel(BaseViewModel):
    sidecarConnectedChanged = Signal()
    protocolVersionChanged = Signal()
    unavailableReasonChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._connected = False
        self._protocol_version = 0  # 0 = 尚未协商
        self._unavailable_reason = "Sidecar 尚未连接"
        client.subscribe("sidecar.state", self._on_sidecar_state)
        client.subscribe("client.disconnected", self._on_disconnected)

    # ---- 事件 ----

    def _on_sidecar_state(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        if payload.get("state") != "ready":
            return
        version = payload.get("protocol_version")
        if isinstance(version, int) and version != self._protocol_version:
            self._protocol_version = version
            self.protocolVersionChanged.emit()
        if not self._connected:
            self._connected = True
            self.sidecarConnectedChanged.emit()
        if self._unavailable_reason:
            self._unavailable_reason = ""
            self.unavailableReasonChanged.emit()

    def _on_disconnected(self, envelope: dict) -> None:
        payload = envelope.get("payload") or {}
        reason = str(payload.get("reason") or "Sidecar 连接已断开")
        if self._connected:
            self._connected = False
            self.sidecarConnectedChanged.emit()
        if reason != self._unavailable_reason:
            self._unavailable_reason = reason
            self.unavailableReasonChanged.emit()

    # ---- 属性 ----

    def _get_connected(self) -> bool:
        return self._connected

    def _get_protocol_version(self) -> int:
        return self._protocol_version

    def _get_unavailable_reason(self) -> str:
        return self._unavailable_reason

    sidecarConnected = Property(bool, _get_connected, notify=sidecarConnectedChanged)
    protocolVersion = Property(int, _get_protocol_version, notify=protocolVersionChanged)
    unavailableReason = Property(str, _get_unavailable_reason, notify=unavailableReasonChanged)
