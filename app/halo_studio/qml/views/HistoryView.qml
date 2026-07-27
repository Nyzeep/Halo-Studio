import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../components/util.js" as Util

// 历史页：本地交付历史（任务 + 结论），只读列表。
RowLayout {
    id: historyPage

    readonly property var hVM: (typeof historyVM !== "undefined") ? historyVM : null
    readonly property var decisionEntries: (hVM !== null && hVM.decisions !== undefined) ? Util.listOr(hVM.decisions) : []

    spacing: 10

    SectionCard {
        Layout.fillWidth: true
        Layout.fillHeight: true
        title: "任务历史"

        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button {
                text: "刷新"
                onClicked: if (historyPage.hVM !== null && historyPage.hVM.list) historyPage.hVM.list(50)
            }
        }
        ListView {
            id: taskHistoryList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 4
            model: (historyPage.hVM !== null && historyPage.hVM.tasks !== undefined) ? historyPage.hVM.tasks : null
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                width: taskHistoryList.width
                radius: Theme.radius
                color: Theme.deep
                implicitHeight: 48
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8
                    StatusBadge { label: Util.agentLabel(model.agent); tone: Theme.accent }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: Util.textOr(model.title, "（无标题）")
                            color: Theme.text
                            elide: Text.ElideRight
                            font.pixelSize: 12
                        }
                        Text {
                            Layout.fillWidth: true
                            text: Util.textOr(model.createdAt, "")
                            color: Theme.textDim
                            elide: Text.ElideRight
                            font.pixelSize: 11
                        }
                    }
                    StatusBadge {
                        label: Util.taskStateLabel(model.state)
                        tone: Util.taskStateTone(model.state, Theme)
                    }
                }
            }
        }
        Text {
            visible: taskHistoryList.count === 0
            text: "暂无任务记录"
            color: Theme.textDim
            font.pixelSize: 12
        }
        ErrorLabel { Layout.fillWidth: true; vm: historyPage.hVM }
    }

    SectionCard {
        Layout.fillWidth: true
        Layout.fillHeight: true
        title: "交付结论"

        ListView {
            id: decisionList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 4
            model: historyPage.decisionEntries
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                width: decisionList.width
                radius: Theme.radius
                color: Theme.deep
                implicitHeight: decisionRow.implicitHeight + 12
                RowLayout {
                    id: decisionRow
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8
                    StatusBadge {
                        label: Util.decisionLabel(modelData ? modelData.kind : undefined)
                        tone: modelData && String(modelData.kind) === "accepted" ? Theme.ok : Theme.danger
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: modelData
                                  ? (Util.textOr(modelData.task_id, "") + "　证据 v" + Util.textOr(modelData.evidence_version, "—"))
                                  : ""
                            color: Theme.text
                            elide: Text.ElideMiddle
                            font.pixelSize: 12
                        }
                        Text {
                            Layout.fillWidth: true
                            text: modelData
                                  ? (Util.textOr(modelData.decided_at, "")
                                     + (Util.hasText(modelData.reason) ? "　原因：" + modelData.reason : ""))
                                  : ""
                            color: Theme.textDim
                            elide: Text.ElideRight
                            font.pixelSize: 11
                        }
                    }
                }
            }
        }
        Text {
            visible: decisionList.count === 0
            text: "暂无结论记录"
            color: Theme.textDim
            font.pixelSize: 12
        }
    }
}
