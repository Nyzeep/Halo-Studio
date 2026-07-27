import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../theme"

Popup {
    id: root
    required property var palette
    readonly property var resultsRef: root.palette && root.palette.results ? root.palette.results : null
    modal: false
    focus: true
    width: Math.min(600, parent ? parent.width * 0.60 : 600)
    height: Math.min(440, parent ? parent.height * 0.68 : 440)
    x: parent ? Math.max(0, (parent.width - width) / 2) : 0
    y: parent ? Math.max(0, parent.height * 0.08) : 0
    visible: root.palette && root.palette.visible
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    padding: Theme.spaceSm

    onVisibleChanged: {
        if (visible)
            queryField.forceActiveFocus()
        else if (root.palette && root.palette.visible)
            root.palette.close()
    }

    background: Rectangle {
        color: Theme.surfaceBackground
        border.color: Theme.focusBorder
        border.width: 1
        radius: Theme.radius
    }

    contentItem: ColumnLayout {
        spacing: Theme.spaceSm
        TextField {
            id: queryField
            Layout.fillWidth: true
            placeholderText: "搜索文件（输入 > 搜索命令）"
            text: root.palette ? root.palette.query : ""
            color: Theme.foreground
            font.pixelSize: Theme.fontSizeMedium
            selectByMouse: true
            onTextEdited: if (root.palette) root.palette.setQuery(text)
            Keys.onPressed: function(event) {
                if (!root.palette)
                    return
                if (event.key === Qt.Key_Down) {
                    root.palette.moveSelection(1)
                    event.accepted = true
                } else if (event.key === Qt.Key_Up) {
                    root.palette.moveSelection(-1)
                    event.accepted = true
                } else if (event.key === Qt.Key_PageDown) {
                    root.palette.moveSelection(10)
                    event.accepted = true
                } else if (event.key === Qt.Key_PageUp) {
                    root.palette.moveSelection(-10)
                    event.accepted = true
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    root.palette.acceptSelected()
                    event.accepted = true
                }
            }
            background: Rectangle {
                color: Theme.inputBackground
                border.color: Theme.inputBorder
                border.width: 1
                radius: Theme.radius
            }
        }
        ListView {
            id: resultsList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.resultsRef
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                id: rowRoot
                required property string label
                required property string description
                required property string group
                required property int index
                width: resultsList.width
                height: 42
                color: root.palette && root.palette.selectedIndex === index ? Theme.listActiveSelectionBackground : (pointer.containsMouse ? Theme.listHoverBackground : Theme.transparentBackground)
                MouseArea {
                    id: pointer
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: {
                        root.palette.moveSelection(index - root.palette.selectedIndex)
                        root.palette.acceptSelected()
                    }
                }
                ColumnLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.spaceSm
                    anchors.rightMargin: Theme.spaceSm
                    spacing: Theme.spaceXxs
                    Text { Layout.fillWidth: true; text: rowRoot.label; color: Theme.foreground; elide: Text.ElideRight; font.pixelSize: Theme.fontSizeSmall }
                    Text { Layout.fillWidth: true; text: rowRoot.description; color: Theme.descriptionForeground; elide: Text.ElideRight; font.pixelSize: Theme.fontSizeXSmall }
                }
            }
        }
        Text {
            Layout.fillWidth: true
            visible: root.palette && (root.palette.busy || root.palette.hint !== "")
            text: root.palette && root.palette.busy ? "正在建立文件索引" : (root.palette ? root.palette.hint : "")
            color: Theme.descriptionForeground
            font.pixelSize: Theme.fontSizeSmall
        }
    }
}
