import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../theme"
import "panels"

Rectangle {
    id: host
    required property var shell
    color: Theme.sideBarBackground
    border.color: Theme.border
    border.width: 1

    readonly property string title: shell && shell.activeSideBarPanel === "task" ? "工作区与任务" : "资源管理器"

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.sideBarHeaderHeight
            color: Theme.sideBarBackground
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spaceMd
                anchors.rightMargin: Theme.spaceXs
                Text { Layout.fillWidth: true; text: host.title; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.fontSizeSmall }
                ToolButton {
                    text: "\ue73e"
                    font.family: Theme.fontIcon
                    ToolTip.visible: hovered
                    ToolTip.text: "折叠侧栏"
                    onClicked: if (host.shell) host.shell.toggleSideBar()
                }
            }
        }
        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: host.shell && host.shell.activeSideBarPanel === "task" ? 1 : 0
            ExplorerPanel { shell: host.shell }
            TaskPanel {}
        }
    }
}
