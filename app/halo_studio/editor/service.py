"""EditorService：打开、保存、冲突、标签与任务事件的编排。"""

from __future__ import annotations

from collections import deque

from PySide6.QtCore import Property, QAbstractListModel, QModelIndex, QObject, Qt, Signal, Slot

from .document import EditorDocument
from .fsio import FileReadResult, FileWriteResult, FsIo
from .search import SearchController


class OpenDocumentsModel(QAbstractListModel):
    DocumentIdRole = int(Qt.ItemDataRole.UserRole) + 1
    PathRole = DocumentIdRole + 1
    FileNameRole = DocumentIdRole + 2
    TitleRole = DocumentIdRole + 3
    DirtyRole = DocumentIdRole + 4
    ReadOnlyRole = DocumentIdRole + 5
    PreviewRole = DocumentIdRole + 6
    ManualEditBadgeRole = DocumentIdRole + 7
    BaselineChangedRole = DocumentIdRole + 8
    EolRole = DocumentIdRole + 9
    EncodingRole = DocumentIdRole + 10
    StateRole = DocumentIdRole + 11

    _ROLE_NAMES = {
        DocumentIdRole: b"documentId",
        PathRole: b"path",
        FileNameRole: b"fileName",
        TitleRole: b"title",
        DirtyRole: b"dirty",
        ReadOnlyRole: b"readOnly",
        PreviewRole: b"preview",
        ManualEditBadgeRole: b"manualEditBadge",
        BaselineChangedRole: b"baselineChanged",
        EolRole: b"eol",
        EncodingRole: b"encoding",
        StateRole: b"state",
    }

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._documents: list[EditorDocument] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._documents)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._documents)):
            return None
        doc = self._documents[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return doc.title
        values = {
            self.DocumentIdRole: doc.documentId,
            self.PathRole: doc.path,
            self.FileNameRole: doc.fileName,
            self.TitleRole: doc.title,
            self.DirtyRole: doc.dirty,
            self.ReadOnlyRole: doc.readOnly,
            self.PreviewRole: doc.preview,
            self.ManualEditBadgeRole: doc.manualEditBadge,
            self.BaselineChangedRole: doc.baselineChanged,
            self.EolRole: doc.eol,
            self.EncodingRole: doc.encoding,
            self.StateRole: doc.state,
        }
        return values.get(role)

    def roleNames(self):  # noqa: N802
        return dict(self._ROLE_NAMES)

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if not 0 <= row < len(self._documents):
            return {}
        doc = self._documents[row]
        return {
            "documentId": doc.documentId,
            "path": doc.path,
            "fileName": doc.fileName,
            "title": doc.title,
            "dirty": doc.dirty,
            "readOnly": doc.readOnly,
            "preview": doc.preview,
            "manualEditBadge": doc.manualEditBadge,
            "baselineChanged": doc.baselineChanged,
            "eol": doc.eol,
            "encoding": doc.encoding,
            "state": doc.state,
        }

    def documents(self) -> list[EditorDocument]:
        return list(self._documents)

    def add(self, document: EditorDocument) -> None:
        row = len(self._documents)
        self.beginInsertRows(QModelIndex(), row, row)
        self._documents.append(document)
        self.endInsertRows()
        document.changed.connect(lambda doc=document: self.notify_document(doc))

    def remove(self, document: EditorDocument) -> None:
        try:
            row = self._documents.index(document)
        except ValueError:
            return
        self.beginRemoveRows(QModelIndex(), row, row)
        self._documents.pop(row)
        self.endRemoveRows()

    def notify_document(self, document: EditorDocument) -> None:
        try:
            row = self._documents.index(document)
        except ValueError:
            return
        index = self.index(row, 0)
        self.dataChanged.emit(index, index, list(self._ROLE_NAMES))


class EditorService(QObject):
    activeChanged = Signal()
    currentSelectionChanged = Signal()
    gotoLineRequested = Signal(str, int, int)
    closeConfirmationRequested = Signal(str, str)
    conflictDetected = Signal(str, str)
    saveFailed = Signal(str, str, str)
    openFailed = Signal(str, str, str)
    allCloseFinished = Signal(bool)
    manualEditMarked = Signal("QVariantList")
    documentSaved = Signal(str, str, str)

    _TERMINAL_STATES = {"review_ready", "accepted", "rejected", "cancelled", "failed", "interrupted"}

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._client = client
        self._fsio = FsIo(client)
        self._model = OpenDocumentsModel(self)
        self._by_id: dict[str, EditorDocument] = {}
        self._by_path: dict[str, EditorDocument] = {}
        self._mru: deque[str] = deque()
        self._active_id = ""
        self._selection = _empty_selection()
        self._pending_close: set[str] = set()
        self._close_all_queue: deque[str] | None = None
        self._search = SearchController(self._active_document, self)
        self._search.matchSelected.connect(self._on_search_match)
        client.subscribe("task.manual_edit", self._on_manual_edit)
        client.subscribe("task.state", self._on_task_state)
        client.subscribe("workspace.changed", self._on_workspace_changed)
        client.subscribe("client.disconnected", self._on_disconnected)

    @Slot(str)
    @Slot(str, int)
    @Slot(str, int, bool)
    def openFile(self, path: str, line: int = -1, preview: bool = False) -> None:  # noqa: N802
        clean_path = _clean_path(path)
        if not clean_path:
            return
        identity = _path_key(clean_path)
        existing = self._by_path.get(identity)
        if existing is not None:
            if not preview:
                existing.set_preview(False)
            self.activate(existing.documentId)
            if line >= 1:
                self.gotoLine(line, 1)
            return

        if preview:
            for old in self._model.documents():
                if old.preview and not old.dirty:
                    self._close_document(old.documentId)
                    break
        document = EditorDocument(clean_path, preview=preview, parent=self)
        self._by_id[document.documentId] = document
        self._by_path[identity] = document
        self._model.add(document)
        self.activate(document.documentId)
        self._fsio.read(
            clean_path,
            lambda result: self._on_opened(document.documentId, result, line),
            lambda error: self._on_open_error(document.documentId, clean_path, error),
        )

    @Slot(str)
    def activate(self, document_id: str) -> None:
        if document_id not in self._by_id:
            return
        if document_id in self._mru:
            self._mru.remove(document_id)
        self._mru.appendleft(document_id)
        if document_id != self._active_id:
            self._active_id = document_id
            self._selection = _empty_selection(self._by_id[document_id].path)
            self.activeChanged.emit()
            self.currentSelectionChanged.emit()
        self._search.refresh()

    @Slot()
    def nextTab(self) -> None:  # noqa: N802
        self._cycle_tab(1)

    @Slot()
    def prevTab(self) -> None:  # noqa: N802
        self._cycle_tab(-1)

    @Slot()
    @Slot(str)
    def save(self, document_id: str = "") -> None:
        doc = self._by_id.get(document_id or self._active_id)
        if doc is None:
            return
        if not doc.dirty:
            if doc.documentId in self._pending_close:
                self._close_document(doc.documentId)
                self._advance_close_all()
            return
        if doc.readOnly:
            self.saveFailed.emit(doc.documentId, "EDITOR_READ_ONLY", "当前文件为只读，不能保存")
            return
        try:
            content = doc.build_save_text()
            # 预先构造一次以尽早报告无法表示的编码。
            doc.build_save_payload()
        except (UnicodeError, UnicodeEncodeError) as exc:
            self.saveFailed.emit(doc.documentId, "EDITOR_ENCODING", f"无法按原编码保存：{exc}")
            return
        doc.set_state("saving")
        self._fsio.write(
            doc.path,
            content,
            doc.diskSha256,
            doc.encoding,
            lambda result: self._on_saved(doc.documentId, result),
            lambda error: self._on_save_error(doc.documentId, error),
        )

    @Slot()
    def saveAll(self) -> None:  # noqa: N802
        for document in self._model.documents():
            if document.dirty:
                self.save(document.documentId)

    @Slot(str, str)
    def resolveConflict(self, document_id: str, decision: str) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is None or doc.state != "conflict":
            return
        if decision == "cancel":
            doc.set_state("ready")
            return
        if decision == "reload":
            doc.set_state("loading")
            self._fsio.read(
                doc.path,
                lambda result: self._on_reloaded(doc.documentId, result),
                lambda error: self._on_save_error(doc.documentId, error),
            )
            return
        if decision == "overwrite":
            doc.set_state("saving")
            self._fsio.write(
                doc.path,
                doc.build_save_text(),
                doc._conflict_hash or doc.diskSha256,
                doc.encoding,
                lambda result: self._on_saved(doc.documentId, result),
                lambda error: self._on_save_error(doc.documentId, error),
            )

    @Slot(str)
    def closeTab(self, document_id: str) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is None:
            return
        if doc.dirty:
            self.closeConfirmationRequested.emit(doc.documentId, doc.title)
            return
        self._close_document(document_id)

    @Slot(str, str)
    def resolveClose(self, document_id: str, decision: str) -> None:  # noqa: N802
        if document_id not in self._by_id:
            return
        if decision == "cancel":
            self._cancel_close_all_if_needed()
            return
        if decision == "discard":
            self._close_document(document_id)
            self._advance_close_all()
            return
        if decision == "save":
            self._pending_close.add(document_id)
            self.save(document_id)

    @Slot()
    def requestCloseAll(self) -> None:  # noqa: N802
        self._close_all_queue = deque(document.documentId for document in self._model.documents())
        self._advance_close_all()

    @Slot()
    def undo(self) -> None:
        doc = self._active_document()
        if doc is not None:
            doc.undo()

    @Slot()
    def redo(self) -> None:
        doc = self._active_document()
        if doc is not None:
            doc.redo()

    @Slot(int)
    @Slot(int, int)
    def gotoLine(self, line: int, column: int = 1) -> None:  # noqa: N802
        doc = self._active_document()
        if doc is None:
            return
        line = min(max(1, int(line)), max(1, doc.lineCount))
        column = max(1, int(column))
        doc.set_cursor(line, column)
        self.gotoLineRequested.emit(doc.documentId, line, column)
        self.activeChanged.emit()

    @Slot(str, object)
    def attachTextDocument(self, document_id: str, quick_doc) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is None or quick_doc is None:
            return
        text_document = getattr(quick_doc, "textDocument", None)
        if callable(text_document):
            doc.attach(text_document())

    @Slot(str, int, int)
    def reportCursor(self, document_id: str, line: int, column: int) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is None:
            return
        doc.set_cursor(line, column)
        if document_id == self._active_id:
            self.activeChanged.emit()

    @Slot(str, "QVariantMap")
    def reportSelection(self, document_id: str, selection: dict) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is None or document_id != self._active_id:
            return
        self._selection = {
            "path": doc.path,
            "startLine": max(1, int(selection.get("startLine") or 1)),
            "startColumn": max(1, int(selection.get("startColumn") or 1)),
            "endLine": max(1, int(selection.get("endLine") or 1)),
            "endColumn": max(1, int(selection.get("endColumn") or 1)),
            "hasSelection": bool(selection.get("hasSelection", False)),
            "text": str(selection.get("text") or "")[:8192],
            "textTruncated": len(str(selection.get("text") or "")) > 8192,
        }
        self.currentSelectionChanged.emit()

    @Slot(str, "QVariantList")
    def setGutterDecorations(self, document_id: str, decorations: list) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is not None:
            doc.set_gutter_decorations(decorations)

    @Slot("QVariantList")
    def setBaselineChangedPaths(self, paths: list) -> None:  # noqa: N802
        wanted = {_path_key(str(path)) for path in paths}
        for doc in self._model.documents():
            doc.set_baseline_changed(_path_key(doc.path) in wanted)

    @Slot(str, str)
    def setDocumentText(self, document_id: str, text: str) -> None:  # noqa: N802
        doc = self._by_id.get(document_id)
        if doc is not None:
            doc.setText(text)
            self._search.refresh()

    def _on_opened(self, document_id: str, result: FileReadResult, line: int) -> None:
        doc = self._by_id.get(document_id)
        if doc is None:
            return
        doc.load(result)
        self._update_titles()
        self._model.notify_document(doc)
        if document_id == self._active_id:
            self.activeChanged.emit()
        if line >= 1:
            self.activate(document_id)
            # 打开方传入的目标行交给 Pane 处理；Pane 可在异步布局完成后自行夹取。
            doc.set_cursor(min(line, max(1, doc.lineCount)), 1)
            self.gotoLineRequested.emit(document_id, line, 1)
            self.activeChanged.emit()

    def _on_open_error(self, document_id: str, path: str, error: dict) -> None:
        self._close_document(document_id)
        self.openFailed.emit(path, str(error.get("code") or "IPC_ERROR"), str(error.get("message") or "无法打开文件"))

    def _on_saved(self, document_id: str, result: FileWriteResult) -> None:
        doc = self._by_id.get(document_id)
        if doc is None:
            return
        doc.mark_saved(result.sha256, result.mtime)
        self._model.notify_document(doc)
        self.documentSaved.emit(doc.documentId, doc.path, result.sha256)
        if document_id in self._pending_close:
            self._pending_close.remove(document_id)
            self._close_document(document_id)
            self._advance_close_all()
        elif document_id == self._active_id:
            self.activeChanged.emit()

    def _on_save_error(self, document_id: str, error: dict) -> None:
        doc = self._by_id.get(document_id)
        if doc is None:
            return
        code = str(error.get("code") or "IPC_ERROR")
        message = str(error.get("message") or "保存失败")
        if code == "FS_CONFLICT":
            doc.set_conflict_hash(str((error.get("details") or {}).get("current_hash") or ""))
            doc.set_state("conflict")
            self.conflictDetected.emit(doc.documentId, doc.path)
            return
        doc.set_state("ready")
        self.saveFailed.emit(doc.documentId, code, message)

    def _on_reloaded(self, document_id: str, result: FileReadResult) -> None:
        doc = self._by_id.get(document_id)
        if doc is not None:
            doc.load(result)
            self._model.notify_document(doc)

    def _on_manual_edit(self, envelope: dict) -> None:
        payload = (envelope or {}).get("payload") or {}
        raw_paths = payload.get("files") or ([] if not payload.get("path") else [payload.get("path")])
        paths = [_clean_path(str(path)) for path in raw_paths if path]
        if not paths:
            self.manualEditMarked.emit([])
            return
        marked: list[str] = []
        wanted = {_path_key(path) for path in paths}
        for doc in self._model.documents():
            if _path_key(doc.path) in wanted:
                doc.set_manual_edit_badge(True)
                marked.append(doc.path)
        self.manualEditMarked.emit(marked)

    def _on_task_state(self, envelope: dict) -> None:
        state = str(((envelope or {}).get("payload") or {}).get("state") or "")
        if state in self._TERMINAL_STATES:
            for doc in self._model.documents():
                doc.set_manual_edit_badge(False)

    def _on_workspace_changed(self, envelope: dict) -> None:
        self._force_clear()

    def _on_disconnected(self, _envelope: dict) -> None:
        for doc in self._model.documents():
            doc.set_read_only(True)

    def _force_clear(self) -> None:
        for doc in list(self._model.documents()):
            self._close_document(doc.documentId)

    def _close_document(self, document_id: str) -> None:
        doc = self._by_id.pop(document_id, None)
        if doc is None:
            return
        self._model.remove(doc)
        self._by_path.pop(_path_key(doc.path), None)
        try:
            self._mru.remove(document_id)
        except ValueError:
            pass
        if document_id == self._active_id:
            self._active_id = self._mru[0] if self._mru else ""
            self._selection = _empty_selection(self._active_document().path if self._active_document() else "")
            self.activeChanged.emit()
            self.currentSelectionChanged.emit()
        doc.deleteLater()

    def _advance_close_all(self) -> None:
        if self._close_all_queue is None:
            return
        while self._close_all_queue:
            document_id = self._close_all_queue.popleft()
            doc = self._by_id.get(document_id)
            if doc is None:
                continue
            if doc.dirty:
                self.closeConfirmationRequested.emit(doc.documentId, doc.title)
                return
            self._close_document(document_id)
        self._close_all_queue = None
        self.allCloseFinished.emit(True)

    def _cancel_close_all_if_needed(self) -> None:
        if self._close_all_queue is not None:
            self._close_all_queue = None
            self.allCloseFinished.emit(False)

    def _cycle_tab(self, direction: int) -> None:
        if len(self._mru) < 2:
            return
        ordered = list(self._mru)
        current = ordered.index(self._active_id) if self._active_id in ordered else 0
        self.activate(ordered[(current + direction) % len(ordered)])

    def _update_titles(self) -> None:
        docs = self._model.documents()
        by_name: dict[str, list[EditorDocument]] = {}
        for doc in docs:
            by_name.setdefault(doc.fileName, []).append(doc)
        for name, same_name_docs in by_name.items():
            if len(same_name_docs) == 1:
                same_name_docs[0].set_title(name)
                continue
            for doc in same_name_docs:
                parent = doc.path.rsplit("/", 1)[0] if "/" in doc.path else ""
                doc.set_title(f"{name} - {parent}" if parent else name)

    def _on_search_match(self, start: int, _length: int) -> None:
        doc = self._active_document()
        if doc is None:
            return
        block = doc.text[:start].count("\n") + 1
        column = start - doc.text.rfind("\n", 0, start)
        self.gotoLine(block, column)

    def _active_document(self) -> EditorDocument | None:
        return self._by_id.get(self._active_id)

    def _get_documents(self) -> QObject:
        return self._model

    def _get_active_document_id(self) -> str:
        return self._active_id

    def _get_active_document(self) -> QObject | None:
        return self._active_document()

    def _get_open_count(self) -> int:
        return len(self._by_id)

    def _get_search(self) -> QObject:
        return self._search

    def _get_current_selection(self) -> dict:
        return dict(self._selection)

    def _get_active_file_path(self) -> str:
        doc = self._active_document()
        return doc.path if doc else ""

    def _get_cursor_line(self) -> int:
        doc = self._active_document()
        return doc.cursorLine if doc else -1

    def _get_cursor_column(self) -> int:
        doc = self._active_document()
        return doc.cursorColumn if doc else -1

    def _get_active_dirty(self) -> bool:
        doc = self._active_document()
        return bool(doc and doc.dirty)

    def _get_active_read_only(self) -> bool:
        doc = self._active_document()
        return bool(doc and doc.readOnly)

    documents = Property(QObject, _get_documents, constant=True)
    activeDocumentId = Property(str, _get_active_document_id, notify=activeChanged)
    activeDocument = Property(QObject, _get_active_document, notify=activeChanged)
    openCount = Property(int, _get_open_count, notify=activeChanged)
    search = Property(QObject, _get_search, constant=True)
    currentSelection = Property("QVariantMap", _get_current_selection, notify=currentSelectionChanged)
    activeFilePath = Property(str, _get_active_file_path, notify=activeChanged)
    cursorLine = Property(int, _get_cursor_line, notify=activeChanged)
    cursorColumn = Property(int, _get_cursor_column, notify=activeChanged)
    activeDirty = Property(bool, _get_active_dirty, notify=activeChanged)
    activeReadOnly = Property(bool, _get_active_read_only, notify=activeChanged)
    documentCount = Property(int, _get_open_count, notify=activeChanged)


def _clean_path(path: str) -> str:
    return str(path or "").replace("\\", "/").strip("/")


def _path_key(path: str) -> str:
    return _clean_path(path).casefold()


def _empty_selection(path: str = "") -> dict:
    return {
        "path": path,
        "startLine": 1,
        "startColumn": 1,
        "endLine": 1,
        "endColumn": 1,
        "hasSelection": False,
        "text": "",
        "textTruncated": False,
    }
