"""命令上下文、注册表与模糊匹配的公共契约测试。"""

from __future__ import annotations

import pytest
from PySide6.QtCore import QCoreApplication

from halo_studio.commands.fuzzy import fuzzy_match, fuzzy_score, score_file_candidate
from halo_studio.commands.registry import CommandRegistry
from halo_studio.commands.when_context import WhenContext


@pytest.fixture(scope="session")
def core_app():
    app = QCoreApplication.instance()
    return app or QCoreApplication([])


@pytest.mark.parametrize(
    ("query", "target", "score", "indices"),
    [
        ("a", "abc", 10, [0]),
        ("ab", "abc", 18, [0, 1]),
        ("ac", "abc", 12, [0, 2]),
        ("cr", "CommandRegistry", 12, [0, 7]),
        ("src/m", "src\\main.py", 44, [0, 1, 2, 3, 4]),
    ],
)
def test_fuzzy_score_matches_design_examples(query, target, score, indices):
    assert fuzzy_score(query, target) == (score, indices)


def test_fuzzy_multisegment_and_file_priority():
    assert fuzzy_match("main py", "src\\main.py") == (45, [4, 5, 6, 7, 9, 10])
    scored = score_file_candidate("main", "src/main.py")
    assert scored is not None
    assert scored.matched_on == "basename"
    assert scored.score >= 1 << 17


def test_when_context_and_registry_share_the_same_enablement_gate(core_app):
    context = WhenContext()
    registry = CommandRegistry(context)
    calls: list[str] = []
    failures: list[tuple[str, str]] = []
    registry.executeFailed.connect(lambda command_id, reason: failures.append((command_id, reason)))

    assert context.evaluate("hasWorkspace && !taskRunning") is False
    assert registry.register(
        "editor.save", "保存文件", "编辑器", lambda: calls.append("save"), "Ctrl+S", "hasActiveEditor"
    ) is True
    assert registry.register(
        "editor.saveAll", "保存全部", "编辑器", lambda: calls.append("all"), "Ctrl+S"
    ) is True
    assert registry.get("editor.saveAll").shortcut is None
    assert registry.execute("editor.save") is False
    assert failures[-1] == ("editor.save", "当前状态下不可用")

    context.set_key("hasActiveEditor", True)
    assert registry.execute("editor.save") is True
    assert calls == ["save"]
    assert registry.commands.rowCount() == 2
    assert registry.is_enabled("editor.save") is True
    assert registry.unregister("editor.save") is True
    assert registry.unregister("editor.save") is False


def test_registry_rejects_invalid_ids_and_unknown_commands(core_app):
    registry = CommandRegistry(WhenContext())
    with pytest.raises(ValueError):
        registry.register("not-an-id", "x", "x", lambda: None)
    assert registry.execute("editor.missing") is False
