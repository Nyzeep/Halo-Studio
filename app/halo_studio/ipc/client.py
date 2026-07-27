"""Sidecar IPC 的 Qt 包装层。

SidecarClient(QObject) 把 connection 层在后台线程触发的回调经内部 Signal
（QueuedConnection）转到本对象所属线程（通常是主线程）后再发射公开信号，
保证 connected/disconnected/eventReceived 与请求回调都在主线程执行。
"""

from __future__ import annotations

import os
import threading
from typing import Any, Callable, Iterable, Mapping, Sequence

from PySide6.QtCore import QObject, Qt, Signal, Slot

from .connection import PROTOCOL_VERSION, SidecarConnection, SidecarError


class SidecarClient(QObject):
    connected = Signal(dict)       # hello 结果（含 protocol_version/capabilities）
    disconnected = Signal(str)     # 用户可读中文原因（协商失败或异常断连）
    eventReceived = Signal(dict)   # 完整事件封包（seq/ts/task_id/event/payload）

    _marshal = Signal(object)      # 内部：把任意可调用对象转到本对象线程执行

    def __init__(
        self,
        exe_path: str | os.PathLike[str] | None = None,
        args: Sequence[str] = (),
        cwd: str | os.PathLike[str] | None = None,
        env: Mapping[str, str] | None = None,
        parent: QObject | None = None,
    ):
        super().__init__(parent)
        self._connection = SidecarConnection(exe_path=exe_path, args=args, cwd=cwd, env=env)
        self._started = False
        self._disconnected_emitted = False  # 只在本对象线程读写
        self._marshal.connect(self._run_marshalled, Qt.ConnectionType.QueuedConnection)
        self._connection.add_event_callback(
            lambda event: self._post(lambda: self.eventReceived.emit(event)))
        self._connection.add_disconnect_callback(
            lambda reason: self._post(lambda: self._emit_disconnected(reason)))

    # ---- 公开 API -------------------------------------------------------

    def start(self, app_protocol_versions: Iterable[int] = (PROTOCOL_VERSION,),
              app_version: str = "0.1.0", timeout: float = 10.0) -> None:
        """后台线程完成 spawn 与 hello 协商；结果经 connected/disconnected 信号返回。"""
        if self._started:
            raise SidecarError("SidecarClient 已启动，不可重复 start")
        self._started = True
        versions = list(app_protocol_versions)

        def work() -> None:
            try:
                self._connection.start()
                result = self._connection.hello(versions, app_version=app_version, timeout=timeout)
            except SidecarError as exc:
                reason = str(exc)
                try:
                    self._connection.close()
                except Exception:
                    pass
                self._post(lambda: self._emit_disconnected(reason))
                return
            self._post(lambda: self.connected.emit(result))

        threading.Thread(target=work, name="halo-sidecar-connect", daemon=True).start()

    def request(self, method: str, params: Mapping[str, Any] | None = None, *,
                on_result: Callable[[dict], None] | None = None,
                on_error: Callable[[Exception], None] | None = None):
        """回调式请求：on_result/on_error 在本对象线程执行；返回底层 Future。"""
        fut = self._connection.request_async(method, params)

        def done(f) -> None:
            try:
                result = f.result()
            except Exception as exc:  # noqa: BLE001 — 统一交给 on_error
                # except 块退出时会解绑 exc，必须先复制到普通局部变量再入闭包
                error = exc
                if on_error is not None:
                    self._post(lambda: on_error(error))
                return
            if on_result is not None:
                self._post(lambda: on_result(result))

        fut.add_done_callback(done)
        return fut

    def close(self) -> None:
        self._connection.close()

    @property
    def protocol_version(self) -> int | None:
        return self._connection.protocol_version

    @property
    def connection(self) -> SidecarConnection:
        return self._connection

    # ---- 内部实现 -------------------------------------------------------

    def _post(self, fn: Callable[[], None]) -> None:
        self._marshal.emit(fn)

    @Slot(object)
    def _run_marshalled(self, fn: Callable[[], None]) -> None:
        fn()

    def _emit_disconnected(self, reason: str) -> None:
        # hello 失败与底层断连回调可能先后到达：只发射一次
        if self._disconnected_emitted:
            return
        self._disconnected_emitted = True
        self.disconnected.emit(reason)
