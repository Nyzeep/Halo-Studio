import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../../theme"

Rectangle {
    id: root
    required property var shell
    readonly property var explorerRef: (typeof explorerVM !== "undefined") ? explorerVM : null
    readonly property var taskContextRef: (typeof taskContextVM !== "undefined") ? taskContextVM : null
    color: Theme.sideBarBackground

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.spaceSm
        spacing: Theme.spaceXs

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceXxs
            ToolButton {
                text: "\ue8b3"
                font.family: Theme.fontIcon
                enabled: root.explorerRef !== null && root.explorerRef.workspaceTrusted
                ToolTip.visible: hovered
                ToolTip.text: "新建文件"
                onClicked: newFileDialog.open()
            }
            ToolButton {
                text: "\ue8a5"
                font.family: Theme.fontIcon
                enabled: root.explorerRef !== null && root.explorerRef.workspaceTrusted
                ToolTip.visible: hovered
                ToolTip.text: "新建文件夹"
                onClicked: newFolderDialog.open()
            }
            Item { Layout.fillWidth: true }
            ToolButton {
                text: "\ue777"
                font.family: Theme.fontIcon
                enabled: root.explorerRef !== null && root.explorerRef.workspaceTrusted
                ToolTip.visible: hovered
                ToolTip.text: "刷新资源管理器"
                onClicked: root.explorerRef.refresh()
            }
            ToolButton {
                text: "\uec37"
                font.family: Theme.fontIcon
                enabled: root.explorerRef !== null && root.explorerRef.workspaceTrusted
                ToolTip.visible: hovered
                ToolTip.text: "全部折叠"
                onClicked: tree.collapseAll()
            }
        }

        ListView {
            id: tree
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.explorerRef === null ? null : root.explorerRef.model
            ScrollBar.vertical: ScrollBar {}
            delegate: ExplorerRow {
                width: tree.width
                explorer: root.explorerRef
                onContextRequested: function(path, isDirectory, x, y) {
                    contextMenu.relPath = path
                    contextMenu.isDirectory = isDirectory
                    contextMenu.popup(x, y)
                }
            }

            function collapseAll() {
                if (!root.explorerRef)
                    return
                for (var row = 0; row < count; ++row) {
                    var item = root.explorerRef.model.get(row)
                    if (item.kind === "dir" && item.expanded)
                        root.explorerRef.collapse(item.relPath)
                }
            }
        }

        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            visible: root.explorerRef === null || !root.explorerRef.workspaceActive || !root.explorerRef.workspaceTrusted || tree.count === 0
            text: root.explorerRef === null || !root.explorerRef.workspaceActive ? "尚未打开工作区" :
                  (!root.explorerRef.workspaceTrusted ? "需要信任工作区后才能浏览文件" : "此文件夹为空")
            color: Theme.descriptionForeground
            font.pixelSize: Theme.fontSizeSmall
        }
    }

    Dialog {
        id: newFileDialog
        modal: true
        title: "新建文件"
        standardButtons: Dialog.Ok | Dialog.Cancel
        property string name: nameField.text
        onAccepted: if (root.explorerRef) root.explorerRef.createFile("", name)
        TextField {
            id: nameField
            width: 280
            placeholderText: "文件名"
            selectByMouse: true
        }
    }

    Dialog {
        id: newFolderDialog
        modal: true
        title: "新建文件夹"
        standardButtons: Dialog.Ok | Dialog.Cancel
        property string name: folderNameField.text
        onAccepted: if (root.explorerRef) root.explorerRef.createDir("", name)
        TextField {
            id: folderNameField
            width: 280
            placeholderText: "文件夹名"
            selectByMouse: true
        }
    }

    Menu {
        id: contextMenu
        property string relPath: ""
        property bool isDirectory: false
        MenuItem {
            text: "在编辑器中打开"
            visible: !contextMenu.isDirectory
            onTriggered: if (root.explorerRef) root.explorerRef.openPinned(contextMenu.relPath)
        }
        MenuItem {
            text: "加入任务上下文"
            visible: !contextMenu.isDirectory && root.taskContextRef !== null
            onTriggered: if (root.taskContextRef) root.taskContextRef.addFile(contextMenu.relPath)
        }
        MenuItem {
            text: "在系统资源管理器中显示"
            onTriggered: if (root.explorerRef) root.explorerRef.revealInSystem(contextMenu.relPath)
        }
    }
}
