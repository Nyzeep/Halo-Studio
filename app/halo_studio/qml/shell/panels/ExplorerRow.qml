import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../../theme"

Rectangle {
    id: root
    required property var explorer
    signal contextRequested(string path, bool isDirectory, real x, real y)
    property string entryPath: model.relPath
    property bool directory: model.kind === "dir"
    property bool directoryExpanded: model.expanded
    property bool directoryLoading: model.loading
    height: Theme.explorerRowHeight
    color: pointer.containsMouse ? Theme.listHoverBackground : Theme.transparentBackground

    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: function(mouse) {
            if (mouse.button === Qt.RightButton) {
                root.contextRequested(root.entryPath, root.directory, mouse.x, mouse.y)
                return
            }
            if (root.directory) {
                if (root.directoryExpanded)
                    root.explorer.collapse(root.entryPath)
                else
                    root.explorer.expand(root.entryPath)
            } else {
                root.explorer.openPreview(root.entryPath)
            }
        }
        onDoubleClicked: function(mouse) {
            if (mouse.button === Qt.LeftButton && !root.directory)
                root.explorer.openPinned(root.entryPath)
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spaceXs
        anchors.rightMargin: Theme.spaceXs
        spacing: Theme.spaceXxs
        Item { Layout.preferredWidth: model.level * Theme.treeIndentWidth }
        Text {
            Layout.preferredWidth: Theme.fontSizeMedium
            horizontalAlignment: Text.AlignHCenter
            text: root.directory ? (root.directoryLoading ? "..." : (root.directoryExpanded ? "v" : ">")) : ""
            color: Theme.mutedForeground
            font.pixelSize: Theme.fontSizeSmall
        }
        Text {
            text: root.directory ? "\ue8b7" : "\ue8a5"
            font.family: Theme.fontIcon
            color: root.directory ? Theme.accent : Theme.mutedForeground
            font.pixelSize: Theme.fontSizeSmall
        }
        Text {
            Layout.fillWidth: true
            text: model.name
            elide: Text.ElideRight
            color: model.readonly ? Theme.descriptionForeground : Theme.foreground
            font.pixelSize: Theme.fontSizeSmall
        }
        Text {
            visible: model.badgeLetter !== ""
            text: model.badgeLetter
            color: Theme[model.badgeColorToken] || Theme.mutedForeground
            font.pixelSize: Theme.fontSizeXSmall
            ToolTip.visible: badgePointer.containsMouse
            ToolTip.text: model.badgeTooltip
            MouseArea { id: badgePointer; anchors.fill: parent; hoverEnabled: true }
        }
    }
}
