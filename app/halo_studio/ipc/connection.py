"""Sidecar IPC 连接层（纯 Python，无 Qt）。

实现 docs/ipc-protocol.md v1 契约的 UI 侧封包读写：
- spawn 子进程（exe 路径参数化，默认 HALO_SIDECAR_EXE 或项目根下 sidecar/target/debug/halo-sidecar.exe）；
- 读线程逐行 JSONL 解析：单行超 1 MiB、坏 JSON、未知封包一律视为协议错误并断连；
- request/response 按 id 配对（concurrent.futures.Future）；
- 事件回调与断连回调（在读线程触发，回调方自行负责线程边界，Qt 侧见 client.py）；
- hello 协议版本协商；close() 温和终止子进程（关 stdin → 等待 → terminate → kill）。
"""

from __future__ import annotations

import json
import os
import subprocess
import threading
import uuid
from concurrent.futures import Future
from pathlib import Path
from typing import Any, Callable, Iterable, Mapping, Sequence

PROTOCOL_VERSION = 1
MAX_LINE_BYTES = 1024 * 1024

_ERROR_CODES = frozenset({
    "HELLO_REQUIRED", "PROTOCOL_VERSION_UNSUPPORTED", "METHOD_NOT_FOUND", "INVALID_PARAMS", "INTERNAL",
    "WORKSPACE_PATH_INVALID", "WORKSPACE_NOT_READABLE", "WORKSPACE_NOT_GIT", "WORKSPACE_NOT_TRUSTED",
    "WORKSPACE_NOT_ACTIVE", "WORKSPACE_IDENTITY_CHANGED",
    "CREDENTIAL_STORE_UNAVAILABLE", "CREDENTIAL_NOT_FOUND", "ENV_NOT_WHITELISTED", "CONFIG_NOT_FOUND",
    "CONFIG_CONFLICT", "RUNTIME_NOT_READY", "RUNTIME_PROBE_FAILED", "RUNTIME_VERSION_MISMATCH",
    "RUNTIME_CAPABILITY_UNAVAILABLE",
    "RUNTIME_ALREADY_RUNNING", "TASK_ALREADY_RUNNING", "TASK_RUNNING", "TASK_NOT_FOUND",
    "TASK_STILL_RUNNING", "TASK_NOT_REVIEWABLE", "EVIDENCE_NOT_FOUND", "EVIDENCE_NOT_LATEST", "EVENT_GAP",
    "HANDOFF_NOT_FOUND", "LINE_TOO_LONG", "PARSE_ERROR",
})
_EVENT_NAMES = frozenset({
    "sidecar.state", "workspace.changed", "runtime.state", "task.state", "task.phase", "trace.item",
    "task.action_request", "task.verification", "task.manual_edit", "task.cancelled", "task.finished",
})

_DEFAULT_SIDECAR_RELATIVE = Path("sidecar") / "target" / "debug" / "halo-sidecar.exe"


def default_sidecar_exe() -> str:
    """解析默认 Sidecar 可执行文件路径：环境变量优先，否则相对项目根。"""
    env_path = os.environ.get("HALO_SIDECAR_EXE")
    if env_path:
        return env_path
    # __file__ = <root>/app/halo_studio/ipc/connection.py → parents[3] 是项目根
    project_root = Path(__file__).resolve().parents[3]
    return str(project_root / _DEFAULT_SIDECAR_RELATIVE)


class SidecarError(Exception):
    """IPC 客户端错误基类；str() 一律为用户可读中文。"""


class ConnectionClosedError(SidecarError):
    """连接已断开或尚未建立。"""


class RequestTimeoutError(SidecarError, TimeoutError):
    """请求在限定时间内未收到响应。"""


class RequestError(SidecarError):
    """Sidecar 返回 ok=false 的响应。"""

    def __init__(self, code: str, message: str, details: Mapping[str, Any] | None = None):
        self.code = code
        self.message = message
        self.details: dict[str, Any] = dict(details or {})
        super().__init__(f"[{code}] {message}")


class HelloNegotiationError(SidecarError):
    """协议版本协商失败；携带用户可读原因与双方版本信息。"""

    def __init__(self, message: str, *, code: str = "PROTOCOL_VERSION_UNSUPPORTED",
                 details: Mapping[str, Any] | None = None):
        self.code = code
        self.details: dict[str, Any] = dict(details or {})
        super().__init__(message)


