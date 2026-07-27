"""ReviewViewModel：交付审查（只读）。

只读红线：本视图模型不提供任何写文件 / 编辑 / 保存能力；accept/reject 只记录结论，
不触发任何 Git 操作或工作区回滚（由 Sidecar 契约保证，本层不越权）。
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


class ReviewFileListModel(QAbstractListModel):
    PathRole = int(Qt.ItemDataRole.UserRole) + 1
    ChangeRole = PathRole + 1
    DiffRole = PathRole + 2
    TruncatedRole = PathRole + 3

    _ROLE_KEYS = {
        PathRole: "path",
        ChangeRole: "change",
        DiffRole: "diff",
        TruncatedRole: "truncated",
    }

    def __init__(self, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._files: list[dict] = []

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._files)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._files)):
            return None
        entry = self._files[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return entry.get("path")
        key = self._ROLE_KEYS.get(role)
        return entry.get(key) if key else None

    def roleNames(self):  # noqa: N802
        return {
            self.PathRole: b"path",
            self.ChangeRole: b"change",
            self.DiffRole: b"diff",
            self.TruncatedRole: b"truncated",
        }

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if 0 <= row < len(self._files):
            return dict(self._files[row])
        return {}

    def reset_with(self, files: list[dict]) -> None:
        self.beginResetModel()
        self._files = [
            {
                "path": str(f.get("path") or ""),
                "change": str(f.get("change") or ""),
                "diff": str(f.get("diff") or ""),
                "truncated": bool(f.get("truncated", False)),
            }
            for f in files
        ]
        self.endResetModel()


class ReviewViewModel(BaseViewModel):
    bundleChanged = Signal()
    decisionChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(client, parent)
        self._files_model = ReviewFileListModel(self)
        self._reset_bundle_fields()
        self._decision_kind = ""
        self._decision_reason = ""
        self._decided_at = ""

    def _reset_bundle_fields(self) -> None:
        self._task_id = ""
        self._evidence_version = 0
        self._is_latest = False
        self._outcome = ""
        self._attribution = ""
        self._attribution_reasons: list = []
        self._summary = ""
        self._verification_status = ""
        self._verification_detail = ""
        self._verification_source = ""
        self._baseline_dirty_files: list = []

    # ---- 命令 ----

    @Slot(str)
    def load(self, task_id: str, version: int | None = None) -> None:
        self._clear_error()
        params: dict = {"task_id": task_id}
        if version is not None:
            params["version"] = int(version)
        self._client.request("review.get", params, self._on_bundle_ok, self._set_error)

    @Slot()
    def accept(self) -> None:
        self._clear_error()
        self._client.request(
            "delivery.accept",
            {"task_id": self._task_id, "evidence_version": self._evidence_version},
            self._on_decision_ok,
            self._set_error,
        )

    @Slot(str)
    def reject(self, reason: str = "") -> None:
        self._clear_error()
        self._client.request(
            "delivery.reject",
            {
                "task_id": self._task_id,
                "evidence_version": self._evidence_version,
                "reason": reason or None,
            },
            self._on_decision_ok,
            self._set_error,
        )

    # ---- 回调 ----

    def _on_bundle_ok(self, bundle: dict) -> None:
        bundle = bundle or {}
        self._task_id = str(bundle.get("task_id") or "")
        self._evidence_version = int(bundle.get("evidence_version") or 0)
        self._is_latest = bool(bundle.get("is_latest", False))
        self._outcome = str(bundle.get("outcome") or "")
        self._attribution = str(bundle.get("attribution") or "")
        self._attribution_reasons = list(bundle.get("attribution_reasons") or [])
        self._summary = str(bundle.get("summary") or "")
        verification = bundle.get("verification") or {}
        self._verification_status = str(verification.get("status") or "")
        self._verification_detail = str(verification.get("detail") or "")
        self._verification_source = str(verification.get("source") or "")
        self._baseline_dirty_files = list(bundle.get("baseline_dirty_files") or [])
        self._files_model.reset_with(list(bundle.get("files") or []))
        self.bundleChanged.emit()

    def _on_decision_ok(self, result: dict) -> None:
        decision = (result or {}).get("decision") or {}
        self._decision_kind = str(decision.get("kind") or "")
        self._decision_reason = str(decision.get("reason") or "")
        self._decided_at = str(decision.get("decided_at") or "")
        self.decisionChanged.emit()

    # ---- 属性（全部只读）----

    def _get_task_id(self) -> str:
        return self._task_id

    def _get_evidence_version(self) -> int:
        return self._evidence_version

    def _get_is_latest(self) -> bool:
        return self._is_latest

    def _get_outcome(self) -> str:
        return self._outcome

    def _get_attribution(self) -> str:
        return self._attribution

    def _get_attribution_reasons(self) -> list:
        return list(self._attribution_reasons)

    def _get_summary(self) -> str:
        return self._summary

    def _get_files_model(self) -> QObject:
        return self._files_model

    def _get_verification_status(self) -> str:
        return self._verification_status

    def _get_verification_detail(self) -> str:
        return self._verification_detail

    def _get_verification_source(self) -> str:
        return self._verification_source

    def _get_baseline_dirty_files(self) -> list:
        return list(self._baseline_dirty_files)

    def _get_decision_kind(self) -> str:
        return self._decision_kind

    def _get_decision_reason(self) -> str:
        return self._decision_reason

    def _get_decided_at(self) -> str:
        return self._decided_at

    taskId = Property(str, _get_task_id, notify=bundleChanged)
    evidenceVersion = Property(int, _get_evidence_version, notify=bundleChanged)
    isLatest = Property(bool, _get_is_latest, notify=bundleChanged)
    outcome = Property(str, _get_outcome, notify=bundleChanged)
    attribution = Property(str, _get_attribution, notify=bundleChanged)
    attributionReasons = Property("QVariantList", _get_attribution_reasons, notify=bundleChanged)
    summary = Property(str, _get_summary, notify=bundleChanged)
    files = Property(QObject, _get_files_model, constant=True)
    verificationStatus = Property(str, _get_verification_status, notify=bundleChanged)
    verificationDetail = Property(str, _get_verification_detail, notify=bundleChanged)
    verificationSource = Property(str, _get_verification_source, notify=bundleChanged)
    baselineDirtyFiles = Property("QVariantList", _get_baseline_dirty_files, notify=bundleChanged)
    decisionKind = Property(str, _get_decision_kind, notify=decisionChanged)
    decisionReason = Property(str, _get_decision_reason, notify=decisionChanged)
    decidedAt = Property(str, _get_decided_at, notify=decisionChanged)
