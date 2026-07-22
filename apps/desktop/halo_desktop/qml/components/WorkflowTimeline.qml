import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var events: []
    property string currentAgentId: ""
    property color panelColor: "#b81c202c"
    property color cardColor: "#cc151821"
    property color borderColor: "#1affffff"
    property color textPrimary: "#f3f5f8"
    property color textMuted: "#9aa3b2"
    property color accent: "#8b5cf6"
    property color cyan: "#22d3ee"
    property color success: "#22c55e"
    property color warning: "#f59e0b"

    Rectangle {
        anchors.fill: parent
        color: root.panelColor
        radius: 10
        border.color: root.borderColor
        border.width: 1
    }

    Column {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 12

        Row {
            width: parent.width
            height: 30
            spacing: 8

            Text {
                text: "Workspace"
                color: root.textPrimary
                font.pixelSize: 17
                font.weight: Font.Medium
                anchors.verticalCenter: parent.verticalCenter
            }

            Rectangle {
                width: 92
                height: 24
                radius: 12
                color: "#1f2937"
                border.color: root.borderColor
                anchors.verticalCenter: parent.verticalCenter

                Text {
                    anchors.centerIn: parent
                    text: "Phase 1"
                    color: root.cyan
                    font.pixelSize: 12
                }
            }
        }

        ListView {
            id: eventList
            width: parent.width
            height: parent.height - 42
            spacing: 10
            clip: true
            model: root.events

            delegate: Rectangle {
                width: eventList.width
                height: modelData.agentId === root.currentAgentId ? Math.max(72, bodyColumn.implicitHeight + 22) : 0
                visible: modelData.agentId === root.currentAgentId
                radius: 10
                color: modelData.role === "user" ? "#3322d3ee" : root.cardColor
                border.color: eventAccent(modelData.kind)
                border.width: 1

                Row {
                    id: bodyColumn
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.margins: 12
                    spacing: 12

                    Rectangle {
                        width: 10
                        height: 10
                        radius: 5
                        color: eventAccent(modelData.kind)
                        anchors.top: parent.top
                        anchors.topMargin: 6
                    }

                    Column {
                        width: parent.width - 24
                        spacing: 6

                        Text {
                            text: modelData.title
                            color: root.textPrimary
                            font.pixelSize: 14
                            font.weight: Font.Medium
                            width: parent.width
                            wrapMode: Text.Wrap
                        }

                        Text {
                            text: modelData.body
                            color: root.textMuted
                            font.pixelSize: 13
                            width: parent.width
                            wrapMode: Text.Wrap
                            visible: text.length > 0
                        }

                        Text {
                            text: modelData.kind + "  #" + modelData.seq
                            color: root.textMuted
                            font.pixelSize: 11
                            font.family: "Cascadia Mono"
                        }
                    }
                }
            }
        }
    }

    function eventAccent(kind) {
        if (kind === "tool.started" || kind === "tool.completed")
            return root.cyan
        if (kind === "token.updated" || kind === "message.completed")
            return root.success
        if (kind === "thinking.delta")
            return root.accent
        if (kind === "shell.stdout" || kind === "shell.started")
            return root.warning
        return root.borderColor
    }
}
