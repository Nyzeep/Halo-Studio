import QtQuick
import QtQuick.Controls

Item {
    id: root

    property var controller
    property string currentAgentId: ""
    property var suggestions: []
    property int selectedSuggestion: 0
    property color panelColor: "#d4212633"
    property color borderColor: "#1affffff"
    property color textPrimary: "#f3f5f8"
    property color textMuted: "#9aa3b2"
    property color accent: "#8b5cf6"
    signal submitted(string text)

    height: 118

    Rectangle {
        anchors.fill: parent
        color: root.panelColor
        radius: 14
        border.color: commandInput.activeFocus ? root.accent : root.borderColor
        border.width: 1
    }

    TextArea {
        id: commandInput
        anchors.left: parent.left
        anchors.right: sendButton.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.margins: 14
        color: root.textPrimary
        placeholderText: "Message or /command"
        placeholderTextColor: root.textMuted
        font.pixelSize: 14
        wrapMode: TextArea.Wrap
        background: Rectangle { color: "transparent" }
        onTextChanged: refreshSuggestions()

        Keys.onPressed: function(event) {
            if (suggestionPopup.visible && event.key === Qt.Key_Down) {
                root.selectedSuggestion = Math.min(root.selectedSuggestion + 1, root.suggestions.length - 1)
                event.accepted = true
            } else if (suggestionPopup.visible && event.key === Qt.Key_Up) {
                root.selectedSuggestion = Math.max(root.selectedSuggestion - 1, 0)
                event.accepted = true
            } else if (suggestionPopup.visible && event.key === Qt.Key_Tab) {
                applySuggestion(root.selectedSuggestion)
                event.accepted = true
            } else if (event.key === Qt.Key_Return && (event.modifiers & Qt.ControlModifier)) {
                submitCurrent()
                event.accepted = true
            }
        }
    }

    Button {
        id: sendButton
        width: 76
        height: 38
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 14
        text: "Send"
        onClicked: submitCurrent()
    }

    Rectangle {
        id: suggestionPopup
        width: Math.min(parent.width, 520)
        height: visible ? Math.min(184, root.suggestions.length * 44 + 12) : 0
        anchors.left: parent.left
        anchors.bottom: parent.top
        anchors.bottomMargin: 8
        color: "#ef151821"
        radius: 10
        border.color: root.borderColor
        border.width: 1
        visible: root.suggestions.length > 0 && commandInput.text.trim().indexOf("/") === 0
        clip: true

        ListView {
            anchors.fill: parent
            anchors.margins: 6
            model: root.suggestions
            clip: true

            delegate: Rectangle {
                width: ListView.view.width
                height: 40
                radius: 7
                color: index === root.selectedSuggestion ? "#282f42" : "transparent"

                Row {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 10

                    Text {
                        text: modelData.name
                        color: root.textPrimary
                        font.pixelSize: 13
                        font.family: "Cascadia Mono"
                        width: 112
                        elide: Text.ElideRight
                    }

                    Text {
                        text: modelData.description
                        color: root.textMuted
                        font.pixelSize: 12
                        width: parent.width - 132
                        elide: Text.ElideRight
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: applySuggestion(index)
                }
            }
        }
    }

    function refreshSuggestions() {
        var query = commandInput.text
        root.selectedSuggestion = 0
        if (query.trim().indexOf("/") !== 0 || root.controller === undefined || root.controller === null) {
            root.suggestions = []
            return
        }
        root.suggestions = root.controller.complete(query, root.currentAgentId)
    }

    function applySuggestion(index) {
        if (index < 0 || index >= root.suggestions.length)
            return
        var suggestion = root.suggestions[index]
        commandInput.text = suggestion.insertText + " "
        commandInput.cursorPosition = commandInput.text.length
        root.suggestions = []
    }

    function submitCurrent() {
        var value = commandInput.text.trim()
        if (value.length === 0)
            return
        root.submitted(value)
        commandInput.text = ""
        root.suggestions = []
    }
}
