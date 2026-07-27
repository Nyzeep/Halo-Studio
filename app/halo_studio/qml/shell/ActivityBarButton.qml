import QtQuick
import QtQuick.Controls
import "../theme"

ToolButton {
    id: control
    required property string entryId
    required property string iconGlyph
    required property string tooltipText
    property bool active: false
    property bool badgeVisible: false

    width: Theme.activityBarWidth
    height: Theme.activityBarWidth
    padding: 0

    contentItem: Text {
        text: control.iconGlyph
        color: control.active ? Theme.activityBarForeground : Theme.activityBarInactiveForeground
        font.family: Theme.fontIcon
        font.pixelSize: 19
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    background: Rectangle {
        color: control.hovered ? Theme.ghostElementHoverBackground : Theme.transparentBackground
        Rectangle {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: 2
            height: parent.height - Theme.spaceSm
            visible: control.active
            color: Theme.activityBarActiveBorder
        }
    }
    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.rightMargin: Theme.spaceXs
        anchors.topMargin: Theme.spaceXs
        width: 7
        height: 7
        radius: 4
        visible: control.badgeVisible
        color: Theme.warning
    }
    ToolTip.visible: hovered
    ToolTip.text: tooltipText
    ToolTip.delay: 450
}
