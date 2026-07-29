"""Explorer 的惰性文件树与 QML 门面。

文件系统请求始终经 :class:`halo_studio.ipc.fs_client.FsClient` 转发到
Sidecar。这里保存的是 UI 树缓存，不读取或写入本地工作区。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import PurePosixPath
from typing import Iterable

from PySide6.QtCore import (
    Property,
    QAbstractListModel,
    QModelIndex,
    QObject,
    QTimer,
    Qt,
    QUrl,
    Signal,
    Slot,
)
from PySide6.QtGui import QDesktopServices, QGuiApplication

from halo_studio.ipc.fs_client import FsClient, FsEntry, FsListResult

from .base import BaseViewModel


@dataclass
class Decoration:
    """由差异化功能写入的文件级装饰。"""

    letter: str
    color_token: str
    tooltip: str
    bubble: bool = False


@dataclass
class FsNode:
    """树的内部节点；QML 只消费 ``FsTreeModel`` 的扁平投影。"""

    path: str
    name: str
    kind: str
    size: int = 0
    mtime: str = ""
    readonly: bool = False
    expanded: bool = False
    children_loaded: bool = False
    loading: bool = False
    truncated: bool = False
    children: list["FsNode"] = field(default_factory=list)


class FsTreeModel(QAbstractListModel):
    """把惰性目录树映射为 QML ``ListView`` 能消费的扁平行。"""

    NameRole = int(Qt.ItemDataRole.UserRole) + 1
    RelPathRole = NameRole + 1
    KindRole = NameRole + 2
    LevelRole = NameRole + 3
    ExpandedRole = NameRole + 4
    LoadingRole = NameRole + 5
    IsEditingRole = NameRole + 6
    BadgeLetterRole = NameRole + 7
    BadgeColorTokenRole = NameRole + 8
    BadgeTooltipRole = NameRole + 9
    TruncatedRole = NameRole + 10
    ReadonlyRole = NameRole + 11

    _ROLE_NAMES = {
        NameRole: b"name",
        RelPathRole: b"relPath",
        KindRole: b"kind",
        LevelRole: b"level",
        ExpandedRole: b"expanded",
        LoadingRole: b"loading",
        IsEditingRole: b"isEditing",
        BadgeLetterRole: b"badgeLetter",
        BadgeColorTokenRole: b"badgeColorToken",
        BadgeTooltipRole: b"badgeTooltip",
        TruncatedRole: b"truncated",
        ReadonlyRole: b"readonly",
    }

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._root = FsNode(path="", name="", kind="dir", expanded=True)
        self._nodes: dict[str, FsNode] = {"": self._root}
        self._visible: list[tuple[FsNode, int]] = []
        self._decorations: dict[str, Decoration] = {}

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._visible)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._visible)):
            return None
        node, level = self._visible[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return node.name
        if role == self.NameRole:
            return node.name
        if role == self.RelPathRole:
            return node.path
        if role == self.KindRole:
            return node.kind
        if role == self.LevelRole:
            return level
        if role == self.ExpandedRole:
            return node.expanded
        if role == self.LoadingRole:
            return node.loading
        if role == self.IsEditingRole:
            return False
        if role == self.TruncatedRole:
            return node.truncated
        if role == self.ReadonlyRole:
            return node.readonly
        decoration = self._decoration_for(node)
        if role == self.BadgeLetterRole:
            return decoration.letter if decoration else ""
        if role == self.BadgeColorTokenRole:
            return decoration.color_token if decoration else ""
        if role == self.BadgeTooltipRole:
            return decoration.tooltip if decoration else ""
        return None

    def roleNames(self):  # noqa: N802
        return dict(self._ROLE_NAMES)

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if not 0 <= row < len(self._visible):
            return {}
        node, level = self._visible[row]
        decoration = self._decoration_for(node)
        return {
            "name": node.name,
            "relPath": node.path,
            "kind": node.kind,
            "level": level,
            "expanded": node.expanded,
            "loading": node.loading,
            "isEditing": False,
            "truncated": node.truncated,
            "readonly": node.readonly,
            "badgeLetter": decoration.letter if decoration else "",
            "badgeColorToken": decoration.color_token if decoration else "",
            "badgeTooltip": decoration.tooltip if decoration else "",
        }

    def visible_rows(self) -> list[FsNode]:
        """返回当前可见节点的快照，供控制器决定刷新范围。"""
        return [node for node, _level in self._visible]

    def node(self, path: str) -> FsNode | None:
        return self._nodes.get(_clean_path(path))

    def apply_listing(
        self,
        dir_path: str,
        entries: Iterable[FsEntry] | FsListResult,
        truncated: bool | None = None,
    ) -> None:
        """用一层 ``fs.list`` 结果更新缓存，同时保留已展开子树。"""
        path = _clean_path(dir_path)
        if isinstance(entries, FsListResult):
            truncated = entries.truncated
            entries = entries.entries
        parent = self._nodes.get(path)
        if parent is None:
            parent_name = PurePosixPath(path).name if path else ""
            parent = FsNode(path=path, name=parent_name, kind="dir")
            self._nodes[path] = parent

        previous = {child.path: child for child in parent.children}
        updated: list[FsNode] = []
        next_paths: set[str] = set()
        for entry in entries:
            child_path = _clean_path(entry.path)
            if not child_path:
                continue
            node = previous.get(child_path) or self._nodes.get(child_path)
            if node is None:
                node = FsNode(path=child_path, name=entry.name, kind=entry.kind)
                self._nodes[child_path] = node
            node.name = entry.name or PurePosixPath(child_path).name
            node.kind = entry.kind or "file"
            node.size = entry.size
            node.mtime = entry.mtime
            node.readonly = entry.readonly
            updated.append(node)
            next_paths.add(child_path)

        for child in parent.children:
            if child.path not in next_paths:
                self._forget_subtree(child)

        parent.children = sorted(updated, key=_node_sort_key)
        parent.children_loaded = True
        parent.loading = False
        parent.truncated = bool(truncated)
        self._rebuild_visible()

    def clear(self) -> None:
        self.beginResetModel()
        self._root = FsNode(path="", name="", kind="dir", expanded=True)
        self._nodes = {"": self._root}
        self._visible = []
        self.endResetModel()

    def set_expanded(self, path: str, expanded: bool) -> None:
        node = self.node(path)
        if node is None or node.kind != "dir" or node.expanded == expanded:
            return
        node.expanded = expanded
        self._rebuild_visible()

    def set_loading(self, path: str, loading: bool) -> None:
        node = self.node(path)
        if node is None or node.loading == loading:
            return
        node.loading = loading
        self._emit_node_changed(node, [self.LoadingRole])

    def insert_entry(self, parent_path: str, entry: FsEntry) -> None:
        parent = self.node(parent_path)
        if parent is None or parent.kind != "dir":
            return
        existing = self.node(entry.path)
        node = existing or FsNode(path=_clean_path(entry.path), name=entry.name, kind=entry.kind)
        node.name = entry.name or PurePosixPath(node.path).name
        node.kind = entry.kind or "file"
        node.size = entry.size
        node.mtime = entry.mtime
        node.readonly = entry.readonly
        self._nodes[node.path] = node
        parent.children = [child for child in parent.children if child.path != node.path]
        parent.children.append(node)
        parent.children.sort(key=_node_sort_key)
        parent.children_loaded = True
        self._rebuild_visible()

    def rename_entry(self, old_path: str, entry: FsEntry) -> None:
        old = self.node(old_path)
        parent_path = _parent_path(old_path)
        if old is not None:
            parent = self.node(parent_path)
            if parent is not None:
                parent.children = [child for child in parent.children if child is not old]
            self._forget_subtree(old)
        self.insert_entry(_parent_path(entry.path), entry)

    def set_decorations(self, decorations: dict[str, Decoration]) -> None:
        self._decorations = {_clean_path(path): decoration for path, decoration in decorations.items()}
        if self._visible:
            top = self.index(0, 0)
            bottom = self.index(len(self._visible) - 1, 0)
            self.dataChanged.emit(
                top,
                bottom,
                [self.BadgeLetterRole, self.BadgeColorTokenRole, self.BadgeTooltipRole],
            )

    def _rebuild_visible(self) -> None:
        rows: list[tuple[FsNode, int]] = []

        def append_children(parent: FsNode, level: int) -> None:
            for child in parent.children:
                rows.append((child, level))
                if child.kind == "dir" and child.expanded:
                    append_children(child, level + 1)

        append_children(self._root, 0)
        self.beginResetModel()
        self._visible = rows
        self.endResetModel()

    def _forget_subtree(self, node: FsNode) -> None:
        for child in list(node.children):
            self._forget_subtree(child)
        self._nodes.pop(node.path, None)

    def _emit_node_changed(self, node: FsNode, roles: list[int]) -> None:
        for row, (visible, _level) in enumerate(self._visible):
            if visible is node:
                index = self.index(row, 0)
                self.dataChanged.emit(index, index, roles)
                return

    def _decoration_for(self, node: FsNode) -> Decoration | None:
        direct = self._decorations.get(node.path)
        if direct is not None:
            return direct
        if node.kind != "dir" or node.expanded:
            return None
        prefix = f"{node.path}/" if node.path else ""
        for path in sorted(self._decorations):
            decoration = self._decorations[path]
            if decoration.bubble and path.startswith(prefix):
                return decoration
        return None


class ExplorerViewModel(BaseViewModel):
    """资源管理器的 QML 门面，控制器只持有 IPC 和 UI 树缓存。"""

    workspaceChanged = Signal()
    autoRefreshChanged = Signal()
    errorOccurred = Signal(str, str)

    _TERMINAL_TASK_STATES = {"review_ready", "cancelled", "failed", "interrupted"}

    def __init__(self, client, editor=None, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._fs = FsClient(client)
        self._model = FsTreeModel(self)
        self._editor = editor
        self._workspace_active = False
        self._workspace_trusted = False
        self._workspace_path = ""
        self._auto_refresh_enabled = False
        self._auto_refresh_interval_ms = 30_000
        self._refresh_timer = QTimer(self)
        self._refresh_timer.setInterval(self._auto_refresh_interval_ms)
        self._refresh_timer.timeout.connect(self._refresh_if_foreground)
        client.subscribe("workspace.changed", self._on_workspace_changed)
        client.subscribe("task.state", self._on_task_state)

    def set_editor(self, editor) -> None:
        """装配期注入编辑器服务，避免 Explorer 反向构造编辑器。"""
        self._editor = editor

    def set_workspace_state(self, active: bool, trust: str, real_path: str = "") -> None:
        """由装配层同步 WorkspaceViewModel 的已经确认状态。"""
        was_usable = self._is_usable
        self._workspace_active = bool(active)
        self._workspace_trusted = self._workspace_active and trust == "trusted"
        self._workspace_path = str(real_path or "")
        self.workspaceChanged.emit()
        if self._is_usable and not was_usable:
            self._model.clear()
            self.refresh()
        elif not self._is_usable:
            self._model.clear()

    @Slot(str)
    def expand(self, rel_path: str) -> None:
        if not self._ensure_workspace():
            return
        path = _clean_path(rel_path)
        node = self._model.node(path)
        if node is None or node.kind != "dir":
            return
        if node.children_loaded:
            self._model.set_expanded(path, True)
            return
        self._model.set_expanded(path, True)
        self._model.set_loading(path, True)
        self._fs.list(
            path,
            1,
            lambda result: self._model.apply_listing(path, result.entries, result.truncated),
            lambda error: self._on_listing_error(path, error),
        )

    @Slot(str)
    def collapse(self, rel_path: str) -> None:
        self._model.set_expanded(_clean_path(rel_path), False)

    @Slot()
    def refresh(self) -> None:
        if not self._ensure_workspace():
            return
        paths = [""]
        paths.extend(
            node.path
            for node in self._model.visible_rows()
            if node.kind == "dir" and node.expanded
        )
        for path in dict.fromkeys(paths):
            self._model.set_loading(path, True)
            self._fs.list(
                path,
                1,
                lambda result, listed_path=path: self._model.apply_listing(
                    listed_path, result.entries, result.truncated
                ),
                lambda error, listed_path=path: self._on_listing_error(listed_path, error),
            )

    @Slot(str, str)
    def createFile(self, parent_dir: str, name: str) -> None:  # noqa: N802
        self._create(parent_dir, name, is_dir=False)

    @Slot(str, str)
    def createDir(self, parent_dir: str, name: str) -> None:  # noqa: N802
        self._create(parent_dir, name, is_dir=True)

    @Slot(str, str)
    def rename(self, rel_path: str, new_name: str) -> None:
        if not self._ensure_workspace() or not self._validate_or_error(new_name):
            return
        old_path = _clean_path(rel_path)
        new_path = _join_path(_parent_path(old_path), new_name)
        self._fs.rename(
            old_path,
            new_path,
            lambda entry: self._model.rename_entry(old_path, entry),
            self._report_error,
        )

    @Slot(str)
    def openPreview(self, rel_path: str) -> None:  # noqa: N802
        self._open_editor(_clean_path(rel_path), pinned=False)

    @Slot(str)
    def openPinned(self, rel_path: str) -> None:  # noqa: N802
        self._open_editor(_clean_path(rel_path), pinned=True)

    @Slot(str)
    def revealInSystem(self, rel_path: str) -> None:  # noqa: N802
        path = PurePosixPath(_clean_path(rel_path)).parent.as_posix()
        local_path = self._workspace_path
        if path and path != ".":
            local_path = f"{local_path}/{path}" if local_path else path
        if local_path:
            QDesktopServices.openUrl(QUrl.fromLocalFile(local_path))

    @Slot(str, result=str)
    def validateName(self, name: str) -> str:  # noqa: N802
        return _validate_name(name)

    @Slot(bool)
    def setAutoRefreshEnabled(self, enabled: bool) -> None:  # noqa: N802
        enabled = bool(enabled)
        if enabled == self._auto_refresh_enabled:
            return
        self._auto_refresh_enabled = enabled
        if enabled:
            self._refresh_timer.start()
        else:
            self._refresh_timer.stop()
        self.autoRefreshChanged.emit()

    @Slot(int)
    def setAutoRefreshIntervalMs(self, interval_ms: int) -> None:  # noqa: N802
        interval_ms = max(1_000, int(interval_ms))
        if interval_ms == self._auto_refresh_interval_ms:
            return
        self._auto_refresh_interval_ms = interval_ms
        self._refresh_timer.setInterval(interval_ms)
        self.autoRefreshChanged.emit()

    def _create(self, parent_dir: str, name: str, is_dir: bool) -> None:
        if not self._ensure_workspace() or not self._validate_or_error(name):
            return
        parent_path = _clean_path(parent_dir)
        path = _join_path(parent_path, name)

        def created(entry: FsEntry) -> None:
            self._model.insert_entry(parent_path, entry)
            if not is_dir:
                self._open_editor(entry.path, pinned=False)

        if is_dir:
            self._fs.create_dir(path, created, self._report_error)
        else:
            self._fs.create_file(path, "", created, self._report_error)

    def _open_editor(self, path: str, pinned: bool) -> None:
        if not path or self._editor is None:
            return
        callback = getattr(self._editor, "openFile", None)
        if callback is None:
            callback = getattr(self._editor, "open_file", None)
        if callback is None:
            return
        try:
            callback(path, pinned=pinned)
        except TypeError:
            callback(path)

    def _on_workspace_changed(self, envelope: dict) -> None:
        payload = (envelope or {}).get("payload") or {}
        self.set_workspace_state(
            bool(payload.get("active", False)),
            str(payload.get("trust") or "untrusted"),
            str(payload.get("real_path") or ""),
        )

    def _on_task_state(self, envelope: dict) -> None:
        payload = (envelope or {}).get("payload") or {}
        if str(payload.get("state") or "") in self._TERMINAL_TASK_STATES:
            self.refresh()

    def _on_listing_error(self, path: str, error: dict) -> None:
        self._model.set_loading(path, False)
        self._report_error(error)

    def _report_error(self, error: dict) -> None:
        self._set_error(error or {})
        self.errorOccurred.emit(self.errorCode, self.errorMessage)

    def _validate_or_error(self, name: str) -> bool:
        message = _validate_name(name)
        if not message:
            return True
        self._report_error({"code": "FS_INVALID_NAME", "message": message, "details": {}})
        return False

    def _ensure_workspace(self) -> bool:
        if self._is_usable:
            return True
        if self._workspace_active:
            self._report_error({"code": "WORKSPACE_NOT_TRUSTED", "message": "需要信任工作区后才能浏览文件", "details": {}})
        return False

    def _refresh_if_foreground(self) -> None:
        app = QGuiApplication.instance()
        if app is not None and app.applicationState() != Qt.ApplicationState.ApplicationActive:
            return
        self.refresh()

    @property
    def _is_usable(self) -> bool:
        return self._workspace_active and self._workspace_trusted

    def _get_model(self) -> QObject:
        return self._model

    def _get_workspace_active(self) -> bool:
        return self._workspace_active

    def _get_workspace_trusted(self) -> bool:
        return self._workspace_trusted

    def _get_auto_refresh_enabled(self) -> bool:
        return self._auto_refresh_enabled

    def _get_auto_refresh_interval_ms(self) -> int:
        return self._auto_refresh_interval_ms

    model = Property(QObject, _get_model, constant=True)
    workspaceActive = Property(bool, _get_workspace_active, notify=workspaceChanged)
    workspaceTrusted = Property(bool, _get_workspace_trusted, notify=workspaceChanged)
    autoRefreshEnabled = Property(bool, _get_auto_refresh_enabled, notify=autoRefreshChanged)
    autoRefreshIntervalMs = Property(int, _get_auto_refresh_interval_ms, notify=autoRefreshChanged)


def _clean_path(path: str) -> str:
    return str(path or "").replace("\\", "/").strip("/")


def _join_path(parent: str, name: str) -> str:
    return f"{parent}/{name}" if parent else name


def _parent_path(path: str) -> str:
    parent = PurePosixPath(_clean_path(path)).parent if path else None
    if parent is None or parent == PurePosixPath("."):
        return ""
    return _clean_path(str(parent))


def _node_sort_key(node: FsNode) -> tuple[int, str]:
    return (0 if node.kind == "dir" else 1, node.name.casefold())


def _validate_name(name: str) -> str:
    value = str(name or "")
    if not value.strip():
        return "名称不能为空"
    if "/" in value or "\\" in value:
        return "名称不能包含路径分隔符"
    if "\x00" in value:
        return "名称不能包含空字符"
    if value.endswith((" ", ".")):
        return "名称不能以空格或点结尾"
    stem = value.split(".", 1)[0].upper()
    reserved = {"CON", "PRN", "AUX", "NUL"}
    reserved.update(f"COM{number}" for number in range(1, 10))
    reserved.update(f"LPT{number}" for number in range(1, 10))
    if stem in reserved:
        return "名称不能使用 Windows 保留名"
    return ""
