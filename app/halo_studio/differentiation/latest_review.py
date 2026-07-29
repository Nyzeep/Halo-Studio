"""Shared lifecycle for projections backed by the latest review bundle."""

from __future__ import annotations

from collections.abc import Callable

_REVIEWABLE_STATES = {"review_ready", "accepted", "rejected", "cancelled", "failed", "interrupted"}


class LatestReviewLifecycle:
    """Load each current task evidence version once and discard stale callbacks."""

    def __init__(self, client, apply_bundle: Callable[[dict], None], clear_projection: Callable[[], None]) -> None:
        self._client = client
        self._apply_bundle = apply_bundle
        self._clear_projection = clear_projection
        self._generation = 0
        self._task_id = ""
        self._loaded_task_version: tuple[str, int] = ("", 0)
        client.subscribe("task.finished", self._on_task_finished)
        client.subscribe("task.state", self._on_task_state)
        client.subscribe("workspace.changed", self._on_workspace_changed)

    def sync_task(self, task_id: str, state: str, evidence_version: int = 0) -> None:
        task_id = str(task_id or "")
        state = str(state or "")
        if task_id != self._task_id:
            self._reset(task_id)
        if state == "created":
            if self._loaded_task_version != ("", 0):
                self._reset(task_id)
            return
        if not task_id or state not in _REVIEWABLE_STATES or not evidence_version:
            return
        key = (task_id, int(evidence_version))
        if key != self._loaded_task_version:
            self._loaded_task_version = key
            self._clear_projection()
            self._load_latest(key)

    def clear(self) -> None:
        self._reset("")

    def _reset(self, task_id: str) -> None:
        self._generation += 1
        self._task_id = task_id
        self._loaded_task_version = ("", 0)
        self._clear_projection()

    def _load_latest(self, key: tuple[str, int]) -> None:
        generation = self._generation
        task_id, _evidence_version = key
        self._client.request(
            "review.get",
            {"task_id": task_id},
            lambda bundle: self._on_review_loaded(key, generation, bundle),
            lambda _error: None,
        )

    def _on_review_loaded(self, key: tuple[str, int], generation: int, bundle: dict) -> None:
        task_id, evidence_version = key
        if (
            generation != self._generation
            or key != self._loaded_task_version
            or task_id != self._task_id
        ):
            return
        source = bundle if isinstance(bundle, dict) else {}
        response_task_id = str(source.get("task_id") or "")
        if response_task_id and response_task_id != task_id:
            return
        try:
            response_version = int(source.get("evidence_version") or 0)
        except (TypeError, ValueError):
            return
        if response_version != evidence_version:
            return
        if source.get("is_latest") is False:
            self._clear_projection()
            return
        self._apply_bundle(source)

    def _on_task_finished(self, envelope: dict) -> None:
        payload = (envelope or {}).get("payload") or {}
        task_id = str((envelope or {}).get("task_id") or "")
        evidence_version = int(payload.get("evidence_version") or 0)
        if task_id and evidence_version:
            self.sync_task(task_id, "review_ready", evidence_version)

    def _on_task_state(self, envelope: dict) -> None:
        payload = (envelope or {}).get("payload") or {}
        task = payload.get("task") or {}
        task_id = str((envelope or {}).get("task_id") or task.get("task_id") or "")
        state = str(payload.get("state") or "")
        evidence_version = int(task.get("latest_evidence_version") or 0)
        self.sync_task(task_id, state, evidence_version)

    def _on_workspace_changed(self, _envelope: dict) -> None:
        self.clear()
