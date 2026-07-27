"""设计 15 差异化功能的纯逻辑与控制器契约测试。"""

from __future__ import annotations

from types import SimpleNamespace

import pytest
from PySide6.QtCore import QCoreApplication

import halo_studio.differentiation.attribution_gutter as attribution_gutter_module
from halo_studio.differentiation.attribution_gutter import AttributionGutterController
from halo_studio.differentiation.baseline_badges import BaselineBadgeController
from halo_studio.differentiation.diffparse import added_line_ranges, first_target_line
from halo_studio.differentiation.manual_edit_notifier import ManualEditNotifier
from halo_studio.differentiation.review_jump import ReviewJumpViewModel
from halo_studio.differentiation.task_context import TaskContextViewModel


class SignalStub:
    def __init__(self) -> None:
        self.handlers: list = []

    def connect(self, handler) -> None:
        self.handlers.append(handler)

    def emit(self, *args) -> None:
        for handler in list(self.handlers):
            handler(*args)


class FakeClient:
    def __init__(self, defer_review: bool = False) -> None:
        self.subscriptions: dict[str, list] = {}
        self.requests: list[tuple[str, dict]] = []
        self._defer_review = defer_review
        self._review_callbacks: list = []

    def subscribe(self, event: str, handler) -> None:
        self.subscriptions.setdefault(event, []).append(handler)

    def request(self, method, params, on_ok=None, on_err=None) -> None:
        self.requests.append((method, dict(params)))
        if on_ok is not None and method == "review.get":
            if self._defer_review:
                self._review_callbacks.append(on_ok)
            else:
                on_ok({})

    def resolve_review(self, bundle: dict, index: int = 0) -> None:
        self._review_callbacks.pop(index)(bundle)

    def emit(self, event: str, payload: dict, task_id: str | None = None) -> None:
        envelope = {"event": event, "payload": payload, "task_id": task_id}
        for handler in list(self.subscriptions.get(event, [])):
            handler(envelope)


class FakeWorkspace:
    def __init__(self, real_path: str = "D:/repo/sub", git_root: str = "D:/repo") -> None:
        self.realPath = real_path
        self.gitRoot = git_root
        self.statusChanged = SignalStub()


class FakeExplorerModel:
    def __init__(self) -> None:
        self.decorations: dict = {}

    def set_decorations(self, decorations: dict) -> None:
        self.decorations = dict(decorations)


class FakeEditor:
    def __init__(self, document=None, documents=None) -> None:
        open_documents = list(documents) if documents is not None else ([document] if document is not None else [])
        self.activeDocument = document if document is not None else (open_documents[0] if open_documents else None)
        self.activeChanged = SignalStub()
        self.documentSaved = SignalStub()
        self.documents = SimpleNamespace(documents=lambda: list(open_documents))
        self.baseline_paths: list[str] = []
        self.gutter_calls: list[tuple[str, list[dict]]] = []

    def setBaselineChangedPaths(self, paths: list) -> None:  # noqa: N802
        self.baseline_paths = list(paths)

    def setGutterDecorations(self, document_id: str, decorations: list) -> None:  # noqa: N802
        self.gutter_calls.append((document_id, list(decorations)))


@pytest.fixture(scope="session")
def core_app():
    return QCoreApplication.instance() or QCoreApplication([])


@pytest.mark.parametrize(
    ("diff", "expected"),
    [
        ("@@ -7,2 +120,3 @@\n-old\n+new\n+next", 120),
        ("@@ -1 +1 @@\n+a\n@@ -9 +10 @@\n+b", 1),
        ("", -1),
        ("@@ broken", -1),
    ],
)
def test_first_target_line_is_conservative(diff, expected):
    assert first_target_line(diff) == expected


