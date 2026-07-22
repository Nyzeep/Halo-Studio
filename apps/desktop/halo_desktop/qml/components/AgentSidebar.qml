import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var agents: []
    property string currentAgentId: agents.length > 0 ? agents[0].id : ""
    property color panelColor: "#b81c202c"
    property color selectedColor: "#262b3a"
    property color borderColor: "#1affffff"
    property color textPrimary: "#f3f5f8"
    property color textMuted: "#9aa3b2"
    property color accent: "#8b5cf6"
    signal agentSelected(string agentId)

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
        spacing: 14

        Text {
            text: "Agents"
            color: root.textPrimary
            font.pixelSize: 16
            font.weight: Font.Medium
        }

        ListView {
            id: agentList
            width: parent.width
            height: parent.height - 50
            spacing: 8
            clip: true
            model: root.agents

            delegate: Rectangle {
                width: agentList.width
                height: 58
                radius: 8
                color: modelData.id === root.currentAgentId ? root.selectedColor : "transparent"
                border.color: modelData.id === root.currentAgentId ? root.accent : root.borderColor
                border.width: 1

                Row {
                    anchors.fill: parent
                    anchors.margins: 10
                    spacing: 10

                    Rectangle {
                        width: 9
                        height: 9
                        radius: 5
                        color: modelData.id === root.currentAgentId ? root.accent : root.textMuted
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Column {
                        width: parent.width - 28
                        spacing: 3
                        anchors.verticalCenter: parent.verticalCenter

                        Text {
                            text: modelData.name
                            color: root.textPrimary
                            font.pixelSize: 14
                            elide: Text.ElideRight
                            width: parent.width
                        }

                        Text {
                            text: modelData.provider + " / " + modelData.transport
                            color: root.textMuted
                            font.pixelSize: 12
                            elide: Text.ElideRight
                            width: parent.width
                        }
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: {
                        root.currentAgentId = modelData.id
                        root.agentSelected(modelData.id)
                    }
                    onEntered: if (modelData.id !== root.currentAgentId) parent.color = "#141924"
                    onExited: if (modelData.id !== root.currentAgentId) parent.color = "transparent"
                }
            }
        }
    }
}
