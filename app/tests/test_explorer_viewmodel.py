"""Explorer 文件树的公共 ViewModel 契约测试。"""

from __future__ import annotations

from dataclasses import dataclass

import pytest
from PySide6.QtCore import QCoreApplication

from halo_studio.ipc.fs_client import FsEntry, FsListResult
from halo_studio.viewmodels.explorer_viewmodel import Decoration, ExplorerViewModel


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

    def emit(self, event: str, payload: dict) -> None:
        envelope = {"event": event, "payload": payload}
        for handler in list(self._subscriptions.get(event, [])):
            handler(envelope)


class EditorRecorder:
    def __init__(self) -> None:
        self.previewed: list[str] = []
        self.pinned: list[str] = []

    def openFile(self, path: str, pinned: bool = False) -> None:  # noqa: N802
        (self.pinned if pinned else self.previewed).append(path)


@pytest.fixture(scope="session")
def core_app():
    app = QCoreApplication.instance()
    return app or QCoreApplication([])


@pytest.fixture()
def client() -> FakeClient:
    return FakeClient()


def _trusted(client: FakeClient) -> None:
    client.emit("workspace.changed", {"active": True, "trust": "trusted"})


def _entries(*items: tuple[str, str]) -> dict:
    return {
        "path": "",
        "entries": [
            {
                "name": name,
                "path": name,
                "kind": kind,
                "size": 0,
                "mtime": "2026-07-27T08:00:00Z",
                "readonly": False,
            }
            for name, kind in items
        ],
        "truncated": False,
    }


def _row_paths(vm: ExplorerViewModel) -> list[str]:
    model = vm.model
    return [model.data(model.index(row, 0), model.RelPathRole) for row in range(model.rowCount())]


def test_workspace_gate_and_lazy_expand_uses_cached_listing(core_app, client):
    vm = ExplorerViewModel(client)

    client.emit("workspace.changed", {"active": True, "trust": "untrusted"})
    vm.refresh()
    assert client.requests == []
    assert vm.workspaceActive is True
    assert vm.workspaceTrusted is False

    _trusted(client)
    assert [(item.method, item.params) for item in client.requests] == [
        ("fs.list", {"path": "", "depth": 1})
    ]
    client.ok(_entries(("src", "dir"), ("README.md", "file")))
    assert _row_paths(vm) == ["src", "README.md"]

    vm.expand("src")
    assert client.requests[-1].method == "fs.list"
    assert client.requests[-1].params == {"path": "src", "depth": 1}
    src_index = vm.model.index(0, 0)
    assert vm.model.data(src_index, vm.model.LoadingRole) is True
    client.ok({
        "path": "src",
        "entries": [{
            "name": "main.py", "path": "src/main.py", "kind": "file", "size": 12,
            "mtime": "2026-07-27T08:00:00Z", "readonly": False,
        }],
        "truncated": False,
    })
    assert _row_paths(vm) == ["src", "src/main.py", "README.md"]

    vm.collapse("src")
    vm.expand("src")
    assert len(client.requests) == 2
    assert _row_paths(vm) == ["src", "src/main.py", "README.md"]


def test_terminal_task_refreshes_loaded_paths_and_workspace_close_clears_tree(core_app, client):
    vm = ExplorerViewModel(client)
    _trusted(client)
    client.ok(_entries(("src", "dir")))
    vm.expand("src")
    client.ok({"path": "src", "entries": [], "truncated": False})
    request_count = len(client.requests)

    client.emit("task.state", {"state": "review_ready"})
    assert [(item.method, item.params) for item in client.requests[request_count:]] == [
        ("fs.list", {"path": "", "depth": 1}),
        ("fs.list", {"path": "src", "depth": 1}),
    ]

    client.emit("workspace.changed", {"active": False})
    assert vm.workspaceActive is False
    assert vm.model.rowCount() == 0


def test_create_validate_open_and_bubble_decorations(core_app, client):
    editor = EditorRecorder()
    vm = ExplorerViewModel(client, editor=editor)
    _trusted(client)
    client.ok(_entries(("src", "dir")))
    vm.expand("src")
    client.ok({"path": "src", "entries": [], "truncated": False})

    assert vm.validateName("") == "名称不能为空"
    assert vm.validateName("a/b") == "名称不能包含路径分隔符"
    assert vm.validateName("CON") == "名称不能使用 Windows 保留名"
    assert vm.validateName("name.") == "名称不能以空格或点结尾"
    assert vm.validateName("main.py") == ""

    vm.createFile("src", "new.py")
    assert client.requests[-1].method == "fs.create_file"
    assert client.requests[-1].params == {"path": "src/new.py", "content": ""}
    client.ok({
        "entry": {
            "name": "new.py", "path": "src/new.py", "kind": "file", "size": 0,
            "mtime": "2026-07-27T08:00:00Z", "readonly": False,
        }
    })
    assert "src/new.py" in _row_paths(vm)
    assert editor.previewed == ["src/new.py"]

    vm.collapse("src")
    vm.model.set_decorations({
        "src/new.py": Decoration("M", "decorationModifiedForeground", "任务基线后已修改", True)
    })
    src_index = vm.model.index(0, 0)
    assert vm.model.data(src_index, vm.model.BadgeLetterRole) == "M"
    assert vm.model.data(src_index, vm.model.BadgeColorTokenRole) == "decorationModifiedForeground"

    vm.openPreview("src/new.py")
    vm.openPinned("src/new.py")
    assert editor.previewed[-1] == "src/new.py"
    assert editor.pinned == ["src/new.py"]


def test_rename_root_file_keeps_a_workspace_relative_target(core_app, client):
    vm = ExplorerViewModel(client)
    _trusted(client)
    client.ok(_entries(("old.txt", "file")))

    vm.rename("old.txt", "new.txt")
    assert client.requests[-1].method == "fs.rename"
    assert client.requests[-1].params == {"from": "old.txt", "to": "new.txt"}


def test_apply_listing_accepts_fs_client_result(core_app, client):
    """模型边界可直接消费 FsClient 的结构化响应，而非字典内部字段。"""
    vm = ExplorerViewModel(client)
    vm.model.apply_listing(
        "",
        FsListResult(
            path="",
            entries=[FsEntry("src", "src", "dir", 0, "", False)],
            truncated=False,
        ),
    )
    assert _row_paths(vm) == ["src"]
