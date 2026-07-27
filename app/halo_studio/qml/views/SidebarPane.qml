import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../components/util.js" as Util

// 左栏：工作区卡片 + Pi 与 OpenCode 两个独立运行时状态卡。
ColumnLayout {
    id: sidebar

    readonly property var wsVM: (typeof workspaceVM !== "undefined") ? workspaceVM : null
    readonly property var rtVM: (typeof runtimeVM !== "undefined") ? runtimeVM : null
    readonly property bool wsActive: wsVM !== null && Util.isTrue(wsVM.active)
    readonly property bool wsTrusted: wsVM !== null && String(wsVM.trustState) === "trusted"

    spacing: 10

    SectionCard {
        Layout.fillWidth: true
        title: "活动工作区"

        Text {
            Layout.fillWidth: true
            text: sidebar.wsActive ? Util.textOr(sidebar.wsVM.realPath, "—") : "尚未打开工作区"
            color: Theme.text
            elide: Text.ElideMiddle
            font.pixelSize: 12
        }
        RowLayout {
            spacing: 8
            StatusBadge {
                label: sidebar.wsActive ? (sidebar.wsTrusted ? "受信任" : "未信任") : "无工作区"
                tone: sidebar.wsActive ? (sidebar.wsTrusted ? Theme.ok : Theme.warn) : Theme.neutral
            }
        }
        Rectangle {
            visible: sidebar.wsVM !== null && Util.isTrue(sidebar.wsVM.identityChanged)
            Layout.fillWidth: true
            radius: Theme.radius
            color: Qt.rgba(Theme.danger.r, Theme.danger.g, Theme.danger.b, 0.12)
            border.color: Theme.danger
            implicitHeight: identityText.implicitHeight + 16
            Text {
                id: identityText
                anchors.fill: parent
                anchors.margins: 8
                text: "工作区身份已变化（目录疑似被替换或重建），信任已降级，请重新确认。"
                color: Theme.danger
                wrapMode: Text.Wrap
                font.pixelSize: 12
            }
        }
        TextField {
            id: wsPathInput
            Layout.fillWidth: true
            placeholderText: "输入 Git 仓库路径以打开或切换"
        }
        RowLayout {
            spacing: 6
            Button {
                text: "信任"
                enabled: sidebar.wsActive && !sidebar.wsTrusted
                onClicked: if (sidebar.wsVM !== null && sidebar.wsVM.trust) sidebar.wsVM.trust()
            }
            Button {
                text: "撤销信任"
                enabled: sidebar.wsActive && sidebar.wsTrusted
                onClicked: if (sidebar.wsVM !== null && sidebar.wsVM.revoke) sidebar.wsVM.revoke()
            }
            Button {
                text: "打开 / 切换"
                enabled: wsPathInput.text.trim().length > 0
                onClicked: if (sidebar.wsVM !== null && sidebar.wsVM.open) sidebar.wsVM.open(wsPathInput.text.trim())
            }
        }
        ErrorLabel { Layout.fillWidth: true; vm: sidebar.wsVM }
    }

    RuntimeCard {
        Layout.fillWidth: true
        agentName: "Pi"
        stateValue: sidebar.rtVM !== null ? sidebar.rtVM.piState : undefined
        reasonValue: sidebar.rtVM !== null ? sidebar.rtVM.piReason : undefined
        hintValue: sidebar.rtVM !== null ? sidebar.rtVM.piRecoveryHint : undefined
        versionValue: sidebar.rtVM !== null ? sidebar.rtVM.piVersion : undefined
    }

    RuntimeCard {
        Layout.fillWidth: true
        agentName: "OpenCode"
        stateValue: sidebar.rtVM !== null ? sidebar.rtVM.opencodeState : undefined
        reasonValue: sidebar.rtVM !== null ? sidebar.rtVM.opencodeReason : undefined
        hintValue: sidebar.rtVM !== null ? sidebar.rtVM.opencodeRecoveryHint : undefined
        versionValue: sidebar.rtVM !== null ? sidebar.rtVM.opencodeVersion : undefined
    }

    RowLayout {
        spacing: 6
        Button {
            text: "刷新运行时状态"
            onClicked: if (sidebar.rtVM !== null && sidebar.rtVM.refresh) sidebar.rtVM.refresh()
        }
    }
    ErrorLabel { Layout.fillWidth: true; vm: sidebar.rtVM }

    Item { Layout.fillHeight: true }
}
