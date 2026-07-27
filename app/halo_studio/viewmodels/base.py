"""视图模型基类与 client 鸭子类型约定。

本层不依赖 ipc 包的任何具体类；构造时注入的 client 只要求两个能力（鸭子类型，
便于测试与并行开发，集成期由 halo_studio.ipc.client 满足同形接口）：

1. ``request(method, params, on_ok, on_err)``
   - ``on_ok(result: dict)``：IPC 响应 ``ok=true`` 时以 ``result`` 调用；
   - ``on_err(error: dict)``：``ok=false`` 时以错误体调用，形如
     ``{"code": "WORKSPACE_NOT_GIT", "message": "<中文用户可读文案>", "details": {...}}``。

2. ``subscribe(event, handler)``
   - ``handler(envelope: dict)``：收到完整事件封包
     ``{"seq": int, "ts": str, "task_id": str|None, "event": str, "payload": dict}``；
   - 连接生命周期由 client 以合成事件名透出（不占用契约事件命名空间）：
     ``"client.disconnected"``，payload = ``{"reason": "<中文原因>"}``。

回调必须已由 client 转到 Qt 主线程（见 module-contracts.md 第 8 节）。
"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, Signal, Slot


class BaseViewModel(QObject):
    """所有视图模型的公共基类：持有 client，并把 IPC 错误文案原样透传给界面。"""

    errorChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._client = client
        self._error_code = ""
        self._error_message = ""

    # ---- 错误透传：code 稳定字符串，message 为 Sidecar 返回的中文文案，直接可显示 ----

    def _set_error(self, error: dict) -> None:
        self._error_code = str(error.get("code") or "")
        self._error_message = str(error.get("message") or "")
        self.errorChanged.emit()

    def _clear_error(self) -> None:
        if self._error_code or self._error_message:
            self._error_code = ""
            self._error_message = ""
            self.errorChanged.emit()

    @Slot()
    def clearError(self) -> None:
        self._clear_error()

    def _get_error_code(self) -> str:
        return self._error_code

    def _get_error_message(self) -> str:
        return self._error_message

    errorCode = Property(str, _get_error_code, notify=errorChanged)
    errorMessage = Property(str, _get_error_message, notify=errorChanged)
