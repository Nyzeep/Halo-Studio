"""编辑器对 ``fs.*`` 的单点异步适配层。"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

from halo_studio.ipc.fs_client import FsClient, FsReadResult, FsWriteResult


@dataclass(frozen=True)
class FileReadResult:
    text: str
    encoding: str
    has_bom: bool
    eol: str
    mixed_eol: bool
    size: int
    sha256: str
    mtime: str
    binary: bool
    decode_lossy: bool
    readonly: bool


@dataclass(frozen=True)
class FileStatResult:
    exists: bool
    size: int
    mtime: str
    sha256: str


@dataclass(frozen=True)
class FileWriteResult:
    sha256: str
    mtime: str


class FsIo:
    """将 Sidecar 的文本契约收敛成编辑器所需的稳定结果。"""

    def __init__(self, client) -> None:
        self._fs = FsClient(client)

    def read(
        self,
        path: str,
        on_ok: Callable[[FileReadResult], None],
        on_err: Callable[[dict], None],
    ) -> None:
        self._fs.read(path, lambda result: on_ok(_from_read_result(result)), on_err)

    def stat(
        self,
        path: str,
        on_ok: Callable[[FileStatResult], None],
        on_err: Callable[[dict], None],
    ) -> None:
        """v1 的 stat 不含哈希，读取结果是唯一有哈希的权威来源。"""
        self._fs.read(
            path,
            lambda result: on_ok(
                FileStatResult(True, result.size, result.mtime, result.hash)
            ),
            on_err,
        )

    def write(
        self,
        path: str,
        content: str,
        expected_sha256: str,
        encoding: str,
        on_ok: Callable[[FileWriteResult], None],
        on_err: Callable[[dict], None],
    ) -> None:
        self._fs.write(
            path,
            content,
            expected_sha256,
            encoding,
            lambda result: on_ok(_from_write_result(result)),
            on_err,
        )


def _from_read_result(result: FsReadResult) -> FileReadResult:
    source = result.content
    crlf_count = source.count("\r\n")
    bare_lf_count = source.count("\n") - crlf_count
    mixed = result.line_ending == "mixed" or (crlf_count > 0 and bare_lf_count > 0)
    if result.line_ending == "crlf" or (mixed and crlf_count >= bare_lf_count):
        eol = "crlf"
    elif result.line_ending == "lf" or bare_lf_count > 0:
        eol = "lf"
    else:
        # Windows 首发的无换行新文件以 CRLF 作为后续保存默认。
        eol = "crlf"
    return FileReadResult(
        text=source.replace("\r\n", "\n").replace("\r", "\n"),
        encoding=result.encoding,
        has_bom=result.encoding in {"utf-8-bom", "utf-16le", "utf-16be"},
        eol=eol,
        mixed_eol=mixed,
        size=result.size,
        sha256=result.hash,
        mtime=result.mtime,
        binary=False,
        decode_lossy=result.lossy,
        readonly=result.readonly,
    )


def _from_write_result(result: FsWriteResult) -> FileWriteResult:
    return FileWriteResult(sha256=result.hash, mtime=result.mtime)
