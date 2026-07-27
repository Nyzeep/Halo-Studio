import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../components/util.js" as Util

// 交接对话框：预览交接包内容（目标 / 摘要 / 选定变更 / 验证）并选择目标 Agent 后确认创建。
Dialog {
    id: handoffDialog

    readonly property var hVM: (typeof handoffVM !== "undefined") ? handoffVM : null
    property string handoffTaskId: ""
    property var handoffSelectedFiles: []

    title: "创建交接包"
    modal: true
    width: 620
    parent: Overlay.overlay
    anchors.centerIn: parent

    onOpened: {
        if (hVM !== null && hVM.preview && handoffTaskId.length > 0)
            hVM.preview(handoffTaskId, handoffSelectedFiles)
    }

    contentItem: ColumnLayout {
        spacing: 8

        Text {
            Layout.fillWidth: true
            text: "交接包只包含任务目标、主 Agent 摘要、选定文件变更与验证结果；不包含完整对话、原始工具日志、凭据或配置文件。"
            color: Theme.textDim
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        Text { text: "任务目标"; color: Theme.text; font.bold: true; font.pixelSize: 12 }
        Text {
            Layout.fillWidth: true
            text: Util.textOr(handoffDialog.hVM !== null ? handoffDialog.hVM.goal : undefined, "—")
            color: Theme.text
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        Text { text: "主 Agent 摘要"; color: Theme.text; font.bold: true; font.pixelSize: 12 }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 110
            TextArea {
                readOnly: true
                text: Util.textOr(handoffDialog.hVM !== null ? handoffDialog.hVM.summary : undefined, "")
                wrapMode: TextArea.Wrap
                placeholderText: "暂无摘要"
                placeholderTextColor: Theme.textDim
                color: Theme.text
                font.pixelSize: 12
                background: Rectangle {
                    color: Theme.deep
                    border.color: Theme.border
                    radius: Theme.radius
                }
            }
        }
        Text { text: "选定变更"; color: Theme.text; font.bold: true; font.pixelSize: 12 }
        ListView {
            id: changesList
            Layout.fillWidth: true
            Layout.preferredHeight: 96
            clip: true
            model: (handoffDialog.hVM !== null && handoffDialog.hVM.selectedChanges !== undefined)
                   ? Util.listOr(handoffDialog.hVM.selectedChanges) : []
            ScrollBar.vertical: ScrollBar {}
            delegate: Text {
                width: changesList.width
                text: "· " + (modelData ? Util.textOr(modelData.path, "") : "")
                color: Theme.text
                elide: Text.ElideMiddle
                font.pixelSize: 12
            }
        }
        Text {
            visible: changesList.count === 0
            text: "（无选定变更）"
            color: Theme.textDim
            font.pixelSize: 12
        }
        RowLayout {
            spacing: 8
            Text { text: "验证："; color: Theme.textDim; font.pixelSize: 12 }
            StatusBadge {
                label: Util.verificationLabel(handoffDialog.hVM !== null ? handoffDialog.hVM.verificationStatus : undefined)
                tone: Util.verificationTone(handoffDialog.hVM !== null ? handoffDialog.hVM.verificationStatus : undefined, Theme)
            }
            Text {
                Layout.fillWidth: true
                text: Util.textOr(handoffDialog.hVM !== null ? handoffDialog.hVM.verificationDetail : undefined, "")
                color: Theme.textDim
                elide: Text.ElideRight
                font.pixelSize: 12
            }
        }
        RowLayout {
            spacing: 8
            Text { text: "目标 Agent："; color: Theme.textDim; font.pixelSize: 12 }
            RadioButton { id: handoffTargetPi; text: "Pi" }
            RadioButton { id: handoffTargetOc; text: "OpenCode"; checked: true }
        }
        Text {
            Layout.fillWidth: true
            visible: handoffDialog.hVM !== null && Util.hasText(handoffDialog.hVM.handoffId)
            text: handoffDialog.hVM !== null ? ("已创建交接包：" + Util.textOr(handoffDialog.hVM.handoffId, "")) : ""
            color: Theme.ok
            font.pixelSize: 12
        }
        ErrorLabel { Layout.fillWidth: true; vm: handoffDialog.hVM }
    }

    footer: DialogButtonBox {
        Button {
            text: "确认创建"
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
        Button {
            text: "取消"
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }
    }

    onAccepted: {
        if (hVM !== null && hVM.create)
            hVM.create(handoffTargetPi.checked ? "pi" : "opencode")
    }
}
