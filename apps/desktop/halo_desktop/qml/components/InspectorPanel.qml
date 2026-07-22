import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var agents: []
    property var events: []
    property string currentAgentId: ""
    property bool debugDrawerOpen: false
    property color panelColor: "#b81c202c"
    property color cardColor: "#cc151821"
    property color borderColor: "#1affffff"
    property color textPrimary: "#f3f5f8"
    property color textMuted: "#9aa3b2"
    property color accent: "#8b5cf6"

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

        Text {
            text: "Inspector"
            color: root.textPrimary
            font.pixelSize: 16
            font.weight: Font.Medium
        }

        Repeater {
            model: [
                { label: "Agent", value: root.currentAgentId },
                { label: "Events", value: visibleEventCount().toString() },
                { label: "Queue", value: "fake runtime" },
                { label: "MCP", value: "manifest only" }
            ]

            Rectangle {
                width: parent.width
                height: 62
                radius: 8
                color: root.cardColor
                border.color: root.borderColor
                border.width: 1

                Column {
                    anchors.fill: parent
                    anchors.margins: 10
                    spacing: 5

                    Text {
                        text: modelData.label
                        color: root.textMuted
                        font.pixelSize: 12
                    }

                    Text {
                        text: modelData.value
                        color: root.textPrimary
                        font.pixelSize: 14
                        width: parent.width
                        elide: Text.ElideRight
                    }
                }
            }
        }

        Rectangle {
            width: parent.width
            height: root.debugDrawerOpen ? 136 : 42
            radius: 8
            color: root.cardColor
            border.color: root.debugDrawerOpen ? root.accent : root.borderColor
            border.width: 1

            Column {
                anchors.fill: parent
                anchors.margins: 10
                spacing: 8

                Row {
                    width: parent.width
                    spacing: 8

                    Text {
                        text: "Debug"
                        color: root.textPrimary
                        font.pixelSize: 13
                        font.weight: Font.Medium
                        width: parent.width - 70
                    }

                    Button {
                        text: root.debugDrawerOpen ? "Hide" : "Show"
                        width: 62
                        height: 24
                        onClicked: root.debugDrawerOpen = !root.debugDrawerOpen
                    }
                }

                Text {
                    text: "Raw terminal stream is reserved for later adapter debugging."
                    color: root.textMuted
                    font.pixelSize: 12
                    width: parent.width
                    wrapMode: Text.Wrap
                    visible: root.debugDrawerOpen
                }
            }
        }
    }

    function visibleEventCount() {
        if (!root.events)
            return 0

        var count = 0
        for (var index = 0; index < root.events.length; index += 1) {
            if (root.events[index].agentId === root.currentAgentId)
                count += 1
        }
        return count
    }
}
