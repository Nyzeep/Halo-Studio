import QtQuick

Rectangle {
    id: root

    property color panelColor: "#b81c202c"
    property color strokeColor: "#1affffff"
    property int padding: 14
    default property alias content: body.data

    color: panelColor
    radius: 10
    border.color: strokeColor
    border.width: 1
    clip: true

    Column {
        id: body
        anchors.fill: parent
        anchors.margins: root.padding
        spacing: 10
    }
}
