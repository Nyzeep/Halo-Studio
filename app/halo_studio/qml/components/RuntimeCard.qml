import QtQuick
import QtQuick.Layouts
import "."
import "util.js" as Util

// 单个受管应用的独立运行时状态卡（绝不与另一受管应用合并为“全局在线”）。
SectionCard {
    id: card

    property string agentName: ""
    property var stateValue
    property var reasonValue
    property var hintValue
    property var versionValue

    title: card.agentName + " 运行时"

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        StatusBadge {
            label: Util.runtimeStateLabel(card.stateValue)
            tone: Util.runtimeStateTone(card.stateValue, Theme)
        }
        Text {
            text: "版本：" + Util.textOr(card.versionValue, "—")
            color: Theme.textDim
            font.pixelSize: 12
        }
        Item { Layout.fillWidth: true }
    }
    Text {
        visible: Util.hasText(card.reasonValue)
        Layout.fillWidth: true
        text: "原因：" + Util.textOr(card.reasonValue, "")
        color: Theme.danger
        wrapMode: Text.Wrap
        font.pixelSize: 12
    }
    Text {
        visible: Util.hasText(card.hintValue)
        Layout.fillWidth: true
        text: "恢复建议：" + Util.textOr(card.hintValue, "")
        color: Theme.warn
        wrapMode: Text.Wrap
        font.pixelSize: 12
    }
}