def test_added_line_ranges_cover_adds_replacements_and_pure_deletes():
    diff = "\n".join(
        [
            "@@ -2,2 +2,3 @@",
            " context",
            "-old",
            "+new",
            "+next",
            "@@ -10 +11,0 @@",
            "-gone",
        ]
    )
    assert added_line_ranges(diff) == [(3, 4), (11, 11)]


@pytest.mark.parametrize(
    ("diff", "expected"),
    [
        ("@@ -1,1 +1,3 @@\n-old\n+one\n+two\n+three", [(1, 3)]),
        ("@@ -1,3 +1,6 @@\n first\n+one\n second\n+two\n+three\n third", [(2, 2), (4, 5)]),
        ("@@ -0,0 +1,2 @@\n+first\n+second", [(1, 2)]),
    ],
)
def test_added_line_ranges_preserves_each_added_segment(diff, expected):
    assert added_line_ranges(diff) == expected


@pytest.mark.parametrize("diff", ["", "not a diff", "@@ broken", "@@ -1,2 +1,1 @@\n+only"])
def test_added_line_ranges_rejects_empty_or_malformed_input(diff):
    assert added_line_ranges(diff) is None


def test_task_context_dedupes_files_and_formats_selection(core_app):
    editor = SimpleNamespace(
        activeFilePath="src/auth.rs",
        currentSelection={
            "path": "src/auth.rs",
            "startLine": 120,
            "endLine": 121,
            "hasSelection": True,
            "text": "first\nsecond",
            "textTruncated": False,
        },
    )
    context = TaskContextViewModel(editor)
    notes: list[str] = []
    context.notesBlockAppended.connect(notes.append)

    assert context.addFile("src\\auth.rs") is True
    assert context.addFile("src/auth.rs") is False
    assert context.filesList() == ["src/auth.rs"]
    assert context.fileCount == 1
    assert context.addActiveEditorSelection() is True
    assert notes == [
        "--- 选区 src/auth.rs 第 120-121 行 ---\nfirst\nsecond\n--- 选区结束 ---"
    ]
    assert context.filesList() == ["src/auth.rs"]
    context.removeFile("src/auth.rs")
    assert context.filesList() == []
    assert context.fileCount == 0


def test_task_context_degrades_oversized_selection_to_location_only(core_app):
    editor = SimpleNamespace(
        activeFilePath="src/auth.rs",
        currentSelection={
            "path": "src/auth.rs",
            "startLine": 1,
            "endLine": 250,
            "hasSelection": True,
            "text": "x\n" * 250,
            "textTruncated": True,
        },
    )
    context = TaskContextViewModel(editor)
    notes: list[str] = []
    context.notesBlockAppended.connect(notes.append)
    context.addActiveEditorSelection()
    assert notes == ["--- 选区 src/auth.rs 第 1-250 行（内容过长未附原文，请按行号查阅）---"]
    assert context.hint


def test_task_context_clears_draft_when_workspace_changes(core_app):
    client = FakeClient()
    editor = SimpleNamespace(activeFilePath="src/auth.rs", currentSelection={})
    context = TaskContextViewModel(editor, client=client)
    cleared: list[bool] = []
    context.draftCleared.connect(lambda: cleared.append(True))

    assert context.addFile("src/auth.rs") is True
    client.emit("workspace.changed", {"active": True, "real_path": "D:/other-workspace"})

    assert context.filesList() == []
    assert cleared == [True]


def test_review_jump_maps_git_root_paths_and_rejects_deleted_or_outside(core_app):
    workspace = FakeWorkspace()
    review = SimpleNamespace(evidenceVersion=7, bundleChanged=SignalStub())
    jump = ReviewJumpViewModel(review, workspace)
    inside = jump.describe("sub/src/auth.rs", "modified", "@@ -1 +120 @@", False)
    assert inside == {
        "editorPath": "src/auth.rs",
        "editorLine": 120,
        "canOpen": True,
        "reason": "定位基于证据版本 v7，文件此后再编辑可能已漂移",
    }
    deleted = jump.describe("sub/src/deleted.rs", "deleted", "", False)
    assert deleted["canOpen"] is False
    assert "删除" in deleted["reason"]
    outside = jump.describe("other/src/a.rs", "modified", "", False)
    assert outside["canOpen"] is False
    assert "子目录之外" in outside["reason"]


