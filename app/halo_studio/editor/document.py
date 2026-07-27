"""编辑器文档状态：文本、磁盘版本、编码与 UI 元数据。"""

from __future__ import annotations

from uuid import uuid4

from PySide6.QtCore import Property, QObject, Signal, Slot
from PySide6.QtGui import QTextCursor, QTextDocument

from .constants import HIGHLIGHT_MAX_BYTES, READONLY_MAX_BYTES
from .fsio import FileReadResult


class EditorDocument(QObject):
    """一个打开标签的可观察状态，文本保留在 Qt 文档中。"""

    changed = Signal()
    contentChanged = Signal()
    gutterChanged = Signal()

    def __init__(self, path: str, preview: bool = False, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._document_id = f"doc-{uuid4()}"
        self._path = path
        self._title = path.rsplit("/", 1)[-1]
        self._preview = preview
        self._read_only = False
        self._oversized = False
        self._highlight_enabled = True
        self._eol = "crlf"
        self._mixed_eol = False
        self._encoding = "utf-8"
        self._has_bom = False
        self._state = "loading"
        self._disk_sha256 = ""
        self._disk_mtime = ""
        self._manual_edit_badge = False
        self._baseline_changed = False
        self._cursor_line = -1
        self._cursor_column = -1
        self._gutter_anchors: list[tuple[QTextCursor, dict]] = []
        self._conflict_hash = ""
        self._qdoc = QTextDocument(self)
        self._connect_qdoc(self._qdoc)

    def _connect_qdoc(self, qdoc: QTextDocument) -> None:
        qdoc.modificationChanged.connect(self._on_modified)
        qdoc.contentsChanged.connect(self._on_contents_changed)
        qdoc.blockCountChanged.connect(lambda _count: self.changed.emit())

    def load(self, result: FileReadResult) -> None:
        """用 Sidecar 的读取结果初始化，清空撤销栈并记录磁盘版本。"""
        self._eol = result.eol
        self._mixed_eol = result.mixed_eol
        self._encoding = result.encoding
        self._has_bom = result.has_bom
        self._oversized = result.size > READONLY_MAX_BYTES
        self._highlight_enabled = result.size <= HIGHLIGHT_MAX_BYTES
        self._read_only = result.readonly or result.decode_lossy or self._oversized or result.binary
        self._disk_sha256 = result.sha256
        self._disk_mtime = result.mtime
        self._state = "ready"
        had_gutter_decorations = bool(self._gutter_anchors)
        self._gutter_anchors = []
        self._qdoc.setPlainText(result.text)
        self._qdoc.clearUndoRedoStacks()
        self._qdoc.setModified(False)
        self.changed.emit()
        self.contentChanged.emit()
        if had_gutter_decorations:
            self.gutterChanged.emit()

    def attach(self, qdoc: QTextDocument) -> None:
        """用 QML TextArea 的 QTextDocument 替换测试/无界面阶段的后备文档。"""
        if qdoc is None or qdoc is self._qdoc:
            return
        text = self._qdoc.toPlainText()
        dirty = self._qdoc.isModified()
        gutter_decorations = self._get_gutter_decorations()
        self._gutter_anchors = []
        self._qdoc = qdoc
        self._connect_qdoc(qdoc)
        qdoc.setPlainText(text)
        qdoc.clearUndoRedoStacks()
        qdoc.setModified(dirty)
        self.set_gutter_decorations(gutter_decorations)
        self.changed.emit()
        self.contentChanged.emit()

    def is_attached(self) -> bool:
        return self._qdoc is not None

    @Slot(str)
    def setText(self, text: str) -> None:  # noqa: N802
        if self._read_only:
            return
        text = str(text)
        previous = self._qdoc.toPlainText()
        if text == previous:
            return

        # Replace only the changed span so QTextCursor gutter anchors retain their
        # document-relative positions across unsaved edits.
        prefix_length = _common_prefix_length(previous, text)
        suffix_length = _common_suffix_length(previous, text, prefix_length)
        previous_end = len(previous) - suffix_length if suffix_length else len(previous)
        next_end = len(text) - suffix_length if suffix_length else len(text)
        cursor = QTextCursor(self._qdoc)
        cursor.setPosition(_utf16_offset(previous[:prefix_length]))
        cursor.setPosition(_utf16_offset(previous[:previous_end]), QTextCursor.KeepAnchor)
        cursor.beginEditBlock()
        cursor.removeSelectedText()
        cursor.insertText(text[prefix_length:next_end])
        cursor.endEditBlock()
        self._qdoc.setModified(True)
        self._preview = False
        self.changed.emit()
        self.contentChanged.emit()

    def build_save_text(self) -> str:
        separator = "\r\n" if self._eol == "crlf" else "\n"
        return self._qdoc.toPlainText().replace("\n", separator)

    def build_save_payload(self) -> bytes:
        content = self.build_save_text()
        if self._encoding == "utf-8":
            return content.encode("utf-8")
        if self._encoding == "utf-8-bom":
            return b"\xef\xbb\xbf" + content.encode("utf-8")
        if self._encoding == "utf-16le":
            return b"\xff\xfe" + content.encode("utf-16le")
        if self._encoding == "utf-16be":
            return b"\xfe\xff" + content.encode("utf-16be")
        raise UnicodeError("未知编码的文件不可写入")

    def mark_saved(self, sha256: str, mtime: str) -> None:
        self._disk_sha256 = sha256
        self._disk_mtime = mtime
        self._state = "ready"
        self._mixed_eol = False
        self._qdoc.setModified(False)
        self.changed.emit()

    def set_state(self, state: str) -> None:
        if state != self._state:
            self._state = state
            self.changed.emit()

    def set_conflict_hash(self, sha256: str) -> None:
        self._conflict_hash = sha256

    def set_read_only(self, value: bool) -> None:
        value = bool(value)
        if value != self._read_only:
            self._read_only = value
            self.changed.emit()

    def set_manual_edit_badge(self, value: bool) -> None:
        value = bool(value)
        if value != self._manual_edit_badge:
            self._manual_edit_badge = value
            self.changed.emit()

    def set_baseline_changed(self, value: bool) -> None:
        value = bool(value)
        if value != self._baseline_changed:
            self._baseline_changed = value
            self.changed.emit()

    def set_preview(self, value: bool) -> None:
        value = bool(value)
        if value != self._preview:
            self._preview = value
            self.changed.emit()

    def set_title(self, title: str) -> None:
        if title != self._title:
            self._title = title
            self.changed.emit()

    def set_cursor(self, line: int, column: int) -> None:
        line = max(1, int(line))
        column = max(1, int(column))
        if (line, column) != (self._cursor_line, self._cursor_column):
            self._cursor_line = line
            self._cursor_column = column
            self.changed.emit()

    def set_gutter_decorations(self, decorations: list[dict]) -> None:
        anchors: list[tuple[QTextCursor, dict]] = []
        for item in decorations:
            if not isinstance(item, dict):
                continue
            try:
                line = int(item.get("line", 0))
            except (TypeError, ValueError):
                continue
            block = self._qdoc.findBlockByNumber(line - 1)
            if not block.isValid():
                continue
            cursor = QTextCursor(self._qdoc)
            cursor.setPosition(block.position())
            anchors.append((cursor, {key: value for key, value in item.items() if key != "line"}))
        self._gutter_anchors = anchors
        self.gutterChanged.emit()

    def undo(self) -> None:
        if not self._read_only:
            self._qdoc.undo()

    def redo(self) -> None:
        if not self._read_only:
            self._qdoc.redo()

    def _on_modified(self, _modified: bool) -> None:
        self.changed.emit()

    def _on_contents_changed(self) -> None:
        self.contentChanged.emit()
        if self._gutter_anchors:
            self.gutterChanged.emit()

    def _get_document_id(self) -> str:
        return self._document_id

    def _get_path(self) -> str:
        return self._path

    def _get_file_name(self) -> str:
        return self._path.rsplit("/", 1)[-1]

    def _get_title(self) -> str:
        return self._title

    def _get_dirty(self) -> bool:
        return self._qdoc.isModified()

    def _get_read_only(self) -> bool:
        return self._read_only

    def _get_oversized(self) -> bool:
        return self._oversized

    def _get_highlight_enabled(self) -> bool:
        return self._highlight_enabled

    def _get_eol(self) -> str:
        return self._eol

    def _get_mixed_eol(self) -> bool:
        return self._mixed_eol

    def _get_encoding(self) -> str:
        return self._encoding

    def _get_has_bom(self) -> bool:
        return self._has_bom

    def _get_line_count(self) -> int:
        return self._qdoc.blockCount()

    def _get_manual_edit_badge(self) -> bool:
        return self._manual_edit_badge

    def _get_baseline_changed(self) -> bool:
        return self._baseline_changed

    def _get_state(self) -> str:
        return self._state

    def _get_cursor_line(self) -> int:
        return self._cursor_line

    def _get_cursor_column(self) -> int:
        return self._cursor_column

    def _get_disk_sha256(self) -> str:
        return self._disk_sha256

    def _get_text(self) -> str:
        return self._qdoc.toPlainText()

    def _get_preview(self) -> bool:
        return self._preview

    def _get_gutter_decorations(self) -> list[dict]:
        decorations: list[dict] = []
        for cursor, metadata in self._gutter_anchors:
            block = cursor.block()
            if block.isValid():
                decorations.append({"line": block.blockNumber() + 1, **metadata})
        return decorations

    documentId = Property(str, _get_document_id, constant=True)
    path = Property(str, _get_path, constant=True)
    fileName = Property(str, _get_file_name, notify=changed)
    title = Property(str, _get_title, notify=changed)
    dirty = Property(bool, _get_dirty, notify=changed)
    readOnly = Property(bool, _get_read_only, notify=changed)
    oversized = Property(bool, _get_oversized, notify=changed)
    highlightEnabled = Property(bool, _get_highlight_enabled, notify=changed)
    eol = Property(str, _get_eol, notify=changed)
    mixedEol = Property(bool, _get_mixed_eol, notify=changed)
    encoding = Property(str, _get_encoding, notify=changed)
    hasBom = Property(bool, _get_has_bom, notify=changed)
    lineCount = Property(int, _get_line_count, notify=changed)
    manualEditBadge = Property(bool, _get_manual_edit_badge, notify=changed)
    baselineChanged = Property(bool, _get_baseline_changed, notify=changed)
    state = Property(str, _get_state, notify=changed)
    cursorLine = Property(int, _get_cursor_line, notify=changed)
    cursorColumn = Property(int, _get_cursor_column, notify=changed)
    diskSha256 = Property(str, _get_disk_sha256, notify=changed)
    text = Property(str, _get_text, notify=contentChanged)
    preview = Property(bool, _get_preview, notify=changed)
    gutterDecorations = Property("QVariantList", _get_gutter_decorations, notify=gutterChanged)


def _common_prefix_length(left: str, right: str) -> int:
    limit = min(len(left), len(right))
    index = 0
    while index < limit and left[index] == right[index]:
        index += 1
    return index


def _common_suffix_length(left: str, right: str, prefix_length: int) -> int:
    limit = min(len(left), len(right)) - prefix_length
    length = 0
    while length < limit and left[-(length + 1)] == right[-(length + 1)]:
        length += 1
    return length


def _utf16_offset(value: str) -> int:
    return len(value.encode("utf-16-le")) // 2
