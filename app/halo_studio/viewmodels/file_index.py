"""由 Sidecar 提供候选路径的进程内文件索引。"""

from __future__ import annotations

import time

from PySide6.QtCore import Property, QObject, Signal

from halo_studio.ipc.fs_client import FsClient, FsSearchResult


class FileIndex(QObject):
    refreshed = Signal()
    failed = Signal(str)
    busyChanged = Signal()
    truncatedChanged = Signal()

    def __init__(self, client, when_context, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._fs = FsClient(client)
        self._when_context = when_context
        self._generation = 0
        self._paths: list[str] = []
        self._updated_at = 0.0
        self._busy = False
        self._truncated = False

    def snapshot(self) -> tuple[int, list[str]]:
        return self._generation, list(self._paths)

    def ensure_fresh(self, ttl_seconds: int = 30) -> None:
        if not self._when_context.hasWorkspace:
            self.failed.emit("工作区未信任，文件索引不可用")
            return
        if self._busy or (self._paths and time.monotonic() - self._updated_at < ttl_seconds):
            return
        self._busy = True
        self.busyChanged.emit()
        self._fs.search(
            glob=None,
            query=None,
            case_sensitive=False,
            max_results=20_000,
            on_ok=self._on_refreshed,
            on_error=self._on_error,
        )

    def invalidate(self) -> None:
        self._updated_at = 0.0

    def _on_refreshed(self, result: FsSearchResult) -> None:
        paths = sorted({item.path.replace("\\", "/") for item in result.items if item.path}, key=str.casefold)
        self._paths = paths
        self._generation += 1
        self._updated_at = time.monotonic()
        self._set_busy(False)
        if self._truncated != result.truncated:
            self._truncated = result.truncated
            self.truncatedChanged.emit()
        self.refreshed.emit()

    def _on_error(self, error: dict) -> None:
        self._set_busy(False)
        self.failed.emit(str((error or {}).get("message") or "文件索引加载失败"))

    def _set_busy(self, value: bool) -> None:
        if value != self._busy:
            self._busy = value
            self.busyChanged.emit()

    busy = Property(bool, lambda self: self._busy, notify=busyChanged)
    truncated = Property(bool, lambda self: self._truncated, notify=truncatedChanged)
