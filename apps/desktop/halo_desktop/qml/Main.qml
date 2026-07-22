import QtQuick
import QtQuick.Controls
import "components"
import "styles"

ApplicationWindow {
    id: window
    width: 1320
    height: 820
    minimumWidth: 1040
    minimumHeight: 680
    visible: true
    title: "Halo Studio"
    color: theme.background

    Theme { id: theme }

    property var appController: typeof controller === "undefined" ? null : controller
    property var agents: appController === null ? [] : appController.agents
    property var events: appController === null ? [] : appController.events
    property string currentAgentId: agents.length > 0 ? agents[0].id : "codex-cli"
    property bool debugDrawerOpen: false

    Rectangle {
        anchors.fill: parent
        color: theme.background

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: parent.height
            gradient: Gradient {
                GradientStop { position: 0.0; color: "#0b0d12" }
                GradientStop { position: 0.55; color: "#10131a" }
                GradientStop { position: 1.0; color: "#0d1016" }
            }
        }

        Rectangle {
            width: parent.width * 0.56
            height: parent.height * 0.42
            anchors.right: parent.right
            anchors.top: parent.top
            color: "#1f2433"
            opacity: 0.36
            radius: 26
        }
    }

    Column {
        anchors.fill: parent
        anchors.margins: 18
        spacing: 14

        Row {
            width: parent.width
            height: 46
            spacing: 12

            Text {
                text: "Halo Studio"
                color: theme.textPrimary
                font.pixelSize: 20
                font.weight: Font.Medium
                anchors.verticalCenter: parent.verticalCenter
            }

            Text {
                text: "Native Agent Workspace"
                color: theme.textMuted
                font.pixelSize: 13
                anchors.verticalCenter: parent.verticalCenter
            }

            Item { width: parent.width - 430; height: 1 }

            Rectangle {
                width: 148
                height: 30
                radius: 15
                color: theme.glass
                border.color: theme.border
                anchors.verticalCenter: parent.verticalCenter

                Text {
                    anchors.centerIn: parent
                    text: window.agents.length + " agents ready"
                    color: theme.cyan
                    font.pixelSize: 12
                }
            }
        }

        Row {
            width: parent.width
            height: parent.height - 60
            spacing: 12

            AgentSidebar {
                id: sidebar
                width: 260
                height: parent.height
                agents: window.agents
                currentAgentId: window.currentAgentId
                panelColor: theme.glass
                selectedColor: theme.glassStrong
                borderColor: theme.border
                textPrimary: theme.textPrimary
                textMuted: theme.textMuted
                accent: theme.accent
                onAgentSelected: function(agentId) {
                    window.currentAgentId = agentId
                }
            }

            Column {
                width: parent.width - sidebar.width - inspector.width - 24
                height: parent.height
                spacing: 12

                WorkflowTimeline {
                    id: timeline
                    width: parent.width
                    height: parent.height - composer.height - 12
                    events: window.events
                    currentAgentId: window.currentAgentId
                    panelColor: theme.glass
                    cardColor: theme.panel
                    borderColor: theme.border
                    textPrimary: theme.textPrimary
                    textMuted: theme.textMuted
                    accent: theme.accent
                    cyan: theme.cyan
                    success: theme.success
                    warning: theme.warning
                }

                CommandComposer {
                    id: composer
                    width: parent.width
                    controller: window.appController
                    currentAgentId: window.currentAgentId
                    panelColor: theme.glassStrong
                    borderColor: theme.border
                    textPrimary: theme.textPrimary
                    textMuted: theme.textMuted
                    accent: theme.accent
                    onSubmitted: function(text) {
                        console.log("composer submitted", text)
                    }
                }
            }

            InspectorPanel {
                id: inspector
                width: 340
                height: parent.height
                agents: window.agents
                events: window.events
                currentAgentId: window.currentAgentId
                debugDrawerOpen: window.debugDrawerOpen
                panelColor: theme.glass
                cardColor: theme.panel
                borderColor: theme.border
                textPrimary: theme.textPrimary
                textMuted: theme.textMuted
                accent: theme.accent
            }
        }
    }
}
