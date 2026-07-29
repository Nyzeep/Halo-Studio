import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../../theme"
import "../../components/util.js" as Util
import "../../differentiation"

ScrollView {
    id: root
    readonly property var wsRef: (typeof workspaceVM !== "undefined") ? workspaceVM : null
    readonly property var runtimeRef: (typeof runtimeVM !== "undefined") ? runtimeVM : null
    readonly property var taskRef: (typeof taskVM !== "undefined") ? taskVM : null
    readonly property var configRef: (typeof configVM !== "undefined") ? configVM : null
    readonly property var contextRef: (typeof taskContextVM !== "undefined") ? taskContextVM : null
    clip: true
    contentWidth: availableWidth

    ColumnLayout {
        width: root.availableWidth
        spacing: Theme.spaceMd

        Label { text: "工作区"; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.fontSizeSmall }
        TextField {
            id: workspacePath
            Layout.fillWidth: true
            placeholderText: "Git 仓库路径"
            color: Theme.foreground
            background: Rectangle { color: Theme.inputBackground; border.color: activeFocus ? Theme.focusBorder : Theme.inputBorder; radius: Theme.radius }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceXs
            Button {
                text: "打开"
                enabled: workspacePath.text.trim().length > 0
                onClicked: if (root.wsRef && root.wsRef.open) root.wsRef.open(workspacePath.text.trim())
            }
            Button {
                text: "信任"
                enabled: root.wsRef && root.wsRef.active && String(root.wsRef.trustState) !== "trusted"
                onClicked: if (root.wsRef && root.wsRef.trust) root.wsRef.trust()
            }
            Item { Layout.fillWidth: true }
            ToolButton {
                text: "\ue72c"
                font.family: Theme.fontIcon
                ToolTip.visible: hovered
                ToolTip.text: "刷新运行时状态"
                onClicked: if (root.runtimeRef && root.runtimeRef.refresh) root.runtimeRef.refresh()
            }
        }
        Text {
            Layout.fillWidth: true
            text: root.wsRef && root.wsRef.active ? String(root.wsRef.realPath) : "未选择工作区"
            color: Theme.descriptionForeground
            elide: Text.ElideMiddle
            font.pixelSize: Theme.fontSizeXSmall
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }
        Label { text: "新建 Agent 任务"; color: Theme.foreground; font.bold: true; font.pixelSize: Theme.fontSizeSmall }
        RowLayout {
            Layout.fillWidth: true
            RadioButton { id: pi; text: "Pi"; checked: true }
            RadioButton { id: opencode; text: "OpenCode" }
        }
        ComboBox {
            id: configSelect
            Layout.fillWidth: true
            textRole: "name"
            valueRole: "configId"
            model: root.configRef && root.configRef.configs !== undefined ? root.configRef.configs : null
        }
        TextField {
            id: title
            Layout.fillWidth: true
            placeholderText: "任务标题"
            color: Theme.foreground
            background: Rectangle { color: Theme.inputBackground; border.color: activeFocus ? Theme.focusBorder : Theme.inputBorder; radius: Theme.radius }
        }
        TextArea {
            id: instructions
            Layout.fillWidth: true
            Layout.preferredHeight: 116
            placeholderText: "任务目标"
            wrapMode: TextArea.Wrap
            color: Theme.foreground
            background: Rectangle { color: Theme.inputBackground; border.color: instructions.activeFocus ? Theme.focusBorder : Theme.inputBorder; radius: Theme.radius }
        }
        TaskContextChips {
            Layout.fillWidth: true
            taskContext: root.contextRef
        }
        TextArea {
            id: notes
            Layout.fillWidth: true
            Layout.preferredHeight: 76
            placeholderText: "补充说明（可由编辑器选区加入）"
            wrapMode: TextArea.Wrap
            color: Theme.foreground
            background: Rectangle { color: Theme.inputBackground; border.color: notes.activeFocus ? Theme.focusBorder : Theme.inputBorder; radius: Theme.radius }
        }
        Button {
            Layout.fillWidth: true
            text: "创建任务"
            enabled: root.taskRef && instructions.text.trim().length > 0
            onClicked: {
                if (!root.taskRef || !root.taskRef.create)
                    return
                root.taskRef.agent = pi.checked ? "pi" : "opencode"
                root.taskRef.configId = configSelect.currentValue === undefined || configSelect.currentValue === null ? "" : String(configSelect.currentValue)
                root.taskRef.title = title.text.trim().length > 0 ? title.text.trim() : instructions.text.split("\n")[0].trim()
                root.taskRef.instructions = instructions.text
                root.taskRef.files = root.contextRef && root.contextRef.filesList ? root.contextRef.filesList() : []
                root.taskRef.baseDiff = ""
                root.taskRef.notes = notes.text
                root.taskRef.create()
            }
        }
        Text {
            Layout.fillWidth: true
            visible: root.taskRef && Util.hasText(root.taskRef.errorMessage)
            text: root.taskRef ? root.taskRef.errorMessage : ""
            color: Theme.danger
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeXSmall
        }
    }

    Connections {
        target: root.contextRef
        function onNotesBlockAppended(block) {
            if (notes.text.length > 0 && !notes.text.endsWith("\n"))
                notes.text += "\n\n"
            notes.text += block
        }
        function onDraftCleared() {
            notes.text = ""
        }
    }
    Connections {
        target: root.taskRef
        function onTaskCreated() {
            if (root.contextRef) root.contextRef.clear()
        }
    }
}
