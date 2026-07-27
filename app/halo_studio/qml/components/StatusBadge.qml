import QtQuick
import "."

// 状态徽章：文字 + 同色系描边底色。
Rectangle {
    id: badge

    property string label: ""
    property color tone: Theme.neutral

    color: Qt.rgba(tone.r, tone.g, tone.b, 0.16)
    border.color: badge.tone
    border.width: 1
    radius: height / 2
    implicitHeight: badgeText.implicitHeight + 8
    implicitWidth: badgeText.implicitWidth + 18

    Text {
        id: badgeText
        anchors.centerIn: parent
        text: badge.label
        color: badge.tone
        font.pixelSize: 12
    }
}
