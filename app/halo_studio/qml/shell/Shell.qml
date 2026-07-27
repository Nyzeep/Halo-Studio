import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../theme"
import "../views"
import "../palette"

Item {
    id: root
    readonly property var shellRef: (typeof shellVM !== "undefined") ? shellVM : null
    readonly property var registryRef: (typeof commandRegistry !== "undefined") ? commandRegistry : null
    readonly property var paletteRef: (typeof paletteVM !== "undefined") ? paletteVM : null
    readonly property var editorRef: (typeof editorService !== "undefined") ? editorService : null

    HandoffDialog { id: handoffDialog }
    CommandPalette { palette: root.paletteRef }

    Shortcut { sequence: "Ctrl+Shift+P"; context: Qt.ApplicationShortcut; onActivated: if (root.registryRef) root.registryRef.execute("palette.commands") }
    Shortcut { sequence: "Ctrl+P"; context: Qt.ApplicationShortcut; onActivated: if (root.registryRef) root.registryRef.execute("palette.quickOpen") }
    Shortcut { sequence: "Ctrl+S"; context: Qt.ApplicationShortcut; onActivated: if (root.registryRef) root.registryRef.execute("editor.save") }
    Shortcut { sequence: "Ctrl+F"; context: Qt.ApplicationShortcut; onActivated: if (root.registryRef) root.registryRef.execute("editor.find") }
    Shortcut { sequence: "Ctrl+B"; context: Qt.ApplicationShortcut; onActivated: if (root.registryRef) root.registryRef.execute("view.toggleSidebar") }
    Shortcut { sequence: "Ctrl+J"; context: Qt.ApplicationShortcut; onActivated: if (root.registryRef) root.registryRef.execute("view.toggleBottomPanel") }

    RowLayout {
        anchors.fill: parent
        spacing: 0
        ActivityBar { shell: root.shellRef; Layout.fillHeight: true }
        SplitView {
            id: mainSplit
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal
            clip: true
            onResizingChanged: {
                if (!resizing && root.shellRef && sideBar.visible)
                    root.shellRef.storeSideBarWidth(Math.round(sideBar.width))
            }

            SideBarHost {
                id: sideBar
                shell: root.shellRef
                visible: !root.shellRef || root.shellRef.sideBarVisible
                SplitView.minimumWidth: Theme.sideBarMinWidth
                SplitView.preferredWidth: root.shellRef && root.shellRef.sideBarWidth > 0
                    ? root.shellRef.sideBarWidth : Math.min(340, root.width / 4)
                SplitView.maximumWidth: Math.max(520, root.width * 0.55)
            }
            SplitView {
                id: centerSplit
                orientation: Qt.Vertical
                clip: true
                SplitView.fillWidth: true
                onResizingChanged: {
                    if (!resizing && root.shellRef && bottomPanel.visible)
                        root.shellRef.storeBottomPanelHeight(Math.round(bottomPanel.height))
                }

                CenterHost {
                    id: centerHost
                    shell: root.shellRef
                    SplitView.fillHeight: true
                    onHandoffRequested: function(taskId, selectedFiles) {
                        handoffDialog.handoffTaskId = taskId
                        handoffDialog.handoffSelectedFiles = selectedFiles
                        handoffDialog.open()
                    }
                    onOpenInEditorRequested: function(path, line) {
                        if (root.editorRef) root.editorRef.openFile(path, line)
                        if (root.shellRef) root.shellRef.showEditor()
                    }
                }
                BottomPanelHost {
                    id: bottomPanel
                    shell: root.shellRef
                    visible: !root.shellRef || root.shellRef.bottomPanelVisible
                    SplitView.minimumHeight: Theme.bottomPanelMinHeight
                    SplitView.preferredHeight: root.shellRef && root.shellRef.bottomPanelHeight > 0
                        ? root.shellRef.bottomPanelHeight : Math.max(190, root.height / 3)
                    SplitView.maximumHeight: Math.max(320, root.height * 0.72)
                }
            }
        }
    }
}
