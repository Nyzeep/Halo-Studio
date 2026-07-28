"""IDE 壳层布局状态：路由、折叠和 QSettings 持久化。"""

from __future__ import annotations

from PySide6.QtCore import Property, QObject, QSettings, QTimer, Signal, Slot


class ShellViewModel(QObject):
    """纯 UI 状态，不持 IPC client，也不发起任何 Sidecar 请求。"""

    SIDEBAR_ENTRIES = ("explorer", "task")
    CENTER_ENTRIES = ("review", "config", "history")
    CENTER_MODES = ("editor", "review", "config", "history")

    activeSideBarPanelChanged = Signal()
    centerModeChanged = Signal()
    sideBarVisibleChanged = Signal()
    bottomPanelVisibleChanged = Signal()
    sideBarWidthChanged = Signal()
    bottomPanelHeightChanged = Signal()

    def __init__(self, settings: QSettings | None = None, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._settings = settings or QSettings()
        self._active_sidebar_panel = self._read_panel()
        # 中心页不持久化：每次启动都回到编辑器区域。
        self._center_mode = "editor"
        self._side_bar_visible = self._read_bool("shell/sideBarVisible", True)
        self._bottom_panel_visible = self._read_bool("shell/bottomPanelVisible", True)
        self._side_bar_width = self._read_dimension("shell/sideBarWidth")
        self._bottom_panel_height = self._read_dimension("shell/bottomPanelHeight")
        self._dirty = False
        self._flush_timer = QTimer(self)
        self._flush_timer.setSingleShot(True)
        self._flush_timer.setInterval(500)
        self._flush_timer.timeout.connect(self.flush)

    @Slot(str)
    def activate(self, entry_id: str) -> None:
        """执行活动栏点击语义，非法 id 直接忽略。"""
        if entry_id in self.SIDEBAR_ENTRIES:
            if (
                self._active_sidebar_panel == entry_id
                and self._side_bar_visible
                and self._center_mode == "editor"
            ):
                self._set_side_bar_visible(False)
                return
            self._set_active_sidebar_panel(entry_id)
            self._set_center_mode("editor")
            self._set_side_bar_visible(True)
            return
        if entry_id in self.CENTER_ENTRIES:
            self._set_center_mode("editor" if self._center_mode == entry_id else entry_id)

    @Slot()
    def showEditor(self) -> None:
        self._set_center_mode("editor")

    @Slot()
    def showReview(self) -> None:
        self._set_center_mode("review")

    @Slot()
    def toggleSideBar(self) -> None:
        self._set_side_bar_visible(not self._side_bar_visible)

    @Slot()
    def toggleBottomPanel(self) -> None:
        self._set_bottom_panel_visible(not self._bottom_panel_visible)

    @Slot()
    def showBottomPanel(self) -> None:
        self._set_bottom_panel_visible(True)

    @Slot(int)
    def storeSideBarWidth(self, width: int) -> None:
        if width > 0 and width != self._side_bar_width:
            self._side_bar_width = width
            self.sideBarWidthChanged.emit()
            self._schedule_flush()

    @Slot(int)
    def storeBottomPanelHeight(self, height: int) -> None:
        if height > 0 and height != self._bottom_panel_height:
            self._bottom_panel_height = height
            self.bottomPanelHeightChanged.emit()
            self._schedule_flush()

    @Slot()
    def flush(self) -> None:
        if not self._dirty:
            return
        self._flush_timer.stop()
        self._settings.setValue("shell/activeSideBarPanel", self._active_sidebar_panel)
        self._settings.setValue("shell/sideBarVisible", self._side_bar_visible)
        self._settings.setValue("shell/bottomPanelVisible", self._bottom_panel_visible)
        if self._side_bar_width > 0:
            self._settings.setValue("shell/sideBarWidth", self._side_bar_width)
        if self._bottom_panel_height > 0:
            self._settings.setValue("shell/bottomPanelHeight", self._bottom_panel_height)
        self._settings.sync()
        self._dirty = False

    def _set_active_sidebar_panel(self, panel: str) -> None:
        if panel != self._active_sidebar_panel:
            self._active_sidebar_panel = panel
            self.activeSideBarPanelChanged.emit()
            self._schedule_flush()

    def _set_center_mode(self, mode: str) -> None:
        if mode != self._center_mode:
            self._center_mode = mode
            self.centerModeChanged.emit()

    def _set_side_bar_visible(self, visible: bool) -> None:
        if visible != self._side_bar_visible:
            self._side_bar_visible = visible
            self.sideBarVisibleChanged.emit()
            self._schedule_flush()

    def _set_bottom_panel_visible(self, visible: bool) -> None:
        if visible != self._bottom_panel_visible:
            self._bottom_panel_visible = visible
            self.bottomPanelVisibleChanged.emit()
            self._schedule_flush()

    def _schedule_flush(self) -> None:
        self._dirty = True
        self._flush_timer.start()

    def _read_panel(self) -> str:
        value = str(self._settings.value("shell/activeSideBarPanel", "explorer"))
        return value if value in self.SIDEBAR_ENTRIES else "explorer"

    def _read_dimension(self, key: str) -> int:
        try:
            value = int(self._settings.value(key, -1))
        except (TypeError, ValueError):
            return -1
        return value if value > 0 else -1

    def _read_bool(self, key: str, default: bool) -> bool:
        value = self._settings.value(key, default)
        if isinstance(value, str):
            return value.strip().lower() in {"1", "true", "yes"}
        return bool(value)

    def _get_active_sidebar_panel(self) -> str:
        return self._active_sidebar_panel

    def _get_center_mode(self) -> str:
        return self._center_mode

    def _get_side_bar_visible(self) -> bool:
        return self._side_bar_visible

    def _get_bottom_panel_visible(self) -> bool:
        return self._bottom_panel_visible

    def _get_side_bar_width(self) -> int:
        return self._side_bar_width

    def _get_bottom_panel_height(self) -> int:
        return self._bottom_panel_height

    activeSideBarPanel = Property(str, _get_active_sidebar_panel, notify=activeSideBarPanelChanged)
    centerMode = Property(str, _get_center_mode, notify=centerModeChanged)
    sideBarVisible = Property(bool, _get_side_bar_visible, notify=sideBarVisibleChanged)
    bottomPanelVisible = Property(bool, _get_bottom_panel_visible, notify=bottomPanelVisibleChanged)
    sideBarWidth = Property(int, _get_side_bar_width, notify=sideBarWidthChanged)
    bottomPanelHeight = Property(int, _get_bottom_panel_height, notify=bottomPanelHeightChanged)
