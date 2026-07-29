"""命令面板与快速打开的公共 ViewModel 行为。"""

from __future__ import annotations

from dataclasses import dataclass

import pytest
from PySide6.QtCore import QCoreApplication

from halo_studio.commands.registry import CommandRegistry
from halo_studio.commands.when_context import WhenContext
from halo_studio.viewmodels.file_index import FileIndex
from halo_studio.viewmodels.palette_vm import PaletteViewModel


@dataclass
class Request:
    method: str
    params: dict
    on_ok: object
    on_err: object


class FakeClient:
    def __init__(self) -> None:
        self.requests: list[Request] = []

    def request(self, method, params, on_ok=None, on_err=None) -> None:
        self.requests.append(Request(method, dict(params), on_ok, on_err))

    def ok(self, result: dict, index: int = -1) -> None:
        callback = self.requests[index].on_ok
        if callback is not None:
            callback(result)


class EditorRecorder:
    def __init__(self) -> None:
        self.opened: list[str] = []
        self.documents = None

    def openFile(self, path: str) -> None:  # noqa: N802
        self.opened.append(path)


@pytest.fixture(scope="session")
def core_app():
    app = QCoreApplication.instance()
    return app or QCoreApplication([])


def _search_result(*paths: str) -> dict:
    return {
        "items": [{"path": path} for path in paths],
        "truncated": False,
        "scanned_files": len(paths),
    }


def test_quick_open_indexes_through_sidecar_and_accepts_a_file(core_app):
    client = FakeClient()
    context = WhenContext()
    context.set_key("hasWorkspace", True)
    registry = CommandRegistry(context)
    editor = EditorRecorder()
    index = FileIndex(client, context)
    palette = PaletteViewModel(registry, index, editor)

    palette.open("")
    assert client.requests[-1].method == "fs.search"
    assert client.requests[-1].params == {
        "glob": None,
        "query": None,
        "case_sensitive": False,
        "max_results": 20000,
    }
    client.ok(_search_result("src/main.py", "README.md"))
    palette.setQuery("main")
    row = palette.results.get(0)
    assert row["itemKind"] == "file"
    assert row["itemId"] == "src/main.py"
    assert row["matchedOn"] == "basename"

    palette.acceptSelected()
    assert editor.opened == ["src/main.py"]
    assert palette.visible is False


def test_command_mode_filters_when_and_executes_through_registry(core_app):
    client = FakeClient()
    context = WhenContext()
    registry = CommandRegistry(context)
    calls: list[str] = []
    registry.register("palette.commands", "显示所有命令", "面板", lambda: calls.append("palette"))
    registry.register("editor.save", "保存文件", "编辑器", lambda: calls.append("save"), when="hasActiveEditor")
    palette = PaletteViewModel(registry, FileIndex(client, context), EditorRecorder())

    palette.open(">")
    assert [palette.results.get(row)["itemId"] for row in range(palette.results.rowCount())] == ["palette.commands"]
    palette.acceptSelected()
    assert calls == ["palette"]

    context.set_key("hasActiveEditor", True)
    palette.open(">save")
    assert palette.results.get(0)["itemId"] == "editor.save"
    palette.acceptSelected()
    assert calls[-1] == "save"


def test_untrusted_file_index_does_not_issue_a_request(core_app):
    client = FakeClient()
    context = WhenContext()
    index = FileIndex(client, context)
    failures: list[str] = []
    index.failed.connect(failures.append)
    index.ensure_fresh()
    assert client.requests == []
    assert failures == ["工作区未信任，文件索引不可用"]
