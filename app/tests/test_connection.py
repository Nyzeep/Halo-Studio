"""connection 层契约测试：真实子进程运行 fake_sidecar.py（stdin/stdout JSONL）。"""

from __future__ import annotations

import contextlib
import json
import os
import sys
import time
from pathlib import Path

import pytest

from halo_studio.ipc.connection import (
    MAX_LINE_BYTES,
    ConnectionClosedError,
    HelloNegotiationError,
    RequestError,
    RequestTimeoutError,
    SidecarConnection,
)

FAKE_SIDECAR = Path(__file__).resolve().parent / "fake_sidecar.py"


def make_connection(tmp_path: Path, script: dict | None = None,
                    versions: str = "1") -> SidecarConnection:
    env = dict(os.environ)
    env["FAKE_SIDECAR_PROTOCOL_VERSIONS"] = versions
    if script is not None:
        script_path = tmp_path / "fake_script.json"
        script_path.write_text(json.dumps(script, ensure_ascii=False), encoding="utf-8")
        env["FAKE_SIDECAR_SCRIPT"] = str(script_path)
    else:
        env.pop("FAKE_SIDECAR_SCRIPT", None)
    return SidecarConnection(exe_path=sys.executable, args=["-u", str(FAKE_SIDECAR)], env=env)


@contextlib.contextmanager
def connected(tmp_path: Path, script: dict | None = None, versions: str = "1",
              hello: bool = True):
    conn = make_connection(tmp_path, script=script, versions=versions)
    conn.start()
    try:
        if hello:
            conn.hello([1])
        yield conn
    finally:
        conn.close()


def wait_until(predicate, timeout: float = 8.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.02)
    return predicate()


# ---- hello 协商 ----------------------------------------------------------

def test_hello_negotiation_success(tmp_path):
    with connected(tmp_path, hello=False) as conn:
        result = conn.hello([1])
        assert result["protocol_version"] == 1
        assert conn.protocol_version == 1
        assert isinstance(result["capabilities"], list) and "workspace" in result["capabilities"]
        assert result["sidecar_version"]


def test_hello_version_mismatch_gives_readable_reason(tmp_path):
    with connected(tmp_path, versions="99", hello=False) as conn:
        with pytest.raises(HelloNegotiationError) as excinfo:
            conn.hello([1])
        err = excinfo.value
        assert err.code == "PROTOCOL_VERSION_UNSUPPORTED"
        assert err.details.get("sidecar_protocol_versions") == [99]
        # 用户可读中文原因，供界面直接展示
        assert "协议版本" in str(err)
        assert conn.protocol_version is None


def test_request_before_hello_rejected(tmp_path):
    with connected(tmp_path, hello=False) as conn:
        with pytest.raises(RequestError) as excinfo:
            conn.request("workspace.status", timeout=5.0)
        assert excinfo.value.code == "HELLO_REQUIRED"
        assert excinfo.value.message


# ---- 请求/响应配对 --------------------------------------------------------

def test_builtin_workspace_status(tmp_path):
    with connected(tmp_path) as conn:
        assert conn.request("workspace.status", timeout=5.0) == {"active": False}


def test_request_response_pairing_out_of_order(tmp_path):
    script = {"responses": {
        "slow.echo": {"result": {"which": "slow"}, "delay_ms": 600},
        "fast.echo": {"result": {"which": "fast"}},
    }}
    with connected(tmp_path, script=script) as conn:
        fut_slow = conn.request_async("slow.echo")
        fut_fast = conn.request_async("fast.echo")
        # fast 后发先至：配对必须按 id 而非到达顺序
        assert fut_fast.result(5.0)["which"] == "fast"
        assert not fut_slow.done()
        assert fut_slow.result(5.0)["which"] == "slow"


def test_error_response_maps_to_request_error(tmp_path):
    script = {"responses": {
        "task.create": {"error": {"code": "WORKSPACE_NOT_TRUSTED",
                                  "message": "工作区尚未确认信任，无法创建任务",
                                  "details": {"workspace_id": "ws-x"}}},
    }}
    with connected(tmp_path, script=script) as conn:
        with pytest.raises(RequestError) as excinfo:
            conn.request("task.create", {"agent": "pi"}, timeout=5.0)
        err = excinfo.value
        assert err.code == "WORKSPACE_NOT_TRUSTED"
        assert "信任" in err.message
        assert err.details == {"workspace_id": "ws-x"}


def test_runtime_capability_unavailable_is_request_error_without_disconnect(tmp_path):
    script = {"responses": {
        "task.create": {"error": {
            "code": "RUNTIME_CAPABILITY_UNAVAILABLE",
            "message": "当前 OpenCode 版本不支持受管任务执行",
            "details": {"recovery": "请更新 OpenCode 后重试"},
        }},
    }}
    with connected(tmp_path, script=script) as conn:
        with pytest.raises(RequestError) as excinfo:
            conn.request("task.create", {"agent": "opencode"}, timeout=5.0)
        assert excinfo.value.code == "RUNTIME_CAPABILITY_UNAVAILABLE"
        assert excinfo.value.details == {"recovery": "请更新 OpenCode 后重试"}

        # 这是合法的业务失败，不能被当作未知协议错误关闭连接。
        assert conn.is_connected
        assert conn.request("workspace.status", timeout=5.0) == {"active": False}


def test_request_timeout(tmp_path):
    script = {"responses": {"test.silent": {"no_response": True}}}
    with connected(tmp_path, script=script) as conn:
        with pytest.raises(RequestTimeoutError):
            conn.request("test.silent", timeout=0.5)
        # 超时不破坏连接：后续请求仍可用
        assert conn.request("workspace.status", timeout=5.0) == {"active": False}


# ---- 事件顺序 -------------------------------------------------------------

