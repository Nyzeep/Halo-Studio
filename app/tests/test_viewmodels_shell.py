"""IDE 壳层布局状态的公开行为测试。"""

from __future__ import annotations

from PySide6.QtCore import QCoreApplication, QSettings

from halo_studio.viewmodels.shell import ShellViewModel


def _settings(tmp_path) -> QSettings:
    QCoreApplication.instance() or QCoreApplication([])
    settings = QSettings(str(tmp_path / "shell.ini"), QSettings.Format.IniFormat)
    settings.clear()
    return settings


def test_activity_routing_and_repeated_click_collapse(tmp_path):
    vm = ShellViewModel(_settings(tmp_path))

    assert vm.activeSideBarPanel == "explorer"
    assert vm.centerMode == "editor"
    assert vm.sideBarVisible is True

    vm.activate("task")
    assert vm.activeSideBarPanel == "task"
    assert vm.centerMode == "editor"
    assert vm.sideBarVisible is True

    vm.activate("task")
    assert vm.sideBarVisible is False

    vm.activate("review")
    assert vm.centerMode == "review"
    assert vm.sideBarVisible is False
    vm.activate("review")
    assert vm.centerMode == "editor"

    before = (vm.activeSideBarPanel, vm.centerMode, vm.sideBarVisible)
    vm.activate("not-a-view")
    assert (vm.activeSideBarPanel, vm.centerMode, vm.sideBarVisible) == before


def test_layout_preferences_persist_but_center_starts_in_editor(tmp_path):
    settings = _settings(tmp_path)
    vm = ShellViewModel(settings)
    vm.activate("history")
    vm.toggleSideBar()
    vm.toggleBottomPanel()
    vm.storeSideBarWidth(392)
    vm.storeBottomPanelHeight(241)
    vm.flush()

    restored = ShellViewModel(settings)
    assert restored.centerMode == "editor"
    assert restored.sideBarVisible is False
    assert restored.bottomPanelVisible is False
    assert restored.sideBarWidth == 392
    assert restored.bottomPanelHeight == 241

    restored.storeSideBarWidth(0)
    restored.storeBottomPanelHeight(-1)
    assert restored.sideBarWidth == 392
    assert restored.bottomPanelHeight == 241


def test_show_bottom_panel_is_idempotent(tmp_path):
    vm = ShellViewModel(_settings(tmp_path))
    vm.toggleBottomPanel()
    assert vm.bottomPanelVisible is False

    vm.showBottomPanel()
    assert vm.bottomPanelVisible is True
    vm.showBottomPanel()
    assert vm.bottomPanelVisible is True