def test_manual_edit_notifier_tracks_paths_per_task_and_resets_on_new_task(core_app):
    client = FakeClient()
    notifier = ManualEditNotifier(client)
    client.emit("task.manual_edit", {"path": "src/a.rs"}, task_id="task-1")
    client.emit("task.manual_edit", {"path": "src/a.rs"}, task_id="task-1")
    client.emit("task.manual_edit", {"path": "src/b.rs"}, task_id="task-1")
    assert notifier.manualEditPaths == ["src/a.rs", "src/b.rs"]
    assert notifier.manualEditCount == 2
    client.emit("task.state", {"state": "created", "task": {"task_id": "task-2"}}, task_id="task-2")
    assert notifier.manualEditPaths == []

    client.emit("task.manual_edit", {"path": "src/c.rs"}, task_id="task-2")
    client.emit("workspace.changed", {"active": True})
    assert notifier.manualEditPaths == []


def test_baseline_badges_map_evidence_and_drop_paths_outside_subworkspace(core_app):
    client = FakeClient()
    explorer = SimpleNamespace(model=FakeExplorerModel())
    editor = FakeEditor()
    controller = BaselineBadgeController(client, explorer, editor, FakeWorkspace())
    controller.apply_bundle(
        {
            "evidence_version": 2,
            "files": [
                {"path": "sub/src/a.rs", "change": "modified"},
                {"path": "sub/src/new.rs", "change": "added"},
                {"path": "other/out.rs", "change": "deleted"},
            ],
        }
    )
    decorations = explorer.model.decorations
    assert set(decorations) == {"src/a.rs", "src/new.rs"}
    assert decorations["src/a.rs"].letter == "M"
    assert decorations["src/new.rs"].letter == "A"
    assert editor.baseline_paths == ["src/a.rs", "src/new.rs"]

    client.emit("workspace.changed", {"active": True})
    assert explorer.model.decorations == {}
    assert editor.baseline_paths == []


def test_baseline_badges_ignore_review_callback_after_task_reset(core_app):
    client = FakeClient(defer_review=True)
    explorer = SimpleNamespace(model=FakeExplorerModel())
    editor = FakeEditor()
    controller = BaselineBadgeController(client, explorer, editor, FakeWorkspace())
    controller.sync_task("task-old", "review_ready", 1)
    controller.sync_task("task-new", "created")

    client.resolve_review(
        {
            "evidence_version": 1,
            "files": [{"path": "sub/src/old.rs", "change": "modified"}],
        }
    )

    assert explorer.model.decorations == {}
    assert editor.baseline_paths == []


def test_baseline_badges_load_each_evidence_version_once(core_app):
    client = FakeClient(defer_review=True)
    explorer = SimpleNamespace(model=FakeExplorerModel())
    controller = BaselineBadgeController(client, explorer, FakeEditor(), FakeWorkspace())
    task = {"task_id": "task-1", "latest_evidence_version": 3}

    client.emit("task.state", {"state": "review_ready", "task": task}, task_id="task-1")
    client.emit("task.finished", {"outcome": "finished", "evidence_version": 3}, task_id="task-1")

    assert client.requests == [("review.get", {"task_id": "task-1"})]


