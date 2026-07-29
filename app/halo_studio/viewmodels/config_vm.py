"""ConfigViewModel：受管启动配置列表 / 保存 / 删除 / 凭据引用检查。

凭据红线：本视图模型只处理凭据引用名（credential_ref），字段一律白名单过滤，
任何形如密钥明文的字段在进入请求或本地模型之前即被丢弃。
"""

from __future__ import annotations

from PySide6.QtCore import (
    Property,
    QAbstractListModel,
    QModelIndex,
    QObject,
    Qt,
    Signal,
    Slot,
)

from .base import BaseViewModel

# `config.save` 只创建新配置，绝不能把列表记录的 config_id 再发送回 IPC。
# 白名单以外的键一律丢弃，从构造上保证视图模型永不持有、也永不发送“密钥明文”字段。
_SAVE_FIELDS = (
    "name",
    "agent",
    "executable_path",
    "model",
    "thinking_level",
    "credential_ref",
)
_RECORD_FIELDS = ("config_id",) + _SAVE_FIELDS + ("created_at", "updated_at")


def _filter_record(config: dict) -> dict:
    return {k: config[k] for k in _RECORD_FIELDS if k in config}


class ConfigListModel(QAbstractListModel):
    ConfigIdRole = int(Qt.ItemDataRole.UserRole) + 1
    NameRole = ConfigIdRole + 1
    AgentRole = ConfigIdRole + 2
    ExecutablePathRole = ConfigIdRole + 3
    ModelRole = ConfigIdRole + 4
    ThinkingLevelRole = ConfigIdRole + 5
    CredentialRefRole = ConfigIdRole + 6
    CreatedAtRole = ConfigIdRole + 7
    UpdatedAtRole = ConfigIdRole + 8

    _ROLE_KEYS = {
        ConfigIdRole: "config_id",
        NameRole: "name",
        AgentRole: "agent",
        ExecutablePathRole: "executable_path",
        ModelRole: "model",
        ThinkingLevelRole: "thinking_level",
        CredentialRefRole: "credential_ref",
        CreatedAtRole: "created_at",
        UpdatedAtRole: "updated_at",
    }

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._configs: list[dict] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._configs)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._configs)):
            return None
        cfg = self._configs[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return cfg.get("name")
        key = self._ROLE_KEYS.get(role)
        return cfg.get(key) if key else None

    def roleNames(self):  # noqa: N802
        return {
            self.ConfigIdRole: b"configId",
            self.NameRole: b"name",
            self.AgentRole: b"agent",
            self.ExecutablePathRole: b"executablePath",
            self.ModelRole: b"model",
            self.ThinkingLevelRole: b"thinkingLevel",
            self.CredentialRefRole: b"credentialRef",
            self.CreatedAtRole: b"createdAt",
            self.UpdatedAtRole: b"updatedAt",
        }

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if 0 <= row < len(self._configs):
            return dict(self._configs[row])
        return {}

    # ---- 由 ConfigViewModel 调用 ----

    def reset_with(self, configs: list[dict]) -> None:
        self.beginResetModel()
        self._configs = [_filter_record(c) for c in configs]
        self.endResetModel()

    def upsert(self, config: dict) -> None:
        config = _filter_record(config)
        cid = config.get("config_id")
        for row, existing in enumerate(self._configs):
            if existing.get("config_id") == cid:
                self._configs[row] = config
                idx = self.index(row, 0)
                self.dataChanged.emit(idx, idx)
                return
        row = len(self._configs)
        self.beginInsertRows(QModelIndex(), row, row)
        self._configs.append(config)
        self.endInsertRows()

    def remove(self, config_id: str) -> None:
        for row, existing in enumerate(self._configs):
            if existing.get("config_id") == config_id:
                self.beginRemoveRows(QModelIndex(), row, row)
                del self._configs[row]
                self.endRemoveRows()
                return


class ConfigViewModel(BaseViewModel):
    credentialCheckChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._model = ConfigListModel(self)
        self._credential_checked_ref = ""
        self._credential_exists = False
        self._credential_store_available = False

    # ---- 命令 ----

    @Slot()
    def refresh(self) -> None:
        self._clear_error()
        self._client.request("config.list", {}, self._on_list_ok, self._set_error)

    @Slot("QVariantMap")
    def save(self, config: dict) -> None:
        self._clear_error()
        payload = {k: config[k] for k in _SAVE_FIELDS if k in config}
        self._client.request("config.save", payload, self._on_save_ok, self._set_error)

    @Slot(str)
    def delete(self, config_id: str) -> None:
        self._clear_error()
        self._client.request(
            "config.delete",
            {"config_id": config_id},
            lambda _result, cid=config_id: self._model.remove(cid),
            self._set_error,
        )

    @Slot(str)
    def credentialCheck(self, credential_ref: str) -> None:  # noqa: N802
        self._clear_error()

        def on_ok(result: dict, ref: str = credential_ref) -> None:
            self._credential_checked_ref = ref
            self._credential_exists = bool(result.get("exists", False))
            self._credential_store_available = bool(result.get("store_available", False))
            self.credentialCheckChanged.emit()

        self._client.request(
            "config.credential_check", {"credential_ref": credential_ref}, on_ok, self._set_error
        )

    # ---- 回调 ----

    def _on_list_ok(self, result: dict) -> None:
        self._model.reset_with(list(result.get("configs") or []))

    def _on_save_ok(self, result: dict) -> None:
        config = result.get("config")
        if isinstance(config, dict):
            self._model.upsert(config)

    # ---- 属性 ----

    def _get_model(self) -> QObject:
        return self._model

    def _get_checked_ref(self) -> str:
        return self._credential_checked_ref

    def _get_credential_exists(self) -> bool:
        return self._credential_exists

    def _get_store_available(self) -> bool:
        return self._credential_store_available

    configs = Property(QObject, _get_model, constant=True)
    credentialCheckedRef = Property(str, _get_checked_ref, notify=credentialCheckChanged)
    credentialExists = Property(bool, _get_credential_exists, notify=credentialCheckChanged)
    credentialStoreAvailable = Property(bool, _get_store_available, notify=credentialCheckChanged)