class SidecarConnection:
    """与 Sidecar 子进程的单连接；实例不可重用（断开后需新建）。"""

    def __init__(
        self,
        exe_path: str | os.PathLike[str] | None = None,
        args: Sequence[str] = (),
        cwd: str | os.PathLike[str] | None = None,
        env: Mapping[str, str] | None = None,
    ):
        self._cmd: list[str] = [str(exe_path) if exe_path is not None else default_sidecar_exe(),
                                *(str(a) for a in args)]
        self._cwd = str(cwd) if cwd is not None else None
        self._env = dict(env) if env is not None else None
        self._proc: subprocess.Popen[bytes] | None = None
        self._reader: threading.Thread | None = None
        self._lock = threading.Lock()
        self._write_lock = threading.Lock()
        self._pending: dict[str, Future] = {}
        self._event_callbacks: list[Callable[[dict], None]] = []
        self._disconnect_callbacks: list[Callable[[str], None]] = []
        self._disconnect_fired = False
        self._disconnect_reason: str | None = None
        self._closing = False
        self._last_event_seq = 0
        self.protocol_version: int | None = None
        self.hello_result: dict[str, Any] | None = None

    # ---- 生命周期 -------------------------------------------------------

    def start(self) -> "SidecarConnection":
        if self._proc is not None:
            raise SidecarError("连接已启动，不可重复 start")
        creationflags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
        try:
            self._proc = subprocess.Popen(
                self._cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,  # 契约：stderr 仅诊断文本，不承载协议
                cwd=self._cwd,
                env=self._env,
                creationflags=creationflags,
            )
        except OSError as exc:
            raise SidecarError(f"无法启动 Sidecar 进程（{self._cmd[0]}）：{exc}") from exc
        self._reader = threading.Thread(target=self._read_loop, name="halo-sidecar-reader", daemon=True)
        self._reader.start()
        return self

    def close(self, grace: float = 3.0) -> None:
        """温和终止：先关 stdin 让 Sidecar 自行退出，超时再 terminate/kill。"""
        with self._lock:
            self._closing = True
        proc = self._proc
        if proc is None:
            return
        if proc.poll() is None:
            try:
                if proc.stdin is not None:
                    proc.stdin.close()
            except OSError:
                pass
            try:
                proc.wait(timeout=grace)
            except subprocess.TimeoutExpired:
                proc.terminate()
                try:
                    proc.wait(timeout=2.0)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    try:
                        proc.wait(timeout=2.0)
                    except subprocess.TimeoutExpired:
                        pass
        reader = self._reader
        if reader is not None and reader is not threading.current_thread():
            reader.join(timeout=2.0)

    @property
    def process(self) -> subprocess.Popen | None:
        return self._proc

    @property
    def is_connected(self) -> bool:
        return self._proc is not None and not self._disconnect_fired and not self._closing

    # ---- 回调注册 -------------------------------------------------------

    def add_event_callback(self, callback: Callable[[dict], None]) -> None:
        with self._lock:
            self._event_callbacks.append(callback)

    def add_disconnect_callback(self, callback: Callable[[str], None]) -> None:
        with self._lock:
            self._disconnect_callbacks.append(callback)
            fired, reason = self._disconnect_fired, self._disconnect_reason
        # 注册时已断连：立即补发，避免调用方错过通知
        if fired and reason is not None:
            self._safe_invoke(callback, reason)

    # ---- 请求 -----------------------------------------------------------

    def request_async(self, method: str, params: Mapping[str, Any] | None = None) -> Future:
        _, fut = self._submit(method, params)
        return fut

    def request(self, method: str, params: Mapping[str, Any] | None = None,
                timeout: float = 10.0) -> dict[str, Any]:
        req_id, fut = self._submit(method, params)
        try:
            return fut.result(timeout)
        except TimeoutError:
            with self._lock:
                self._pending.pop(req_id, None)
            raise RequestTimeoutError(f"请求 {method} 超过 {timeout} 秒未收到 Sidecar 响应") from None

    def hello(self, app_protocol_versions: Iterable[int] = (PROTOCOL_VERSION,),
              app_version: str = "0.1.0", timeout: float = 10.0) -> dict[str, Any]:
        """执行 sidecar.hello 版本协商；成功后保存 protocol_version。"""
        offered = [int(v) for v in app_protocol_versions]
        try:
            result = self.request(
                "sidecar.hello",
                {"app_protocol_versions": offered, "app_version": app_version},
                timeout=timeout,
            )
        except RequestError as exc:
            if exc.code == "PROTOCOL_VERSION_UNSUPPORTED":
                sidecar_versions = exc.details.get("sidecar_protocol_versions")
                raise HelloNegotiationError(
                    f"协议版本协商失败：应用支持 {offered}，Sidecar 支持 {sidecar_versions}。{exc.message}",
                    details=exc.details,
                ) from exc
            raise
        negotiated = result.get("protocol_version")
        if not isinstance(negotiated, int) or negotiated not in offered:
            raise HelloNegotiationError(
                f"Sidecar 返回的协议版本无效：{negotiated!r}（应用支持 {offered}）",
                details={"protocol_version": negotiated},
            )
        self.protocol_version = negotiated
        self.hello_result = result
        return result

    # ---- 内部实现 -------------------------------------------------------

    def _submit(self, method: str, params: Mapping[str, Any] | None) -> tuple[str, Future]:
        fut: Future = Future()
        req_id = f"r-{uuid.uuid4()}"
        envelope = {"v": PROTOCOL_VERSION, "kind": "request", "id": req_id,
                    "method": method, "params": dict(params or {})}
        with self._lock:
            if self._proc is None:
                fut.set_exception(ConnectionClosedError("连接尚未建立（未调用 start）"))
                return req_id, fut
            if self._disconnect_fired or self._closing:
                fut.set_exception(ConnectionClosedError(self._disconnect_reason or "连接已关闭"))
                return req_id, fut
            self._pending[req_id] = fut
        try:
            self._write_line(envelope)
        except SidecarError as exc:
            with self._lock:
                self._pending.pop(req_id, None)
            if not fut.done():
                fut.set_exception(exc)
        return req_id, fut

    def _write_line(self, obj: Mapping[str, Any]) -> None:
        data = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        if len(data) > MAX_LINE_BYTES:
            raise SidecarError("请求超过单行 1 MiB 上限，已拒绝发送")
        proc = self._proc
        if proc is None or proc.stdin is None:
            raise ConnectionClosedError("连接尚未建立")
        with self._write_lock:
            try:
                proc.stdin.write(data + b"\n")
                proc.stdin.flush()
            except (OSError, ValueError) as exc:
                raise ConnectionClosedError(f"向 Sidecar 写入失败，连接可能已断开：{exc}") from exc

    def _read_loop(self) -> None:
        assert self._proc is not None and self._proc.stdout is not None
        stream = self._proc.stdout
        protocol_reason: str | None = None
        while True:
            try:
                # 限长读：即使 Sidecar 发出超长行也不会无界缓冲
                raw = stream.readline(MAX_LINE_BYTES + 2)
            except (OSError, ValueError):
                break
            if not raw:
                break  # EOF
            stripped = raw.rstrip(b"\r\n")
            if not stripped:
                continue
            if len(stripped) > MAX_LINE_BYTES:
                protocol_reason = "协议错误：Sidecar 发送的单行长度超过 1 MiB 上限，连接已中断"
                break
            try:
                msg = json.loads(stripped.decode("utf-8"))
            except (UnicodeDecodeError, ValueError):
                protocol_reason = "协议错误：无法解析 Sidecar 发送的 JSON 行，连接已中断"
                break
            if not isinstance(msg, dict):
                protocol_reason = "协议错误：Sidecar 消息不是 JSON 对象，连接已中断"
                break
            protocol_reason = self._validate_inbound(msg)
            if protocol_reason is not None:
                break
            kind = msg.get("kind")
            if kind == "response":
                self._handle_response(msg)
            elif kind == "event":
                self._handle_event(msg)
            else:
                protocol_reason = f"协议错误：Sidecar 消息 kind 无效（{kind!r}），连接已中断"
                break
        self._on_connection_lost(protocol_reason)

    def _validate_inbound(self, msg: dict) -> str | None:
        """验证 UI 可观察到的 v1 封包形状，失败即断连而不猜测语义。"""
        if isinstance(msg.get("v"), bool) or msg.get("v") != PROTOCOL_VERSION:
            return "协议错误：Sidecar 返回了不受支持的协议版本，连接已中断"

        kind = msg.get("kind")
        if kind == "response":
            unknown = set(msg).difference({"v", "kind", "id", "ok", "result", "error"})
            if unknown:
                return "协议错误：Sidecar 响应含未声明字段，连接已中断"
            if not isinstance(msg.get("id"), str) or not isinstance(msg.get("ok"), bool):
                return "协议错误：Sidecar 响应缺少合法 id 或 ok 字段，连接已中断"
            if msg["ok"]:
                if "result" not in msg or "error" in msg:
                    return "协议错误：Sidecar 成功响应的 result/error 组合无效，连接已中断"
                if not isinstance(msg.get("result"), dict):
                    return "协议错误：Sidecar 成功响应缺少对象 result，连接已中断"
                return None
            if "error" not in msg or "result" in msg:
                return "协议错误：Sidecar 失败响应的 result/error 组合无效，连接已中断"
            error = msg.get("error")
            if not isinstance(error, dict):
                return "协议错误：Sidecar 失败响应缺少 error 对象，连接已中断"
            if set(error).difference({"code", "message", "details"}):
                return "协议错误：Sidecar 错误对象含未声明字段，连接已中断"
            if not isinstance(error.get("code"), str) or not isinstance(error.get("message"), str):
                return "协议错误：Sidecar 失败响应缺少稳定错误码或用户可读文案，连接已中断"
            if error["code"] not in _ERROR_CODES:
                return "协议错误：Sidecar 返回未声明的错误码，连接已中断"
            if "details" in error and not isinstance(error["details"], dict):
                return "协议错误：Sidecar 错误详情不是对象，连接已中断"
            return None

        if kind == "event":
            required = {"v", "kind", "seq", "ts", "task_id", "event", "payload"}
            if not required.issubset(msg):
                return "协议错误：Sidecar 事件缺少必填字段，连接已中断"
            if set(msg).difference(required):
                return "协议错误：Sidecar 事件含未声明字段，连接已中断"
            seq = msg.get("seq")
            if isinstance(seq, bool) or not isinstance(seq, int) or seq <= self._last_event_seq:
                return "协议错误：Sidecar 事件序号不是全局严格递增，连接已中断"
            if not isinstance(msg.get("ts"), str) or not isinstance(msg.get("event"), str):
                return "协议错误：Sidecar 事件缺少时间或事件名，连接已中断"
            if msg["event"] not in _EVENT_NAMES:
                return "协议错误：Sidecar 事件名未在 v1 契约中声明，连接已中断"
            if msg.get("task_id") is not None and not isinstance(msg.get("task_id"), str):
                return "协议错误：Sidecar 事件 task_id 非法，连接已中断"
            if not isinstance(msg.get("payload"), dict):
                return "协议错误：Sidecar 事件 payload 不是对象，连接已中断"
            self._last_event_seq = seq
            return None

        return f"协议错误：Sidecar 消息 kind 无效（{kind!r}），连接已中断"

    def _handle_response(self, msg: dict) -> None:
        with self._lock:
            fut = self._pending.pop(msg.get("id", ""), None)
        if fut is None or fut.done():
            return  # 超时后迟到的响应：按契约丢弃
        if msg.get("ok"):
            result = msg.get("result")
            fut.set_result(result if isinstance(result, dict) else {})
        else:
            err = msg.get("error") or {}
            fut.set_exception(RequestError(
                str(err.get("code", "INTERNAL")),
                str(err.get("message", "")),
                err.get("details") if isinstance(err.get("details"), dict) else {},
            ))

    def _handle_event(self, msg: dict) -> None:
        with self._lock:
            callbacks = list(self._event_callbacks)
        for cb in callbacks:
            self._safe_invoke(cb, msg)

    def _on_connection_lost(self, protocol_reason: str | None) -> None:
        with self._lock:
            if self._disconnect_fired:
                return
            self._disconnect_fired = True
            pending = list(self._pending.values())
            self._pending.clear()
            closing = self._closing
            callbacks = list(self._disconnect_callbacks)
        proc = self._proc
        # 协议错误时无法在字节流中重新同步，只能终止对端
        if protocol_reason is not None and proc is not None and proc.poll() is None:
            try:
                proc.kill()
                proc.wait(timeout=2.0)
            except (OSError, subprocess.TimeoutExpired):
                pass
        if closing:
            reason: str | None = None  # 应用主动关闭：不视为异常断连
        elif protocol_reason is not None:
            reason = protocol_reason
        else:
            exit_code: int | None = None
            if proc is not None:
                try:
                    exit_code = proc.wait(timeout=2.0)
                except subprocess.TimeoutExpired:
                    exit_code = proc.poll()
            if exit_code is not None:
                reason = f"Sidecar 进程已退出（退出码 {exit_code}），连接中断"
            else:
                reason = "Sidecar 输出流已关闭（EOF），连接中断"
        with self._lock:
            self._disconnect_reason = reason
        err = ConnectionClosedError(reason or "连接已由应用主动关闭")
        for fut in pending:
            if not fut.done():
                fut.set_exception(err)
        if reason is not None:
            for cb in callbacks:
                self._safe_invoke(cb, reason)

    @staticmethod
    def _safe_invoke(callback: Callable, arg: Any) -> None:
        try:
            callback(arg)
        except Exception:
            # 回调异常不允许破坏读线程；诊断由回调方自行负责
            pass
