import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../theme"
import "../differentiation"

Rectangle {
    id: root
    required property var shell
    readonly property var appRef: (typeof appVM !== "undefined") ? appVM : null
    readonly property var workspaceRef: (typeof workspaceVM !== "undefined") ? workspaceVM : null
    readonly property var taskRef: (typeof taskVM !== "undefined") ? taskVM : null
    readonly property var editorRef: (typeof editorService !== "undefined") ? editorService : null
    readonly property var manualEditRef: (typeof manualEditNotifier !== "undefined") ? manualEditNotifier : null
    height: Theme.statusBarHeight
    color: Theme.statusBarBackground

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spaceSm
        anchors.rightMargin: Theme.spaceXs
        spacing: Theme.spaceMd
        Rectangle {
            width: 7; height: 7; radius: 4
            color: root.appRef && root.appRef.sidecarConnected ? Theme.success : Theme.danger
        }
        Text { text: root.appRef && root.appRef.sidecarConnected ? "Sidecar" : "Sidecar 不可用"; color: Theme.statusBarForeground; font.pixelSize: Theme.fontSizeXSmall }
        Text { text: root.workspaceRef && root.workspaceRef.active ? (String(root.workspaceRef.trustState) === "trusted" ? "已信任" : "未信任") : "无工作区"; color: Theme.statusBarForeground; font.pixelSize: Theme.fontSizeXSmall }
        Text { text: root.taskRef && root.taskRef.state ? String(root.taskRef.state) : ""; color: Theme.statusBarForeground; font.pixelSize: Theme.fontSizeXSmall }
        AttributionStatusItem {
            task: root.taskRef
            notifier: root.manualEditRef
            shell: root.shell
        }
        Text {
            Layout.fillWidth: true
            text: root.appRef && root.appRef.unavailableReason ? String(root.appRef.unavailableReason) : ""
            color: Theme.statusBarForeground
            elide: Text.ElideRight
            font.pixelSize: Theme.fontSizeXSmall
        }
        Text {
            visible: root.editorRef && root.editorRef.cursorLine > 0
            text: root.editorRef ? ("行 " + root.editorRef.cursorLine + "，列 " + root.editorRef.cursorColumn) : ""
            color: Theme.statusBarForeground
            font.pixelSize: Theme.fontSizeXSmall
        }
        ToolButton {
            text: "\ue70d"
            font.family: Theme.fontIcon
            ToolTip.visible: hovered
            ToolTip.text: root.shell && root.shell.bottomPanelVisible ? "折叠运行轨迹" : "显示运行轨迹"
            onClicked: if (root.shell) root.shell.toggleBottomPanel()
        }
    }
}
