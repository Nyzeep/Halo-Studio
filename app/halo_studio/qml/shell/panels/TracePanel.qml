import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../../theme"
import "../../components/util.js" as Util

ColumnLayout {
    id: root
    readonly property var taskRef: (typeof taskVM !== "undefined") ? taskVM : null
    readonly property var traceRef: (typeof traceVM !== "undefined") ? traceVM : null
    spacing: Theme.spaceSm

    RowLayout {
        Layout.fillWidth: true
        Text {
            Layout.fillWidth: true
            text: root.taskRef ? Util.textOr(root.taskRef.taskTitle, "") : ""
            color: Theme.foreground
            font.pixelSize: Theme.fontSizeSmall
            elide: Text.ElideRight
        }
        Text {
            text: root.taskRef ? Util.taskStateLabel(root.taskRef.state) : "无任务"
            color: root.taskRef && String(root.taskRef.state) === "failed" ? Theme.danger : Theme.mutedForeground
            font.pixelSize: Theme.fontSizeXSmall
        }
        Button {
            text: "取消"
            enabled: root.taskRef && Util.taskIsActive(root.taskRef.state)
            onClicked: if (root.taskRef && root.taskRef.cancel) root.taskRef.cancel()
        }
    }
    ListView {
        id: traceList
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        spacing: 1
        model: root.traceRef
        ScrollBar.vertical: ScrollBar {}
        delegate: Rectangle {
            width: traceList.width
            implicitHeight: traceText.implicitHeight + Theme.spaceSm
            color: ListView.isCurrentItem ? Theme.listActiveSelectionBackground : Theme.panelBackground
            Text {
                id: traceText
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Theme.spaceSm
                anchors.rightMargin: Theme.spaceSm
                text: model.text || ""
                color: Theme.foreground
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeXSmall
            }
        }
    }
    Text {
        Layout.fillWidth: true
        visible: traceList.count === 0
        text: "暂无运行轨迹"
        color: Theme.descriptionForeground
        font.pixelSize: Theme.fontSizeXSmall
    }
}