def test_baseline_badges_discard_a_review_callback_for_an_older_evidence_version(core_app):
    client = FakeClient(defer_review=True)
    explorer = SimpleNamespace(model=FakeExplorerModel())
    editor = FakeEditor()
    controller = BaselineBadgeController(client, explorer, editor, FakeWorkspace())

    controller.sync_task("task-1", "review_ready", 1)
    controller.sync_task("task-1", "review_ready", 2)
    client.resolve_review(
        {
            "task_id": "task-1",
            "evidence_version": 1,
            "files": [{"path": "sub/src/old.rs", "change": "modified"}],
        }
    )

    assert explorer.model.decorations == {}
    assert editor.baseline_paths == []

    client.resolve_review(
        {
            "task_id": "task-1",
            "evidence_version": 2,
            "files": [{"path": "sub/src/new.rs", "change": "added"}],
        }
    )

    assert set(explorer.model.decorations) == {"src/new.rs"}
    assert editor.baseline_paths == ["src/new.rs"]


def test_attribution_gutter_requires_fresh_hash_and_clears_after_save(core_app):
    client = FakeClient()
    document = SimpleNamespace(documentId="doc-1", path="src/auth.rs", diskSha256="sha256:end")
    editor = FakeEditor(document)
    controller = AttributionGutterController(client, editor, FakeWorkspace(real_path="D:/repo", git_root="D:/repo"))
    controller.apply_bundle(
        {
            "evidence_version": 3,
            "is_latest": True,
            "attribution": "mixed",
            "manual_edit_paths": ["src/auth.rs"],
            "files": [
                {
                    "path": "src/auth.rs",
                    "change": "modified",
                    "diff": "@@ -1,1 +4,2 @@\n-old\n+new\n+next",
                    "truncated": False,
                    "end_hash": "sha256:end",
                }
            ],
        }
    )
    assert editor.gutter_calls[-1][0] == "doc-1"
    decorations = editor.gutter_calls[-1][1]
    assert [item["line"] for item in decorations] == [4, 5]
    assert all(item["colorToken"] == "gutterMixedChangeBackground" for item in decorations)

    controller.on_document_saved("doc-1", "src/auth.rs", "sha256:changed")
    assert editor.gutter_calls[-1] == ("doc-1", [])


def test_attribution_gutter_memoizes_ranges_by_task_evidence_and_path(core_app, monkeypatch):
    client = FakeClient()
    document = SimpleNamespace(documentId="doc-1", path="src/auth.rs", diskSha256="sha256:end")
    editor = FakeEditor(document)
    controller = AttributionGutterController(client, editor, FakeWorkspace(real_path="D:/repo", git_root="D:/repo"))
    parsed: list[str] = []

    def parse_ranges(diff: str):
        parsed.append(diff)
        return [(4, 4)]

    monkeypatch.setattr(attribution_gutter_module, "added_line_ranges", parse_ranges)
    bundle = {
        "task_id": "task-1",
        "evidence_version": 3,
        "is_latest": True,
        "attribution": "agent_only",
        "files": [
            {
                "path": "src/auth.rs",
                "diff": "@@ -1,1 +4,1 @@\n-old\n+new",
                "truncated": False,
                "end_hash": "sha256:end",
            }
        ],
    }

    controller.apply_bundle(bundle)
    controller.refresh_documents()
    assert parsed == [bundle["files"][0]["diff"]]

    controller.apply_bundle({**bundle, "evidence_version": 4})
    assert parsed == [bundle["files"][0]["diff"], bundle["files"][0]["diff"]]


def test_attribution_gutter_respects_explicit_empty_manual_edit_paths(core_app):
    client = FakeClient()
    document = SimpleNamespace(documentId="doc-1", path="src/auth.rs", diskSha256="sha256:end")
    editor = FakeEditor(document)
    notifier = SimpleNamespace(manualEditPaths=["src/auth.rs"])
    controller = AttributionGutterController(
        client,
        editor,
        FakeWorkspace(real_path="D:/repo", git_root="D:/repo"),
        notifier,
    )
    controller.apply_bundle(
        {
            "evidence_version": 3,
            "attribution": "mixed",
            "manual_edit_paths": [],
            "files": [
                {
                    "path": "src/auth.rs",
                    "diff": "@@ -1,1 +4,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:end",
                }
            ],
        }
    )

    decoration = editor.gutter_calls[-1][1][0]
    assert decoration["colorToken"] == "gutterAgentChangeBackground"
    assert "本任务整体为 Mixed" in decoration["tooltip"]


