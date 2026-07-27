"""命令注册表：菜单、面板和快捷键的单一事实来源。"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass
from typing import Callable

from PySide6.QtCore import Property, QAbstractListModel, QModelIndex, QObject, Qt, Signal, Slot

from .when_context import WhenContext

_LOG = logging.getLogger(__name__)
_AREAS = {"app", "view", "palette", "workspace", "editor", "task", "review", "handoff", "config", "history"}
_ID_RE = re.compile(r"^(?P<area>[a-z]+)\.(?P<name>[A-Za-z][A-Za-z0-9]*)$")


@dataclass(frozen=True)
class Command:
    id: str
    title: str
    category: str
    callback: Callable[[], None]
    shortcut: str | None = None
    when: str | None = None


class CommandListModel(QAbstractListModel):
    CommandIdRole = int(Qt.ItemDataRole.UserRole) + 1
    TitleRole = CommandIdRole + 1
    CategoryRole = CommandIdRole + 2
    ShortcutRole = CommandIdRole + 3
    EnabledRole = CommandIdRole + 4

    _ROLE_NAMES = {
        CommandIdRole: b"commandId",
        TitleRole: b"title",
        CategoryRole: b"category",
        ShortcutRole: b"shortcut",
        EnabledRole: b"enabled",
    }

    def __init__(self, context: WhenContext, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._context = context
        self._commands: list[Command] = []
        context.changed.connect(self.refresh_enabled)

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._commands)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._commands)):
            return None
        command = self._commands[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return command.title
        values = {
            self.CommandIdRole: command.id,
            self.TitleRole: command.title,
            self.CategoryRole: command.category,
            self.ShortcutRole: command.shortcut or "",
            self.EnabledRole: self._context.evaluate(command.when),
        }
        return values.get(role)

    def roleNames(self):  # noqa: N802
        return dict(self._ROLE_NAMES)

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if not 0 <= row < len(self._commands):
            return {}
        command = self._commands[row]
        return {
            "commandId": command.id,
            "title": command.title,
            "category": command.category,
            "shortcut": command.shortcut or "",
            "enabled": self._context.evaluate(command.when),
        }

    def commands(self) -> list[Command]:
        return list(self._commands)

    def insert(self, command: Command) -> None:
        index = next(
            (row for row, existing in enumerate(self._commands) if _sort_key(command) < _sort_key(existing)),
            len(self._commands),
        )
        self.beginInsertRows(QModelIndex(), index, index)
        self._commands.insert(index, command)
        self.endInsertRows()

    def remove(self, command_id: str) -> bool:
        for row, command in enumerate(self._commands):
            if command.id == command_id:
                self.beginRemoveRows(QModelIndex(), row, row)
                self._commands.pop(row)
                self.endRemoveRows()
                return True
        return False

    def refresh_enabled(self) -> None:
        if self._commands:
            self.dataChanged.emit(
                self.index(0, 0), self.index(len(self._commands) - 1, 0), [self.EnabledRole]
            )


class CommandRegistry(QObject):
    commandsChanged = Signal()
    commandExecuted = Signal(str)
    executeFailed = Signal(str, str)

    def __init__(self, when_context: WhenContext, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._context = when_context
        self._model = CommandListModel(when_context, self)
        self._by_id: dict[str, Command] = {}
        self._shortcuts: dict[str, str] = {}

    def register(
        self,
        id: str,
        title: str,
        category: str,
        callback: Callable[[], None],
        shortcut: str | None = None,
        when: str | None = None,
    ) -> bool:
        _validate_id(id)
        if id in self._by_id:
            _LOG.warning("命令已注册，忽略重复定义：%s", id)
            return False
        normalized_shortcut = shortcut.strip() if shortcut else None
        if normalized_shortcut:
            key = normalized_shortcut.casefold()
            if key in self._shortcuts:
                _LOG.warning("快捷键 %s 已由 %s 占用", normalized_shortcut, self._shortcuts[key])
                normalized_shortcut = None
            else:
                self._shortcuts[key] = id
        command = Command(id, str(title), str(category), callback, normalized_shortcut, when)
        self._by_id[id] = command
        self._model.insert(command)
        self.commandsChanged.emit()
        return True

    def unregister(self, id: str) -> bool:
        command = self._by_id.pop(id, None)
        if command is None:
            return False
        if command.shortcut:
            self._shortcuts.pop(command.shortcut.casefold(), None)
        self._model.remove(id)
        self.commandsChanged.emit()
        return True

    @Slot(str, result=bool)
    def execute(self, id: str) -> bool:
        command = self._by_id.get(id)
        if command is None:
            self.executeFailed.emit(id, "命令不存在")
            return False
        if not self._context.evaluate(command.when):
            self.executeFailed.emit(id, "当前状态下不可用")
            return False
        try:
            command.callback()
        except Exception as exc:  # noqa: BLE001 - 命令异常不能终止 Qt 主循环
            _LOG.exception("执行命令失败：%s", id)
            self.executeFailed.emit(id, str(exc) or "命令执行失败")
            return False
        self.commandExecuted.emit(id)
        return True

    def get(self, id: str) -> Command | None:
        return self._by_id.get(id)

    def is_enabled(self, id: str) -> bool:
        command = self._by_id.get(id)
        return bool(command and self._context.evaluate(command.when))

    def snapshot(self) -> list[Command]:
        return self._model.commands()

    commands = Property(QObject, lambda self: self._model, constant=True)


def _validate_id(command_id: str) -> None:
    match = _ID_RE.fullmatch(command_id)
    if match is None or match.group("area") not in _AREAS:
        raise ValueError(f"非法命令 id：{command_id}")


def _sort_key(command: Command) -> tuple[str, str, str]:
    return (command.category, command.title, command.id)
