import QtQuick
import QtQuick.Controls
import "../theme"

Item {
    id: root
    required property var task
    required property var notifier
    required property var shell
    objectName: "statusBarDifferentiationSlot"
    implicitWidth: mixedButton.implicitWidth
    implicitHeight: mixedButton.implicitHeight
    visible: root.task !== null && String(root.task.attribution) === "mixed"

    Button {
        id: mixedButton
        anchors.fill: parent
        text: "归因 Mixed"
        font.pixelSize: Theme.fontSizeXSmall
        ToolTip.visible: hovered
        ToolTip.text: "本任务期间发生人工介入：" + (root.notifier ? root.notifier.manualEditCount : 0) + " 个文件"
        onClicked: if (root.shell) root.shell.showBottomPanel()
        contentItem: Text {
            text: mixedButton.text
            color: Theme.warning
            font.pixelSize: Theme.fontSizeXSmall
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle { color: "transparent" }
    }
}
