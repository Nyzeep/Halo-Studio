"""编辑器核心的公共服务接口测试。"""

from __future__ import annotations

from dataclasses import dataclass

import pytest
from PySide6.QtCore import QCoreApplication

from halo_studio.editor.service import EditorService


@dataclass
class Request:
    method: str
    params: dict
    on_ok: object
    on_err: object


class FakeClient:
    def __init__(self) -> None:
        self.requests: list[Request] = []
        self._subscriptions: dict[str, list] = {}

    def request(self, method, params, on_ok=None, on_err=None) -> None:
        self.requests.append(Request(method, dict(params), on_ok, on_err))

    def subscribe(self, event, handler) -> None:
        self._subscriptions.setdefault(event, []).append(handler)

    def ok(self, result: dict, index: int = -1) -> None:
        callback = self.requests[index].on_ok
        if callback is not None:
            callback(result)

    def err(self, code: str, message: str, details: dict | None = None, index: int = -1) -> None:
        callback = self.requests[index].on_err
        if callback is not None:
            callback({"code": code, "message": message, "details": details or {}})

    def emit(self, event: str, payload: dict) -> None:
        envelope = {"event": event, "payload": payload}
        for handler in list(self._subscriptions.get(event, [])):
            handler(envelope)


@pytest.fixture(scope="session")
def core_app():
    app = QCoreApplication.instance()
    return app or QCoreApplication([])


@pytest.fixture()
def client() -> FakeClient:
    return FakeClient()


def _read(path: str = "src/main.py", content: str = "print('ok')\r\n", **changes) -> dict:
    result = {
        "path": path,
        "content": content,
        "encoding": "utf-8",
        "lossy": False,
        "line_ending": "crlf",
        "hash": "sha256:before",
        "size": len(content.encode("utf-8")),
        "mtime": "2026-07-27T08:00:00Z",
        "readonly": False,
    }
    result.update(changes)
    return result


def _open_ready(service: EditorService, client: FakeClient, path: str = "src/main.py"):
    service.openFile(path)
    assert client.requests[-1].method == "fs.read"
    assert client.requests[-1].params == {"path": path}
    client.ok(_read(path))
    return service.activeDocument


def test_open_deduplicates_normalized_paths_and_honors_line_jump(core_app, client):
    service = EditorService(client)
    jumps: list[tuple[str, int, int]] = []
    service.gotoLineRequested.connect(lambda doc_id, line, column: jumps.append((doc_id, line, column)))

    service.openFile("SRC\\Main.py", 4)
    assert client.requests[-1].params == {"path": "SRC/Main.py"}
    client.ok(_read("SRC/Main.py"))
    document_id = service.activeDocumentId
    assert jumps == [(document_id, 4, 1)]
    assert service.openCount == 1

    service.openFile("src/main.py")
    assert len(client.requests) == 1
    assert service.activeDocumentId == document_id


def test_save_preserves_line_ending_uses_hash_and_updates_document(core_app, client):
    service = EditorService(client)
    document = _open_ready(service, client)
    document.setText("print('changed')\n")
    assert document.dirty is True

    service.save()
    assert client.requests[-1].method == "fs.write"
    assert client.requests[-1].params == {
        "path": "src/main.py",
        "content": "print('changed')\r\n",
        "expected_hash": "sha256:before",
        "encoding": "utf-8",
    }
    client.ok({
        "path": "src/main.py",
        "hash": "sha256:after",
        "size": 18,
        "mtime": "2026-07-27T08:01:00Z",
    })
    assert document.dirty is False
    assert document.diskSha256 == "sha256:after"
    assert document.state == "ready"


def test_gutter_decorations_follow_unsaved_prefix_edits(core_app, client):
    service = EditorService(client)
    document = _open_ready(service, client)
    document.setText("first\nsecond\nthird")
    service.setGutterDecorations(
        document.documentId,
        [{"line": 2, "kind": "attribution", "colorToken": "gutterAgentChangeBackground", "tooltip": "evidence"}],
    )

    document.setText("prefix\nfirst\nsecond\nthird")

    assert document.gutterDecorations == [
        {"line": 3, "kind": "attribution", "colorToken": "gutterAgentChangeBackground", "tooltip": "evidence"}
    ]


def test_document_text_replacement_uses_utf16_cursor_offsets(core_app, client):
    service = EditorService(client)
    document = _open_ready(service, client)
    original = "prefix \U0001F600\nsecond\nthird"
    updated = "prefix x\nsecond\nthird"
    document.setText(original)
    service.setGutterDecorations(
        document.documentId,
        [{"line": 3, "kind": "attribution", "colorToken": "gutterAgentChangeBackground", "tooltip": "evidence"}],
    )

    document.setText(updated)

    assert document.text == updated
    assert document.gutterDecorations[0]["line"] == 3


def test_conflict_requires_explicit_resolution_and_keeps_dirty_text(core_app, client):
    service = EditorService(client)
    document = _open_ready(service, client)
    document.setText("local\n")
    conflicts: list[tuple[str, str]] = []
    service.conflictDetected.connect(lambda doc_id, path: conflicts.append((doc_id, path)))

    service.save()
    client.err("FS_CONFLICT", "文件内容已被外部修改", {"current_hash": "sha256:remote"})
    assert conflicts == [(document.documentId, "src/main.py")]
    assert document.state == "conflict"
    assert document.dirty is True

    service.resolveConflict(document.documentId, "overwrite")
    assert client.requests[-1].method == "fs.write"
    assert client.requests[-1].params["expected_hash"] == "sha256:remote"

    client.ok({"path": "src/main.py", "hash": "sha256:merged", "size": 6, "mtime": "later"})
    assert document.dirty is False
    assert document.diskSha256 == "sha256:merged"


def test_lossy_reads_are_readonly_and_manual_edit_badges_follow_events(core_app, client):
    service = EditorService(client)
    service.openFile("src/legacy.txt")
    client.ok(_read("src/legacy.txt", "\ufffd", encoding="unknown", lossy=True))
    document = service.activeDocument
    assert document.readOnly is True

    service.openFile("src/main.py")
    client.ok(_read("src/main.py"))
    current = service.activeDocument
    client.emit("task.manual_edit", {"path": "src/main.py"})
    assert current.manualEditBadge is True

    client.emit("task.state", {"state": "review_ready"})
    assert current.manualEditBadge is False


def test_workspace_switch_force_closes_open_documents(core_app, client):
    service = EditorService(client)
    _open_ready(service, client, "src/first.py")
    _open_ready(service, client, "src/second.py")
    assert service.openCount == 2

    client.emit("workspace.changed", {"active": True, "real_path": "D:/next-workspace"})

    assert service.openCount == 0
    assert service.activeDocumentId == ""
