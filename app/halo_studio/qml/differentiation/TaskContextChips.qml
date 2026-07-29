import QtQuick
import QtQuick.Controls
import "../theme"

Item {
    id: root
    required property var taskContext
    objectName: "taskContextSelectorSlot"
    implicitWidth: parent ? parent.width : 0
    implicitHeight: content.implicitHeight
    visible: root.taskContext !== null && (root.taskContext.fileCount > 0 || root.taskContext.hint.length > 0)

    Column {
        id: content
        width: parent.width
        spacing: Theme.spaceXxs
        Flow {
            width: parent.width
            spacing: Theme.spaceXxs
            Repeater {
                model: root.taskContext === null ? null : root.taskContext.files
                delegate: Rectangle {
                    required property string relPath
                    height: 24
                    width: chipLabel.implicitWidth + removeButton.width + Theme.spaceSm
                    color: Theme.surfaceBackground
                    border.color: Theme.border
                    radius: Theme.radius
                    Text {
                        id: chipLabel
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.spaceSm
                        anchors.right: removeButton.left
                        anchors.rightMargin: Theme.spaceXxs
                        anchors.verticalCenter: parent.verticalCenter
                        text: relPath
                        color: Theme.foreground
                        elide: Text.ElideMiddle
                        font.pixelSize: Theme.fontSizeXSmall
                    }
                    ToolButton {
                        id: removeButton
                        anchors.right: parent.right
                        anchors.rightMargin: 1
                        anchors.verticalCenter: parent.verticalCenter
                        width: 22
                        height: 22
                        text: "\ue711"
                        font.family: Theme.fontIcon
                        ToolTip.visible: hovered
                        ToolTip.text: "移除任务上下文文件"
                        onClicked: if (root.taskContext) root.taskContext.removeFile(relPath)
                    }
                }
            }
        }
        Text {
            width: parent.width
            visible: root.taskContext !== null && root.taskContext.hint.length > 0
            text: root.taskContext ? root.taskContext.hint : ""
            color: Theme.warning
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeXSmall
        }
    }
}
