import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "components"
import "components/util.js" as Util
import "views"

// Halo Studio 主窗口：原生暗色桌面工作台。
// 红线：全界面无编辑器、无终端、无浏览器组件。
ApplicationWindow {
    id: root

    readonly property var appRef: (typeof appVM !== "undefined") ? appVM : null

    visible: true
    width: 1280
    height: 800
    minimumWidth: 1024
    minimumHeight: 640
    title: "Halo Studio —— Pi / OpenCode 可验证编码交付工作台"
    color: Theme.background

    palette.window: Theme.background
    palette.windowText: Theme.text
    palette.base: Theme.deep
    palette.alternateBase: Theme.surface
    palette.text: Theme.text
    palette.button: Theme.surfaceAlt
    palette.buttonText: Theme.text
    palette.highlight: Theme.accent
    palette.highlightedText: "#ffffff"
    palette.placeholderText: Theme.textDim
    palette.mid: Theme.border
    palette.dark: Theme.border
    palette.light: Theme.surfaceAlt

    RowLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 10

        SidebarPane {
            Layout.preferredWidth: 330
            Layout.maximumWidth: 360
            Layout.fillHeight: true
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 8

            TabBar {
                id: mainTabs
                Layout.fillWidth: true
                TabButton { text: "任务" }
                TabButton { text: "审查" }
                TabButton { text: "配置" }
                TabButton { text: "历史" }
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: mainTabs.currentIndex

                TaskView {}
                ReviewView {
                    onHandoffRequested: function(taskId, selectedFiles) {
                        handoffDialog.handoffTaskId = taskId
                        handoffDialog.handoffSelectedFiles = selectedFiles
                        handoffDialog.open()
                    }
                }
                ConfigView {}
                HistoryView {}
            }
        }
    }

    HandoffDialog { id: handoffDialog }

    // 底部状态条：Sidecar 连接状态、协议版本、不可用原因（常显）。
    footer: Rectangle {
        height: 32
        color: Theme.surface

        Rectangle {
            anchors.top: parent.top
            width: parent.width
            height: 1
            color: Theme.border
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 16

            Rectangle {
                width: 9
                height: 9
                radius: 4.5
                color: root.appRef !== null && root.appRef.sidecarConnected === true ? Theme.ok : Theme.danger
            }
            Text {
                text: "Sidecar：" + Util.connectionLabel(root.appRef !== null ? root.appRef.sidecarConnected : undefined)
                color: Theme.text
                font.pixelSize: 12
            }
            Text {
                text: "协议版本：" + (root.appRef !== null && root.appRef.protocolVersion > 0
                                     ? "v" + root.appRef.protocolVersion : "—")
                color: Theme.textDim
                font.pixelSize: 12
            }
            Text {
                Layout.fillWidth: true
                text: "不可用原因：" + Util.textOr(root.appRef !== null ? root.appRef.unavailableReason : undefined, "—")
                color: Theme.textDim
                elide: Text.ElideRight
                font.pixelSize: 12
            }
        }
    }
}
