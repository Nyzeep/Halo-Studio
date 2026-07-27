import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../theme"

Rectangle {
    id: root
    readonly property var editorRef: (typeof editorService !== "undefined") ? editorService : null
    readonly property var activeDoc: root.editorRef && root.editorRef.activeDocument ? root.editorRef.activeDocument : null
    readonly property var searchRef: root.editorRef && root.editorRef.search ? root.editorRef.search : null
    readonly property real gutterLineHeight: Math.max(18, Math.ceil(lineMetrics.lineSpacing))
    color: Theme.editorBackground

    FontMetrics {
        id: lineMetrics
        font: textArea.font
    }

    function positionForLine(text, line, column) {
        var targetLine = Math.max(1, line)
        var start = 0
        for (var current = 1; current < targetLine; ++current) {
            var newline = text.indexOf("\n", start)
            if (newline < 0)
                return text.length
            start = newline + 1
        }
        return Math.min(text.length, start + Math.max(0, column - 1))
    }

    function lineColumnAt(text, position) {
        var before = text.substring(0, Math.max(0, position))
        return {
            "line": before.split("\n").length,
            "column": position - before.lastIndexOf("\n")
        }
    }

    function reportSelection() {
        if (!root.editorRef || !root.activeDoc)
            return
        var start = root.lineColumnAt(textArea.text, textArea.selectionStart)
        var end = root.lineColumnAt(textArea.text, textArea.selectionEnd)
        root.editorRef.reportSelection(root.activeDoc.documentId, {
            "startLine": start.line,
            "startColumn": start.column,
            "endLine": end.line,
            "endColumn": end.column,
            "hasSelection": textArea.selectionStart !== textArea.selectionEnd,
            "text": textArea.selectedText
        })
    }

    function gutterDecoration(line) {
        if (root.activeDoc === null || root.activeDoc.gutterDecorations === undefined)
            return null
        var decorations = root.activeDoc.gutterDecorations
        for (var index = 0; index < decorations.length; ++index) {
            if (Number(decorations[index].line) === line)
                return decorations[index]
        }
        return null
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 34
            color: Theme.panelBackground
            border.color: Theme.border
            border.width: 1

            ListView {
                id: tabs
                anchors.fill: parent
                anchors.leftMargin: Theme.spaceXs
                anchors.rightMargin: Theme.spaceXs
                orientation: ListView.Horizontal
                spacing: Theme.spaceXxs
                clip: true
                model: root.editorRef === null ? null : root.editorRef.documents
                delegate: ToolButton {
                    id: tab
                    required property string documentId
                    required property string path
                    required property string title
                    required property bool dirty
                    required property bool readOnly
                    required property bool preview
                    required property bool manualEditBadge
                    required property bool baselineChanged
                    height: parent.height
                    text: title + (dirty ? " *" : "") + (manualEditBadge ? " \u2691" : "") + (baselineChanged ? " M" : "") + (readOnly ? " [只读]" : "")
                    font.italic: preview
                    ToolTip.visible: hovered
                    ToolTip.text: path
                    onClicked: if (root.editorRef) root.editorRef.activate(documentId)
                    contentItem: Text {
                        text: tab.text
                        color: root.editorRef && root.editorRef.activeDocumentId === tab.documentId ? Theme.foreground : Theme.mutedForeground
                        font.pixelSize: Theme.fontSizeSmall
                        font.italic: tab.preview
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                    }
                    background: Rectangle {
                        color: root.editorRef && root.editorRef.activeDocumentId === tab.documentId ? Theme.surfaceBackground : Theme.transparentBackground
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: root.searchRef && root.searchRef.active ? 36 : 0
            visible: Layout.preferredHeight > 0
            color: Theme.surfaceBackground
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spaceSm
                anchors.rightMargin: Theme.spaceSm
                spacing: Theme.spaceXs
                TextField {
                    id: findInput
                    Layout.fillWidth: true
                    placeholderText: "查找"
                    text: root.searchRef ? root.searchRef.query : ""
                    color: Theme.foreground
                    onTextEdited: if (root.searchRef) root.searchRef.setQuery(text)
                }
                Text {
                    text: root.searchRef ? (root.searchRef.currentIndex + "/" + root.searchRef.matchCount) : ""
                    color: Theme.descriptionForeground
                    font.pixelSize: Theme.fontSizeSmall
                }
                ToolButton {
                    text: "↑"
                    ToolTip.visible: hovered
                    ToolTip.text: "上一个匹配"
                    onClicked: if (root.searchRef) root.searchRef.findPrevious()
                }
                ToolButton {
                    text: "↓"
                    ToolTip.visible: hovered
                    ToolTip.text: "下一个匹配"
                    onClicked: if (root.searchRef) root.searchRef.findNext()
                }
                ToolButton {
                    text: "×"
                    ToolTip.visible: hovered
                    ToolTip.text: "关闭查找"
                    onClicked: if (root.searchRef) root.searchRef.close()
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            TextArea {
                id: textArea
                anchors.fill: parent
                anchors.leftMargin: Theme.spaceMd + gutter.width + Theme.spaceXxs
                anchors.rightMargin: Theme.spaceMd
                anchors.topMargin: Theme.spaceMd
                anchors.bottomMargin: Theme.spaceMd
                visible: root.activeDoc !== null
                text: root.activeDoc === null ? "" : root.activeDoc.text
                readOnly: root.activeDoc !== null && root.activeDoc.readOnly
                wrapMode: TextEdit.NoWrap
                selectByMouse: true
                persistentSelection: true
                color: Theme.foreground
                selectionColor: Theme.editorSelectionBackground
                selectedTextColor: Theme.foreground
                font.family: Theme.fontMono
                font.pixelSize: Theme.fontSizeMedium
                topPadding: 1
                bottomPadding: 1
                background: Rectangle {
                    color: Theme.editorBackground
                    border.color: Theme.border
                    border.width: 1
                }
                onTextChanged: {
                    if (root.editorRef && root.activeDoc && text !== root.activeDoc.text)
                        root.editorRef.setDocumentText(root.activeDoc.documentId, text)
                }
                onCursorPositionChanged: {
                    if (!root.editorRef || !root.activeDoc)
                        return
                    var before = text.substring(0, cursorPosition)
                    var line = before.split("\n").length
                    var column = cursorPosition - before.lastIndexOf("\n")
                    root.editorRef.reportCursor(root.activeDoc.documentId, line, column)
                }
                onSelectionStartChanged: root.reportSelection()
                onSelectionEndChanged: root.reportSelection()
                Keys.onPressed: function(event) {
                    if (event.matches(StandardKey.Find) && root.searchRef) {
                        root.searchRef.open(false)
                        event.accepted = true
                    } else if (event.matches(StandardKey.Save) && root.editorRef) {
                        root.editorRef.save()
                        event.accepted = true
                    }
                }
            }

            Rectangle {
                id: gutter
                anchors.left: parent.left
                anchors.leftMargin: Theme.spaceMd
                anchors.top: parent.top
                anchors.topMargin: Theme.spaceMd
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Theme.spaceMd
                width: 42
                visible: root.activeDoc !== null
                color: Theme.panelBackground
                border.color: Theme.border
                border.width: 1
                clip: true
                ListView {
                    id: gutterLines
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true
                    interactive: false
                    contentY: textArea.contentY
                    model: root.activeDoc === null ? 0 : root.activeDoc.lineCount
                    delegate: Rectangle {
                        readonly property var decoration: root.gutterDecoration(index + 1)
                        width: gutter.width - 2
                        height: root.gutterLineHeight
                        color: decoration === null ? "transparent" : (Theme[decoration.colorToken] || Theme.transparentBackground)
                        Text {
                            anchors.right: parent.right
                            anchors.rightMargin: Theme.spaceXxs
                            anchors.verticalCenter: parent.verticalCenter
                            text: index + 1
                            color: Theme.editorLineNumberForeground
                            font.family: Theme.fontMono
                            font.pixelSize: Theme.fontSizeXSmall
                        }
                        ToolTip.visible: decorationPointer.containsMouse && decoration !== null
                        ToolTip.text: decoration === null ? "" : String(decoration.tooltip)
                        MouseArea {
                            id: decorationPointer
                            anchors.fill: parent
                            hoverEnabled: true
                        }
                    }
                }
            }

            ColumnLayout {
                anchors.centerIn: parent
                visible: root.activeDoc === null
                spacing: Theme.spaceSm
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "Halo Studio"
                    color: Theme.foreground
                    font.pixelSize: Theme.fontSizeLarge
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "从资源管理器打开一个文件"
                    color: Theme.descriptionForeground
                    font.pixelSize: Theme.fontSizeSmall
                }
            }
        }
    }

    Connections {
        target: root.editorRef
        function onGotoLineRequested(documentId, line, column) {
            if (root.activeDoc && root.activeDoc.documentId === documentId)
                textArea.cursorPosition = root.positionForLine(textArea.text, line, column)
        }
        function onActiveChanged() {
            if (root.activeDoc)
                textArea.cursorPosition = 0
        }
    }
}