def test_attribution_gutter_maps_subworkspace_manual_paths_to_review_paths(core_app):
    client = FakeClient()
    document = SimpleNamespace(documentId="doc-1", path="src/auth.rs", diskSha256="sha256:end")
    editor = FakeEditor(document)
    controller = AttributionGutterController(client, editor, FakeWorkspace())

    controller.apply_bundle(
        {
            "evidence_version": 3,
            "is_latest": True,
            "attribution": "mixed",
            "manual_edit_paths": ["src/auth.rs"],
            "files": [
                {
                    "path": "sub/src/auth.rs",
                    "diff": "@@ -1,1 +4,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:end",
                }
            ],
        }
    )

    decoration = editor.gutter_calls[-1][1][0]
    assert decoration["colorToken"] == "gutterMixedChangeBackground"
    assert "行级归因不作断言" in decoration["tooltip"]


def test_attribution_gutter_refreshes_all_open_documents_and_clears_on_workspace_change(core_app):
    client = FakeClient()
    first = SimpleNamespace(documentId="doc-1", path="src/first.rs", diskSha256="sha256:first")
    second = SimpleNamespace(documentId="doc-2", path="src/second.rs", diskSha256="sha256:second")
    editor = FakeEditor(first, documents=[first, second])
    controller = AttributionGutterController(client, editor, FakeWorkspace(real_path="D:/repo", git_root="D:/repo"))

    controller.apply_bundle(
        {
            "evidence_version": 4,
            "is_latest": True,
            "attribution": "agent_only",
            "files": [
                {
                    "path": "src/first.rs",
                    "diff": "@@ -1,1 +2,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:first",
                },
                {
                    "path": "src/second.rs",
                    "diff": "@@ -1,1 +3,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:second",
                },
            ],
        }
    )

    assert [(document_id, [item["line"] for item in decorations]) for document_id, decorations in editor.gutter_calls] == [
        ("doc-1", [2]),
        ("doc-2", [3]),
    ]

    editor.gutter_calls.clear()
    controller.apply_bundle({"is_latest": False})
    assert editor.gutter_calls == [("doc-1", []), ("doc-2", [])]

    controller.apply_bundle(
        {
            "evidence_version": 4,
            "is_latest": True,
            "attribution": "agent_only",
            "files": [
                {
                    "path": "src/first.rs",
                    "diff": "@@ -1,1 +2,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:first",
                },
                {
                    "path": "src/second.rs",
                    "diff": "@@ -1,1 +3,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:second",
                },
            ],
        }
    )
    client.emit("workspace.changed", {"active": True})
    assert editor.gutter_calls[-2:] == [("doc-1", []), ("doc-2", [])]


def test_attribution_gutter_ignores_review_callback_after_workspace_change(core_app):
    client = FakeClient(defer_review=True)
    document = SimpleNamespace(documentId="doc-1", path="src/auth.rs", diskSha256="sha256:end")
    editor = FakeEditor(document)
    controller = AttributionGutterController(client, editor, FakeWorkspace(real_path="D:/repo", git_root="D:/repo"))
    controller.sync_task("task-old", "review_ready", 1)
    client.emit("workspace.changed", {"active": True})
    calls_after_clear = len(editor.gutter_calls)

    client.resolve_review(
        {
            "evidence_version": 1,
            "files": [
                {
                    "path": "src/auth.rs",
                    "diff": "@@ -1,1 +2,1 @@\n-old\n+new",
                    "truncated": False,
                    "end_hash": "sha256:end",
                }
            ],
        }
    )

    assert len(editor.gutter_calls) == calls_after_clear
