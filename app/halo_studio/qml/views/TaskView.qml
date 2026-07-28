import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../components/util.js" as Util

// 任务页：左侧创建表单（TaskSpec 只携带用户显式提供的内容），右侧结构化运行轨迹。
RowLayout {
    id: taskPage

    readonly property var tVM: (typeof taskVM !== "undefined") ? taskVM : null
    readonly property var trVM: (typeof traceVM !== "undefined") ? traceVM : null
    readonly property var cfgVM: (typeof configVM !== "undefined") ? configVM : null
    readonly property var rtVM: (typeof runtimeVM !== "undefined") ? runtimeVM : null

    spacing: 10

    SectionCard {
        Layout.preferredWidth: 420
        Layout.fillHeight: true
        title: "创建 Agent 任务"

        RowLayout {
            spacing: 8
            Text { text: "主 Agent："; color: Theme.textDim; font.pixelSize: 12 }
            RadioButton { id: agentPi; text: "Pi"; checked: true }
            RadioButton { id: agentOpenCode; text: "OpenCode" }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Text { text: "启动配置："; color: Theme.textDim; font.pixelSize: 12 }
            ComboBox {
                id: configSelect
                Layout.fillWidth: true
                textRole: "name"
                valueRole: "configId"
                model: (taskPage.cfgVM !== null && taskPage.cfgVM.configs !== undefined) ? taskPage.cfgVM.configs : null
            }
            Button {
                text: "启动运行时"
                enabled: configSelect.currentValue !== undefined && configSelect.currentValue !== null
                onClicked: {
                    if (taskPage.rtVM !== null && taskPage.rtVM.start)
                        taskPage.rtVM.start(agentPi.checked ? "pi" : "opencode", String(configSelect.currentValue))
                }
            }
            Button {
                text: "检查兼容性"
                enabled: configSelect.currentValue !== undefined && configSelect.currentValue !== null
                onClicked: {
                    if (taskPage.rtVM !== null && taskPage.rtVM.probe)
                        taskPage.rtVM.probe(agentPi.checked ? "pi" : "opencode", String(configSelect.currentValue))
                }
            }
        }
        Text {
            Layout.fillWidth: true
            text: {
                if (taskPage.rtVM === null)
                    return "兼容性：—"
                var isOpenCode = agentOpenCode.checked
                var compatibility = isOpenCode ? taskPage.rtVM.opencodeCompatibility : taskPage.rtVM.piCompatibility
                var message = isOpenCode ? taskPage.rtVM.opencodeProbeMessage : taskPage.rtVM.piProbeMessage
                if (compatibility === "not_checked")
                    return "兼容性：尚未检查"
                return "兼容性：" + message
            }
            color: Theme.textDim
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        Text {
            Layout.fillWidth: true
            visible: taskPage.rtVM !== null && (agentOpenCode.checked
                ? Util.hasText(taskPage.rtVM.opencodeReason) : Util.hasText(taskPage.rtVM.piReason))
            text: "运行时原因：" + (agentOpenCode.checked ? taskPage.rtVM.opencodeReason : taskPage.rtVM.piReason)
            color: Theme.danger
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        Text {
            Layout.fillWidth: true
            visible: taskPage.rtVM !== null && (agentOpenCode.checked
                ? Util.hasText(taskPage.rtVM.opencodeRecoveryHint) : Util.hasText(taskPage.rtVM.piRecoveryHint))
            text: "恢复建议：" + (agentOpenCode.checked ? taskPage.rtVM.opencodeRecoveryHint : taskPage.rtVM.piRecoveryHint)
            color: Theme.warn
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
        ErrorLabel { Layout.fillWidth: true; vm: taskPage.rtVM }
        Text { text: "任务标题（选填，默认取目标首行）"; color: Theme.textDim; font.pixelSize: 12 }
        TextField {
            id: titleInput
            Layout.fillWidth: true
            placeholderText: "如：修复登录超时"
        }
        Text { text: "任务目标（必填）"; color: Theme.textDim; font.pixelSize: 12 }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 88
            TextArea {
                id: goalInput
                wrapMode: TextArea.Wrap
                placeholderText: "描述本次有限编码任务的目标…"
            }
        }
        Text { text: "选取文件（每行一个，选填）"; color: Theme.textDim; font.pixelSize: 12 }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 64
            TextArea {
                id: filesInput
                wrapMode: TextArea.NoWrap
                font.family: Theme.monoFont
                placeholderText: "src/auth.rs"
            }
        }
        Text { text: "已有 Diff（选填）"; color: Theme.textDim; font.pixelSize: 12 }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 64
            TextArea {
                id: baseDiffInput
                wrapMode: TextArea.NoWrap
                font.family: Theme.monoFont
                placeholderText: "粘贴已有 Diff（可空）"
            }
        }
        Text { text: "补充说明（选填）"; color: Theme.textDim; font.pixelSize: 12 }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            TextArea {
                id: notesInput
                wrapMode: TextArea.Wrap
                placeholderText: "补充说明（可空）"
            }
        }
        Button {
            text: "创建任务"
            enabled: goalInput.text.trim().length > 0
            onClicked: {
                if (taskPage.tVM === null || !taskPage.tVM.create)
                    return
                var goal = goalInput.text
                var explicitTitle = titleInput.text.trim()
                var lines = filesInput.text.split("\n")
                var files = []
                for (var i = 0; i < lines.length; ++i) {
                    var line = lines[i].trim()
                    if (line.length > 0)
                        files.push(line)
                }
                taskPage.tVM.agent = agentPi.checked ? "pi" : "opencode"
                taskPage.tVM.configId = (configSelect.currentValue === undefined || configSelect.currentValue === null)
                    ? "" : String(configSelect.currentValue)
                taskPage.tVM.title = explicitTitle.length > 0 ? explicitTitle : goal.split("\n")[0].trim()
                taskPage.tVM.instructions = goal
                taskPage.tVM.files = files
                taskPage.tVM.baseDiff = baseDiffInput.text
                taskPage.tVM.notes = notesInput.text
                taskPage.tVM.create()
            }
        }
        ErrorLabel { Layout.fillWidth: true; vm: taskPage.tVM }
        Item { Layout.fillHeight: true }
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 10

        SectionCard {
            Layout.fillWidth: true
            Layout.minimumHeight: 160
            Layout.preferredHeight: 220
            Layout.maximumHeight: 280
            title: "活动会话"

            ListView {
                id: sessionList
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: 100
                clip: true
                spacing: 4
                model: taskPage.tVM !== null ? taskPage.tVM.sessionMessages : []
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    required property var modelData

                    width: sessionList.width
                    color: Theme.deep
                    radius: Theme.radius
                    implicitHeight: sessionRow.implicitHeight + 12

                    RowLayout {
                        id: sessionRow
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        spacing: 8

                        StatusBadge {
                            label: Util.sessionRoleLabel(modelData.role)
                            tone: Util.sessionRoleTone(modelData.role, Theme)
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2

                            Text {
                                Layout.fillWidth: true
                                text: Util.textOr(modelData.text, "")
                                color: Theme.text
                                wrapMode: Text.Wrap
                                font.pixelSize: 12
                            }
                            Text {
                                visible: modelData.truncated === true
                                text: "已截断"
                                color: Theme.textDim
                                font.pixelSize: 11
                            }
                        }
                    }
                }
            }
            Text {
                Layout.fillWidth: true
                visible: sessionList.count === 0
                text: "暂无会话记录"
                color: Theme.textDim
                font.pixelSize: 12
            }
        }

        SectionCard {
            Layout.fillWidth: true
            Layout.fillHeight: true
            title: "运行轨迹"

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                StatusBadge {
                    label: taskPage.tVM !== null ? Util.taskStateLabel(taskPage.tVM.state) : "无任务"
                    tone: taskPage.tVM !== null ? Util.taskStateTone(taskPage.tVM.state, Theme) : Theme.neutral
                }
                Text {
                    Layout.fillWidth: true
                    text: taskPage.tVM !== null ? Util.textOr(taskPage.tVM.taskTitle, "") : ""
                    color: Theme.text
                    elide: Text.ElideRight
                    font.pixelSize: 13
                }
                Button {
                    text: "取消任务"
                    enabled: taskPage.tVM !== null && Util.taskIsActive(taskPage.tVM.state)
                    onClicked: if (taskPage.tVM !== null && taskPage.tVM.cancel) taskPage.tVM.cancel()
                }
            }
            Text {
                visible: taskPage.tVM !== null && Util.hasText(taskPage.tVM.cancelMode)
                text: "最终取消方式：" + Util.cancelModeLabel(taskPage.tVM !== null ? taskPage.tVM.cancelMode : "")
                color: Theme.warn
                font.pixelSize: 12
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                TextField {
                    id: manualNoteInput
                    Layout.fillWidth: true
                    placeholderText: "人工介入说明（选填）"
                }
                Button {
                    text: "标记人工介入"
                    enabled: taskPage.tVM !== null && Util.hasText(taskPage.tVM.taskId)
                    onClicked: if (taskPage.tVM !== null && taskPage.tVM.markManualEdit) taskPage.tVM.markManualEdit(manualNoteInput.text)
                }
            }
            ListView {
                id: traceList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 4
                model: taskPage.trVM !== null ? taskPage.trVM : null
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    width: traceList.width
                    color: Theme.deep
                    radius: Theme.radius
                    implicitHeight: traceRow.implicitHeight + 12
                    RowLayout {
                        id: traceRow
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        spacing: 8
                        StatusBadge {
                            label: Util.traceKindLabel(model.kind)
                            tone: Util.traceKindTone(model.kind, Theme)
                        }
                        Text {
                            Layout.fillWidth: true
                            text: Util.textOr(model.text, "")
                            color: Theme.text
                            wrapMode: Text.Wrap
                            font.pixelSize: 12
                        }
                    }
                }
            }
            ErrorLabel { Layout.fillWidth: true; vm: taskPage.trVM }
            Text {
                visible: traceList.count === 0
                text: "暂无运行轨迹。任务运行时这里按序显示阶段、操作请求与验证状态（非原始终端输出）。"
                color: Theme.textDim
                wrapMode: Text.Wrap
                Layout.fillWidth: true
                font.pixelSize: 12
            }
        }
    }
}
