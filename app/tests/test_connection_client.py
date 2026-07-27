"""SidecarClient（Qt 包装）测试：信号与请求回调必须在主线程送达。"""

from __future__ import annotations

import json
import os
import sys
import threading
from pathlib import Path

from halo_studio.ipc.client import SidecarClient

FAKE_SIDECAR = Path(__file__).resolve().parent / "fake_sidecar.py"


def make_client(tmp_path: Path, script: dict | None = None, versions: str = "1") -> SidecarClient:
    env = dict(os.environ)
    env["FAKE_SIDECAR_PROTOCOL_VERSIONS"] = versions
    if script is not None:
        script_path = tmp_path / "fake_script.json"
        script_path.write_text(json.dumps(script, ensure_ascii=False), encoding="utf-8")
        env["FAKE_SIDECAR_SCRIPT"] = str(script_path)
    else:
        env.pop("FAKE_SIDECAR_SCRIPT", None)
    return SidecarClient(exe_path=sys.executable, args=["-u", str(FAKE_SIDECAR)], env=env)


def test_client_connect_request_and_kill_disconnect(qtbot, tmp_path):
    client = make_client(tmp_path)
    try:
        with qtbot.waitSignal(client.connected, timeout=15000) as blocker:
            client.start()
        hello = blocker.args[0]
        assert hello["protocol_version"] == 1
        assert client.protocol_version == 1

        results: list[dict] = []
        threads: list[int] = []

        def on_result(result: dict) -> None:
            results.append(result)
            threads.append(threading.get_ident())

        client.request("workspace.status", on_result=on_result)
        qtbot.waitUntil(lambda: bool(results), timeout=10000)
        assert results[0] == {"active": False}
        # 回调必须经 Signal 转到主线程执行
        assert threads[0] == threading.get_ident()

        with qtbot.waitSignal(client.disconnected, timeout=15000) as disc:
            client.connection.process.kill()
        assert disc.args[0].strip() != ""
    finally:
        client.close()


def test_client_version_mismatch_emits_disconnected(qtbot, tmp_path):
    client = make_client(tmp_path, versions="99")
    try:
        with qtbot.waitSignal(client.disconnected, timeout=15000) as blocker:
            client.start()
        reason = blocker.args[0]
        assert "协议版本" in reason
        assert client.protocol_version is None
    finally:
        client.close()


def test_client_events_arrive_on_main_thread(qtbot, tmp_path):
    script = {"events_on_hello": [
        {"event": "task.phase", "payload": {"phase": "planning"}},
        {"event": "task.phase", "payload": {"phase": "editing"}},
    ]}
    client = make_client(tmp_path, script=script)
    events: list[dict] = []
    client.eventReceived.connect(events.append)
    try:
        with qtbot.waitSignal(client.connected, timeout=15000):
            client.start()
        qtbot.waitUntil(
            lambda: sum(e["event"] == "task.phase" for e in events) >= 2, timeout=10000)
        seqs = [e["seq"] for e in events]
        assert all(b > a for a, b in zip(seqs, seqs[1:]))
        phases = [e["payload"]["phase"] for e in events if e["event"] == "task.phase"]
        assert phases == ["planning", "editing"]
    finally:
        client.close()


def test_client_request_error_callback(qtbot, tmp_path):
    script = {"responses": {"task.create": {"error": {
        "code": "RUNTIME_NOT_READY", "message": "目标运行时尚未就绪", "details": {}}}}}
    client = make_client(tmp_path, script=script)
    errors: list[Exception] = []
    try:
        with qtbot.waitSignal(client.connected, timeout=15000):
            client.start()
        client.request("task.create", {"agent": "pi"}, on_error=errors.append)
        qtbot.waitUntil(lambda: bool(errors), timeout=10000)
        err = errors[0]
        assert getattr(err, "code", None) == "RUNTIME_NOT_READY"
        assert "就绪" in getattr(err, "message", "")
    finally:
        client.close()
