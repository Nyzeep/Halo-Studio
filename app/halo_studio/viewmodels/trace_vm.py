"""TraceViewModel：结构化运行轨迹列表模型。

消费 trace.item / task.phase / task.action_request / task.verification 四类事件，
按全局 seq 排序并按 seq 去重；提供 applySnapshot(snapshot) 从 task.snapshot 结果恢复。
原始终端输出永不作为主内容进入本模型。
"""

from __future__ import annotations

import bisect

from PySide6.QtCore import (
    Property,
    QAbstractListModel,
    QModelIndex,
    QObject,
    Qt,
    Signal,
    Slot,
)

_CONSUMED_EVENTS = ("trace.item", "task.phase", "task.action_request", "task.verification")


class TraceViewModel(QAbstractListModel):
    SeqRole = int(Qt.ItemDataRole.UserRole) + 1
    TsRole = SeqRole + 1
    EventRole = SeqRole + 2
    KindRole = SeqRole + 3
    TextRole = SeqRole + 4
    DetailRole = SeqRole + 5

    _ROLE_KEYS = {
        SeqRole: "seq",
        TsRole: "ts",
        EventRole: "event",
        KindRole: "kind",
        TextRole: "text",
        DetailRole: "detail",
    }

    countChanged = Signal()
    lastSeqChanged = Signal()
    errorChanged = Signal()

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._client = client
        self._items: list[dict] = []
        self._seq_order: list[int] = []
        self._seq_set: set[int] = set()
        self._last_seq = 0
        self._error_code = ""
        self._error_message = ""
        for event in _CONSUMED_EVENTS:
            client.subscribe(event, self._on_event)

    # ---- QAbstractListModel ----

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802
        return 0 if parent.isValid() else len(self._items)

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._items)):
            return None
        item = self._items[index.row()]
        if role == Qt.ItemDataRole.DisplayRole:
            return item.get("text")
        key = self._ROLE_KEYS.get(role)
        return item.get(key) if key else None

    def roleNames(self):  # noqa: N802
        return {
            self.SeqRole: b"seq",
            self.TsRole: b"ts",
            self.EventRole: b"event",
            self.KindRole: b"kind",
            self.TextRole: b"text",
            self.DetailRole: b"detail",
        }

    @Slot(int, result="QVariantMap")
    def get(self, row: int) -> dict:
        if 0 <= row < len(self._items):
            return dict(self._items[row])
        return {}

    # ---- 事件消费 ----

    def _on_event(self, envelope: dict) -> None:
        row = self._normalize(envelope)
        if row is None:
            return
        seq = row["seq"]
        if seq in self._seq_set:
            return
        pos = bisect.bisect_left(self._seq_order, seq)
        self.beginInsertRows(QModelIndex(), pos, pos)
        self._seq_order.insert(pos, seq)
        self._items.insert(pos, row)
        self._seq_set.add(seq)
        self.endInsertRows()
        self.countChanged.emit()
        if seq > self._last_seq:
            self._last_seq = seq
            self.lastSeqChanged.emit()

    @staticmethod
    def _normalize(envelope: dict) -> dict | None:
        event = envelope.get("event")
        seq = envelope.get("seq")
        if event not in _CONSUMED_EVENTS or not isinstance(seq, int):
            return None
        payload = envelope.get("payload") or {}
        if event == "trace.item":
            kind = str(payload.get("kind") or "")
            text = str(payload.get("text") or "")
            detail = payload.get("detail") or {}
        elif event == "task.phase":
            kind = "phase"
            text = str(payload.get("detail") or payload.get("phase") or "")
            detail = payload
        elif event == "task.action_request":
            kind = "action_request"
            text = str(payload.get("prompt") or "")
            # 操作卡片刻意保持为窄 IPC 表面，不能透传未来运行时私有字段，
            # 例如远端句柄或传输元数据。
            detail = {
                "request_id": str(payload.get("request_id") or ""),
                "kind": str(payload.get("kind") or ""),
                "prompt": text,
                "decision_sent": payload.get("decision_sent") is True,
            }
        else:  # task.verification
            kind = "verification"
            text = str(payload.get("detail") or "")
            detail = payload
        return {
            "seq": seq,
            "ts": str(envelope.get("ts") or ""),
            "event": event,
            "kind": kind,
            "text": text,
            "detail": detail,
        }

    # ---- 快照恢复与清空 ----

    @Slot()
    def refresh(self) -> None:
        """拉取当前轨迹；缓冲缺口时以最早可用事件整体重建。"""
        self._clear_error()
        self._request_snapshot(self._last_seq)

    def _request_snapshot(self, after_seq: int) -> None:
        self._client.request(
            "task.snapshot",
            {"after_seq": after_seq},
            self.applySnapshot,
            lambda error: self._on_snapshot_error(error, after_seq),
        )

    def _on_snapshot_error(self, error: dict, after_seq: int) -> None:
        details = error.get("details") or {}
        oldest = details.get("oldest_available_seq") if isinstance(details, dict) else None
        if (
            error.get("code") == "EVENT_GAP"
            and isinstance(oldest, int)
            and not isinstance(oldest, bool)
            and oldest > 0
            and after_seq < oldest
        ):
            # Sidecar 已明确给出仍可恢复的首个事件；从其前一位重取环形缓冲。
            self._request_snapshot(oldest - 1)
            return
        self._set_error(error)

    def _set_error(self, error: dict) -> None:
        self._error_code = str(error.get("code") or "")
        self._error_message = str(error.get("message") or "")
        self.errorChanged.emit()

    def _clear_error(self) -> None:
        if self._error_code or self._error_message:
            self._error_code = ""
            self._error_message = ""
            self.errorChanged.emit()

    @Slot("QVariantMap")
    def applySnapshot(self, snapshot: dict) -> None:  # noqa: N802
        """以 task.snapshot 结果整体重建轨迹（EVENT_GAP 后 UI 整体重建视图的入口）。"""
        snapshot = snapshot or {}
        rows: list[dict] = []
        seen: set[int] = set()
        for envelope in snapshot.get("events") or []:
            row = self._normalize(envelope)
            if row is None or row["seq"] in seen:
                continue
            seen.add(row["seq"])
            rows.append(row)
        rows.sort(key=lambda r: r["seq"])
        self.beginResetModel()
        self._items = rows
        self._seq_order = [r["seq"] for r in rows]
        self._seq_set = seen
        self.endResetModel()
        self.countChanged.emit()
        last_seq = snapshot.get("last_seq")
        if not isinstance(last_seq, int):
            last_seq = self._seq_order[-1] if self._seq_order else 0
        if last_seq != self._last_seq:
            self._last_seq = last_seq
            self.lastSeqChanged.emit()

    @Slot()
    def clear(self) -> None:
        self.beginResetModel()
        self._items = []
        self._seq_order = []
        self._seq_set = set()
        self.endResetModel()
        self.countChanged.emit()

    # ---- 属性 ----

    def _get_count(self) -> int:
        return len(self._items)

    def _get_last_seq(self) -> int:
        return self._last_seq

    def _get_error_code(self) -> str:
        return self._error_code

    def _get_error_message(self) -> str:
        return self._error_message

    count = Property(int, _get_count, notify=countChanged)
    lastSeq = Property(int, _get_last_seq, notify=lastSeqChanged)
    errorCode = Property(str, _get_error_code, notify=errorChanged)
    errorMessage = Property(str, _get_error_message, notify=errorChanged)
