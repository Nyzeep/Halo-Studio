"""最新交付证据的保守行级归因投影。"""

from __future__ import annotations

from PySide6.QtCore import QObject

from .diffparse import added_line_ranges
from .latest_review import LatestReviewLifecycle
from .paths import editor_path_to_review_path


class AttributionGutterController(QObject):
    def __init__(self, client, editor_service, workspace_vm, manual_notifier=None, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._editor = editor_service
        self._workspace = workspace_vm
        self._manual_notifier = manual_notifier
        self._bundle: dict | None = None
        self._ranges_by_evidence: dict[tuple[str, int, str], list[tuple[int, int]] | None] = {}
        self._latest = LatestReviewLifecycle(client, self.apply_bundle, self._clear_projection)
        if hasattr(editor_service, "activeChanged"):
            editor_service.activeChanged.connect(self.refresh_active_document)
        if hasattr(editor_service, "documentSaved"):
            editor_service.documentSaved.connect(self.on_document_saved)

    def apply_bundle(self, bundle: dict) -> None:
        source = bundle if isinstance(bundle, dict) else {}
        if source.get("is_latest") is False:
            self._clear_projection()
            return
        self._bundle = dict(source)
        self.refresh_documents()

    def sync_task(self, task_id: str, state: str, evidence_version: int = 0) -> None:
        self._latest.sync_task(task_id, state, evidence_version)

    def refresh_active_document(self) -> None:
        document = getattr(self._editor, "activeDocument", None)
        if document is None:
            return
        self._write_document_decorations(document)

    def refresh_documents(self) -> None:
        for document in self._documents():
            self._write_document_decorations(document)

    def on_document_saved(self, document_id: str, path: str, sha256: str) -> None:
        document = self._find_document(document_id, path)
        if document is None:
            return
        # 服务已更新 diskSha256；仍以回调值覆盖测试替身和异步时序间隙。
        if getattr(document, "diskSha256", "") != sha256:
            self._editor.setGutterDecorations(document_id, [])
            return
        self._write_document_decorations(document)

    def clear(self) -> None:
        self._latest.clear()

    def _clear_projection(self) -> None:
        for document in self._documents():
            self._editor.setGutterDecorations(str(getattr(document, "documentId", "")), [])
        self._bundle = None
        self._ranges_by_evidence.clear()

    def _write_document_decorations(self, document) -> None:
        document_id = str(getattr(document, "documentId", ""))
        if not document_id:
            return
        evidence = self._matching_file(document)
        if evidence is None:
            self._editor.setGutterDecorations(document_id, [])
            return
        end_hash = str(evidence.get("end_hash") or "")
        if (
            not end_hash
            or bool(evidence.get("truncated", False))
            or end_hash != str(getattr(document, "diskSha256", ""))
        ):
            self._editor.setGutterDecorations(document_id, [])
            return
        bundle = self._bundle or {}
        path = str(evidence.get("path") or "")
        attribution = str(bundle.get("attribution") or "agent_only")
        manual_paths = list(bundle.get("manual_edit_paths") or [])
        if "manual_edit_paths" not in bundle and self._manual_notifier is not None:
            manual_paths = list(getattr(self._manual_notifier, "manualEditPaths", []) or [])
        review_manual_paths = {
            converted.casefold()
            for item in manual_paths
            if (
                converted := editor_path_to_review_path(
                    str(item),
                    str(getattr(self._workspace, "realPath", "")),
                    str(getattr(self._workspace, "gitRoot", "")),
                )
            )
        }
        version = int(bundle.get("evidence_version") or 0)
        if attribution == "mixed" and path.casefold() in review_manual_paths:
            token = "gutterMixedChangeBackground"
            tooltip = f"任务关联变更（证据 v{version} · 归因 Mixed：此文件曾发生人工介入，行级归因不作断言）"
        elif attribution == "mixed":
            token = "gutterAgentChangeBackground"
            tooltip = f"任务关联变更（证据 v{version} · 归因：Agent；本任务整体为 Mixed）"
        else:
            token = "gutterAgentChangeBackground"
            tooltip = f"任务关联变更（证据 v{version} · 归因：仅 Agent）"
        ranges = self._ranges_for(bundle, evidence)
        if ranges is None:
            self._editor.setGutterDecorations(document_id, [])
            return
        decorations = [
            {"line": line, "kind": "attribution", "colorToken": token, "tooltip": tooltip}
            for start, end in ranges
            for line in range(start, end + 1)
        ]
        self._editor.setGutterDecorations(document_id, decorations)

    def _ranges_for(self, bundle: dict, evidence: dict) -> list[tuple[int, int]] | None:
        key = (
            str(bundle.get("task_id") or ""),
            int(bundle.get("evidence_version") or 0),
            str(evidence.get("path") or ""),
        )
        if key not in self._ranges_by_evidence:
            self._ranges_by_evidence[key] = added_line_ranges(str(evidence.get("diff") or ""))
        return self._ranges_by_evidence[key]

    def _matching_file(self, document) -> dict | None:
        bundle = self._bundle or {}
        if bundle.get("is_latest") is False:
            return None
        path = editor_path_to_review_path(
            str(getattr(document, "path", "")),
            str(getattr(self._workspace, "realPath", "")),
            str(getattr(self._workspace, "gitRoot", "")),
        )
        if not path:
            return None
        for item in list(bundle.get("files") or []):
            if str(item.get("path") or "").casefold() == path.casefold():
                return item
        return None

    def _documents(self) -> list:
        model = getattr(self._editor, "documents", None)
        getter = getattr(model, "documents", None)
        if callable(getter):
            return list(getter())
        active = getattr(self._editor, "activeDocument", None)
        return [active] if active is not None else []

    def _find_document(self, document_id: str, path: str):
        for document in self._documents():
            if str(getattr(document, "documentId", "")) == str(document_id):
                return document
        active = getattr(self._editor, "activeDocument", None)
        if active is not None and str(getattr(active, "path", "")) == str(path):
            return active
        return None
