"""fs.* 的薄 IPC 客户端。

本模块不读写工作区，也不验证路径。它只把已有的回调式契约客户端映射为
结构化结果，保证编辑器、资源管理器和快速打开都只能经 Sidecar 访问文件。
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable


@dataclass(frozen=True)
class FsEntry:
    name: str
    path: str
    kind: str
    size: int
    mtime: str
    readonly: bool


@dataclass(frozen=True)
class FsListResult:
    path: str
    entries: list[FsEntry]
    truncated: bool


@dataclass(frozen=True)
class FsReadResult:
    path: str
    content: str
    encoding: str
    lossy: bool
    line_ending: str
    hash: str
    size: int
    mtime: str
    readonly: bool


@dataclass(frozen=True)
class FsWriteResult:
    path: str
    hash: str
    size: int
    mtime: str


@dataclass(frozen=True)
class FsSearchItem:
    path: str
    line: int | None = None
    column: int | None = None
    preview: str | None = None
    preview_truncated: bool | None = None


@dataclass(frozen=True)
class FsSearchResult:
    items: list[FsSearchItem]
    truncated: bool
    scanned_files: int


ErrorCallback = Callable[[dict], None]


class FsClient:
    """以既有 ``request(method, params, on_ok, on_err)`` 鸭子类型消费 fs.*。"""

    def __init__(self, client) -> None:
        self._client = client

    def list(
        self,
        path: str = "",
        depth: int = 1,
        on_ok: Callable[[FsListResult], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.list",
            {"path": path, "depth": depth},
            _list_result,
            on_ok,
            on_error,
        )

    def read(
        self,
        path: str,
        on_ok: Callable[[FsReadResult], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request("fs.read", {"path": path}, _read_result, on_ok, on_error)

    def write(
        self,
        path: str,
        content: str,
        expected_hash: str,
        encoding: str = "utf-8",
        on_ok: Callable[[FsWriteResult], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.write",
            {
                "path": path,
                "content": content,
                "expected_hash": expected_hash,
                "encoding": encoding,
            },
            _write_result,
            on_ok,
            on_error,
        )

    def create_file(
        self,
        path: str,
        content: str = "",
        on_ok: Callable[[FsEntry], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.create_file",
            {"path": path, "content": content},
            lambda result: _entry(result.get("entry") or {}),
            on_ok,
            on_error,
        )

    def create_dir(
        self,
        path: str,
        on_ok: Callable[[FsEntry], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.create_dir",
            {"path": path},
            lambda result: _entry(result.get("entry") or {}),
            on_ok,
            on_error,
        )

    def rename(
        self,
        from_path: str,
        to_path: str,
        on_ok: Callable[[FsEntry], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.rename",
            {"from": from_path, "to": to_path},
            lambda result: _entry(result.get("entry") or {}),
            on_ok,
            on_error,
        )

    def stat(
        self,
        path: str,
        on_ok: Callable[[FsEntry], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.stat",
            {"path": path},
            lambda result: _entry(result.get("entry") or {}),
            on_ok,
            on_error,
        )

    def search(
        self,
        glob: str | None = None,
        query: str | None = None,
        case_sensitive: bool = False,
        max_results: int = 500,
        on_ok: Callable[[FsSearchResult], None] | None = None,
        on_error: ErrorCallback | None = None,
    ):
        return self._request(
            "fs.search",
            {
                "glob": glob,
                "query": query,
                "case_sensitive": case_sensitive,
                "max_results": max_results,
            },
            _search_result,
            on_ok,
            on_error,
        )

    def _request(self, method, params, converter, on_ok, on_error):
        def accept(result: dict) -> None:
            if on_ok is not None:
                on_ok(converter(result or {}))

        return self._client.request(method, params, accept, on_error)


def _entry(value: dict) -> FsEntry:
    return FsEntry(
        name=str(value.get("name") or ""),
        path=str(value.get("path") or ""),
        kind=str(value.get("kind") or "file"),
        size=int(value.get("size") or 0),
        mtime=str(value.get("mtime") or ""),
        readonly=bool(value.get("readonly", False)),
    )


def _list_result(value: dict) -> FsListResult:
    return FsListResult(
        path=str(value.get("path") or ""),
        entries=[_entry(item) for item in value.get("entries") or []],
        truncated=bool(value.get("truncated", False)),
    )


def _read_result(value: dict) -> FsReadResult:
    return FsReadResult(
        path=str(value.get("path") or ""),
        content=str(value.get("content") or ""),
        encoding=str(value.get("encoding") or "utf-8"),
        lossy=bool(value.get("lossy", False)),
        line_ending=str(value.get("line_ending") or "none"),
        hash=str(value.get("hash") or ""),
        size=int(value.get("size") or 0),
        mtime=str(value.get("mtime") or ""),
        readonly=bool(value.get("readonly", False)),
    )


def _write_result(value: dict) -> FsWriteResult:
    return FsWriteResult(
        path=str(value.get("path") or ""),
        hash=str(value.get("hash") or ""),
        size=int(value.get("size") or 0),
        mtime=str(value.get("mtime") or ""),
    )


def _search_result(value: dict) -> FsSearchResult:
    return FsSearchResult(
        items=[
            FsSearchItem(
                path=str(item.get("path") or ""),
                line=_optional_int(item.get("line")),
                column=_optional_int(item.get("column")),
                preview=_optional_str(item.get("preview")),
                preview_truncated=(
                    bool(item["preview_truncated"])
                    if "preview_truncated" in item
                    else None
                ),
            )
            for item in value.get("items") or []
        ],
        truncated=bool(value.get("truncated", False)),
        scanned_files=int(value.get("scanned_files") or 0),
    )


def _optional_int(value) -> int | None:
    return int(value) if value is not None else None


def _optional_str(value) -> str | None:
    return str(value) if value is not None else None
