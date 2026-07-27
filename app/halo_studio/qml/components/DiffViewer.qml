import QtQuick
import QtQuick.Controls
import "."

// 交付审查只读 Diff 视图。
// 审查红线：readOnly 恒为 true，本组件不提供任何编辑、保存或写入能力。
ScrollView {
    id: viewer

    property alias diffText: diffArea.text

    clip: true

    TextArea {
        id: diffArea
        readOnly: true
        selectByMouse: true
        wrapMode: TextArea.NoWrap
        textFormat: TextEdit.PlainText
        font.family: Theme.monoFont
        font.pixelSize: 12
        color: Theme.text
        placeholderText: "暂无 Diff 内容"
        placeholderTextColor: Theme.textDim
        background: Rectangle {
            color: Theme.deep
            radius: Theme.radius
            border.color: Theme.border
        }
    }
}
