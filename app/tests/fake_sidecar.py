"""符合 IPC v1 契约的测试 Sidecar（stdin/stdout JSONL）。仅供测试，绝不进入生产路径。

可脚本化方式：
- 环境变量 FAKE_SIDECAR_PROTOCOL_VERSIONS：逗号分隔的受支持协议版本（默认 "1"）。
- 环境变量 FAKE_SIDECAR_SCRIPT：JSON 脚本文件路径，结构：
  {
    "supported_versions": [1],                  // 可选，优先于环境变量
    "events_on_hello": [EventSpec, ...],        // hello 成功后主动推送
    "responses": {"<method>": ResponseSpec}     // 覆盖/新增方法应答
  }
  ResponseSpec 字段（按优先级）：
    delay_ms          — 应答前延迟（每个请求独立线程处理，可制造乱序响应）
    raw_lines         — 按字面量输出的行（用于坏 JSON 测试），不再发正常响应
    huge_line_bytes   — 输出一行 N 字节的 "x"（用于超长行测试）
    no_response       — true 时不发送响应（用于超时测试）
    error             — {"code","message","details"} 错误响应
    result            — 正常 result（默认 {}）
    events            — [EventSpec] 响应后推送的事件
  EventSpec：{"event": "...", "payload": {...}, "task_id": null, "delay_ms": 0}

内置默认应答：workspace.status → {"active": false}。
未 hello 前任何其他方法 → HELLO_REQUIRED；未知方法 → METHOD_NOT_FOUND。
事件带全局单调递增 seq（由唯一写锁保护）。stdin EOF → 正常退出（温和终止路径）。
"""

from __future__ import annotations

import json
import os
import sys
import threading
import time
from datetime import datetime, timezone


def _load_script() -> dict:
    path = os.environ.get("FAKE_SIDECAR_SCRIPT")
    if not path:
        return {}
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


SCRIPT = _load_script()


def _supported_versions() -> list[int]:
    if "supported_versions" in SCRIPT:
        return [int(v) for v in SCRIPT["supported_versions"]]
    raw = os.environ.get("FAKE_SIDECAR_PROTOCOL_VERSIONS", "1")
    return [int(p) for p in raw.split(",") if p.strip()]


SUPPORTED = _supported_versions()
CAPABILITIES = ["workspace", "config", "pi", "opencode", "task", "review", "handoff", "history"]
BUILTIN_RESPONSES: dict[str, dict] = {
    "workspace.status": {"result": {"active": False}},
}

_out = sys.stdout.buffer
_write_lock = threading.Lock()
_seq = 0
_hello_done = threading.Event()


def _ts() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _write_bytes(data: bytes) -> None:
    with _write_lock:
        _out.write(data)
        _out.flush()


def _write_json(obj: dict) -> None:
    _write_bytes(json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n")


def _push_event(event: str, payload: dict, task_id: str | None = None) -> None:
    global _seq
    # seq 分配与写出必须在同一把锁内，保证全局单调且行序一致
    with _write_lock:
        _seq += 1
        envelope = {"v": 1, "kind": "event", "seq": _seq, "ts": _ts(),
                    "task_id": task_id, "event": event, "payload": payload}
        _out.write(json.dumps(envelope, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n")
        _out.flush()


def _respond_ok(req_id: str, result: dict) -> None:
    _write_json({"v": 1, "kind": "response", "id": req_id, "ok": True, "result": result})


def _respond_err(req_id: str, code: str, message: str, details: dict | None = None) -> None:
    _write_json({"v": 1, "kind": "response", "id": req_id, "ok": False,
                 "error": {"code": code, "message": message, "details": details or {}}})


def _emit_events(specs: list[dict]) -> None:
    for spec in specs:
        delay = spec.get("delay_ms")
        if delay:
            time.sleep(delay / 1000.0)
        _push_event(spec.get("event", "test.event"), spec.get("payload", {}), spec.get("task_id"))


def _handle_hello(req_id: str, params: dict) -> None:
    try:
        app_versions = [int(v) for v in params.get("app_protocol_versions", [])]
    except (TypeError, ValueError):
        app_versions = []
    common = sorted(set(app_versions) & set(SUPPORTED))
    if not common:
        _respond_err(
            req_id,
            "PROTOCOL_VERSION_UNSUPPORTED",
            f"协议版本不受支持：Sidecar 支持的协议版本为 {SUPPORTED}",
            {"sidecar_protocol_versions": SUPPORTED},
        )
        return
    _hello_done.set()
    _respond_ok(req_id, {
        "protocol_version": common[-1],
        "sidecar_version": "0.0.0-fake",
        "capabilities": CAPABILITIES,
    })
    _emit_events(SCRIPT.get("events_on_hello", []))


def _handle_request(msg: dict) -> None:
    req_id = str(msg.get("id", ""))
    method = str(msg.get("method", ""))
    params = msg.get("params") if isinstance(msg.get("params"), dict) else {}
    if method == "sidecar.hello":
        _handle_hello(req_id, params)
        return
    if not _hello_done.is_set():
        _respond_err(req_id, "HELLO_REQUIRED", "请先调用 sidecar.hello 完成握手")
        return
    spec = (SCRIPT.get("responses") or {}).get(method)
    if spec is None:
        spec = BUILTIN_RESPONSES.get(method)
    if spec is None:
        _respond_err(req_id, "METHOD_NOT_FOUND", f"未知方法：{method}")
        return
    delay = spec.get("delay_ms")
    if delay:
        time.sleep(delay / 1000.0)
    if spec.get("raw_lines") is not None:
        for line in spec["raw_lines"]:
            _write_bytes(str(line).encode("utf-8") + b"\n")
    elif spec.get("huge_line_bytes"):
        _write_bytes(b"x" * int(spec["huge_line_bytes"]) + b"\n")
    elif spec.get("no_response"):
        pass
    elif spec.get("error") is not None:
        err = spec["error"]
        _respond_err(req_id, str(err.get("code", "INTERNAL")), str(err.get("message", "")),
                     err.get("details") if isinstance(err.get("details"), dict) else {})
    else:
        result = spec.get("result")
        _respond_ok(req_id, result if isinstance(result, dict) else {})
    _emit_events(spec.get("events", []))


def main() -> int:
    _push_event("sidecar.state", {"state": "ready", "protocol_version": max(SUPPORTED)})
    stdin = sys.stdin.buffer
    while True:
        line = stdin.readline()
        if not line:
            break  # stdin EOF：宿主温和关闭，正常退出
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            print("fake_sidecar: 忽略无法解析的输入行", file=sys.stderr)
            continue
        if not isinstance(msg, dict) or msg.get("kind") != "request":
            continue
        # 每个请求独立线程：允许 delay_ms 制造乱序响应，验证按 id 配对
        threading.Thread(target=_handle_request, args=(msg,), daemon=True).start()
    return 0


if __name__ == "__main__":
    sys.exit(main())
