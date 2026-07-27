import QtQuick
import QtQuick.Layouts
import "."

// 暗色卡片容器：标题 + 内容列。使用方的子项经默认属性进入内容列。
Rectangle {
    id: card

    default property alias content: inner.data
    property string title: ""

    color: Theme.surface
    radius: Theme.radius
    border.color: Theme.border
    border.width: 1
    implicitWidth: outer.implicitWidth + 24
    implicitHeight: outer.implicitHeight + 24

    ColumnLayout {
        id: outer
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Text {
            visible: card.title.length > 0
            text: card.title
            color: Theme.text
            font.bold: true
            font.pixelSize: 14
        }

        ColumnLayout {
            id: inner
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 6
        }
    }
}