def test_event_seq_strictly_monotonic(tmp_path):
    phases = ["planning", "editing", "verifying", "summarizing"]
    script = {"responses": {"test.push": {
        "result": {"pushed": True},
        "events": [{"event": "task.phase", "payload": {"phase": p}} for p in phases],
    }}}
    events: list[dict] = []
    conn = make_connection(tmp_path, script=script)
    conn.add_event_callback(events.append)
    conn.start()
    try:
        conn.hello([1])
        assert conn.request("test.push", timeout=5.0) == {"pushed": True}
        assert wait_until(lambda: sum(e["event"] == "task.phase" for e in events) >= len(phases))
    finally:
        conn.close()
    # 首条事件是 sidecar.state；全局 seq 严格单调递增
    assert events[0]["event"] == "sidecar.state"
    seqs = [e["seq"] for e in events]
    assert all(b > a for a, b in zip(seqs, seqs[1:]))
    got_phases = [e["payload"]["phase"] for e in events if e["event"] == "task.phase"]
    assert got_phases == phases


# ---- 断连检测 -------------------------------------------------------------

def test_killed_process_fires_disconnect_with_reason(tmp_path):
    script = {"responses": {"test.silent": {"no_response": True}}}
    reasons: list[str] = []
    conn = make_connection(tmp_path, script=script)
    conn.add_disconnect_callback(reasons.append)
    conn.start()
    try:
        conn.hello([1])
        pending = conn.request_async("test.silent")
        conn.process.kill()
        assert wait_until(lambda: bool(reasons))
        assert reasons[0].strip() != ""
        assert "退出" in reasons[0] or "EOF" in reasons[0]
        # 断连时挂起请求必须失败，而不是永远等待
        with pytest.raises(ConnectionClosedError):
            pending.result(5.0)
        assert not conn.is_connected
    finally:
        conn.close()


def test_bad_json_line_disconnects_as_protocol_error(tmp_path):
    script = {"responses": {"test.garbage": {"raw_lines": ["这不是 JSON 行 }{"]}}}
    reasons: list[str] = []
    conn = make_connection(tmp_path, script=script)
    conn.add_disconnect_callback(reasons.append)
    conn.start()
    try:
        conn.hello([1])
        pending = conn.request_async("test.garbage")
        assert wait_until(lambda: bool(reasons))
        assert "协议错误" in reasons[0]
        with pytest.raises(ConnectionClosedError):
            pending.result(5.0)
    finally:
        conn.close()


def test_oversized_line_disconnects_as_protocol_error(tmp_path):
    script = {"responses": {"test.huge": {"huge_line_bytes": MAX_LINE_BYTES * 2}}}
    reasons: list[str] = []
    conn = make_connection(tmp_path, script=script)
    conn.add_disconnect_callback(reasons.append)
    conn.start()
    try:
        conn.hello([1])
        pending = conn.request_async("test.huge")
        assert wait_until(lambda: bool(reasons))
        assert "协议错误" in reasons[0] and "1 MiB" in reasons[0]
        with pytest.raises(ConnectionClosedError):
            pending.result(5.0)
    finally:
        conn.close()


def test_wrong_version_response_disconnects_as_protocol_error(tmp_path):
    script = {"responses": {"test.wrong_version": {"raw_lines": [
        '{"v":2,"kind":"response","id":"r-unrelated","ok":true,"result":{}}'
    ]}}}
    reasons: list[str] = []
    conn = make_connection(tmp_path, script=script)
    conn.add_disconnect_callback(reasons.append)
    conn.start()
    try:
        conn.hello([1])
        pending = conn.request_async("test.wrong_version")
        assert wait_until(lambda: bool(reasons))
        with pytest.raises(ConnectionClosedError):
            pending.result(2.0)
    finally:
        conn.close()
    assert "协议错误" in reasons[0]


@pytest.mark.parametrize(
    "raw_line",
    [
        # v1 事件的 task_id 是必填字段，即使值可为 null。
        '{"v":1,"kind":"event","seq":2,"ts":"2026-07-26T08:00:00Z","event":"task.phase","payload":{}}',
        # 封包契约关闭了 additionalProperties，未知字段不能被 UI 猜测性接受。
        '{"v":1,"kind":"event","seq":2,"ts":"2026-07-26T08:00:00Z","task_id":null,"event":"task.phase","payload":{},"unexpected":true}',
        '{"v":1,"kind":"response","id":"r-unrelated","ok":true,"result":{},"unexpected":true}',
    ],
)
def test_inbound_envelope_rejects_missing_or_unknown_fields(tmp_path, raw_line):
    script = {"responses": {"test.malformed_envelope": {"raw_lines": [raw_line]}}}
    reasons: list[str] = []
    conn = make_connection(tmp_path, script=script)
    conn.add_disconnect_callback(reasons.append)
    conn.start()
    try:
        conn.hello([1])
        pending = conn.request_async("test.malformed_envelope")
        assert wait_until(lambda: bool(reasons))
        with pytest.raises(ConnectionClosedError):
            pending.result(2.0)
    finally:
        conn.close()
    assert "协议错误" in reasons[0]


def test_close_terminates_child_gently(tmp_path):
    reasons: list[str] = []
    conn = make_connection(tmp_path)
    conn.add_disconnect_callback(reasons.append)
    conn.start()
    conn.hello([1])
    proc = conn.process
    conn.close()
    # 关闭 stdin 后 fake 读到 EOF 正常退出（退出码 0），无需强杀
    assert proc.wait(timeout=5.0) == 0
    time.sleep(0.2)
    # 应用主动关闭不算异常断连，不触发回调
    assert reasons == []
    # 关闭后的请求立即失败
    with pytest.raises(ConnectionClosedError):
        conn.request("workspace.status", timeout=1.0)
