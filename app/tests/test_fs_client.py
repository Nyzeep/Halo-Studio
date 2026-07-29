"""fs.* IPC 薄客户端的契约测试。"""

from __future__ import annotations

from dataclasses import dataclass

from halo_studio.ipc.fs_client import FsClient, FsReadResult


@dataclass
class Call:
    method: str
    params: dict
    on_ok: object
    on_err: object


class Client:
    def __init__(self) -> None:
        self.calls: list[Call] = []

    def request(self, method, params, on_ok=None, on_err=None):
        self.calls.append(Call(method, dict(params), on_ok, on_err))

    def ok(self, result: dict, index: int = -1) -> None:
        callback = self.calls[index].on_ok
        if callback is not None:
            callback(result)

    def err(self, error: dict, index: int = -1) -> None:
        callback = self.calls[index].on_err
        if callback is not None:
            callback(error)


def test_read_maps_result_and_preserves_lossy_metadata():
    client = Client()
    fs = FsClient(client)
    results: list[FsReadResult] = []

    fs.read("src/main.rs", results.append)

    assert client.calls[-1].method == "fs.read"
    assert client.calls[-1].params == {"path": "src/main.rs"}
    client.ok({
        "path": "src/main.rs",
        "content": "fn main() {}\n",
        "encoding": "unknown",
        "lossy": True,
        "line_ending": "lf",
        "hash": "sha256:old",
        "size": 13,
        "mtime": "2026-07-27T08:00:00Z",
        "readonly": False,
    })

    assert results == [FsReadResult(
        path="src/main.rs",
        content="fn main() {}\n",
        encoding="unknown",
        lossy=True,
        line_ending="lf",
        hash="sha256:old",
        size=13,
        mtime="2026-07-27T08:00:00Z",
        readonly=False,
    )]


def test_all_methods_use_contract_shapes_and_forward_errors_unchanged():
    client = Client()
    fs = FsClient(client)
    failures: list[dict] = []

    fs.list("src", 2, on_error=failures.append)
    fs.write("src/main.rs", "updated", "sha256:old", "utf-8-bom")
    fs.create_file("src/new.rs", "")
    fs.create_dir("src/new")
    fs.rename("src/new.rs", "src/renamed.rs")
    fs.stat("src/main.rs")
    fs.search(glob="**/*.rs", query="fn\\s+main", case_sensitive=True, max_results=40)

    assert [(call.method, call.params) for call in client.calls] == [
        ("fs.list", {"path": "src", "depth": 2}),
        ("fs.write", {
            "path": "src/main.rs", "content": "updated", "expected_hash": "sha256:old", "encoding": "utf-8-bom",
        }),
        ("fs.create_file", {"path": "src/new.rs", "content": ""}),
        ("fs.create_dir", {"path": "src/new"}),
        ("fs.rename", {"from": "src/new.rs", "to": "src/renamed.rs"}),
        ("fs.stat", {"path": "src/main.rs"}),
        ("fs.search", {
            "glob": "**/*.rs", "query": "fn\\s+main", "case_sensitive": True, "max_results": 40,
        }),
    ]
    client.err({"code": "FS_CONFLICT", "message": "文件内容已被外部修改", "details": {"current_hash": "sha256:new"}}, 0)
    assert failures == [{
        "code": "FS_CONFLICT", "message": "文件内容已被外部修改", "details": {"current_hash": "sha256:new"},
    }]
