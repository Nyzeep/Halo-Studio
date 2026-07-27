import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../theme"
import "panels"

Rectangle {
    id: root
    required property var shell
    color: Theme.panelBackground
    border.color: Theme.border
    border.width: 1

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.sideBarHeaderHeight
            color: Theme.surfaceBackground
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spaceMd
                anchors.rightMargin: Theme.spaceXs
                Text { Layout.fillWidth: true; text: "运行轨迹"; color: Theme.foreground; font.pixelSize: Theme.fontSizeSmall }
                ToolButton {
                    text: "\ue70d"
                    font.family: Theme.fontIcon
                    ToolTip.visible: hovered
                    ToolTip.text: "折叠运行轨迹"
                    onClicked: if (root.shell) root.shell.toggleBottomPanel()
                }
            }
        }
        TracePanel { Layout.fillWidth: true; Layout.fillHeight: true; Layout.margins: Theme.spaceSm }
    }
}
