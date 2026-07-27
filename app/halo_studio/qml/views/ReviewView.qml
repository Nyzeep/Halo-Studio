import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"
import "../components/util.js" as Util

// 审查页：文件变更列表 + 只读 Diff + 验证结论 + 归因与任务前已有修改 + 接受/拒绝。
ColumnLayout {
    id: reviewPage

    signal handoffRequested(string taskId, var selectedFiles)
    signal openInEditorRequested(string path, int line)

    readonly property var rVM: (typeof reviewVM !== "undefined") ? reviewVM : null
    readonly property var tVM: (typeof taskVM !== "undefined") ? taskVM : null
    readonly property var jumpVM: (typeof reviewJumpVM !== "undefined") ? reviewJumpVM : null

    // 依赖 evidenceVersion（notify=bundleChanged）：证据更新后强制重取当前文件 Diff
    readonly property string currentDiff: {
        var version = reviewPage.rVM !== null ? reviewPage.rVM.evidenceVersion : 0
        if (reviewPage.rVM === null || reviewPage.rVM.files === undefined || fileList.currentIndex < 0)
            return ""
        var filesModel = reviewPage.rVM.files
        if (!filesModel || typeof filesModel.get !== "function")
            return ""
        var entry = filesModel.get(fileList.currentIndex)
        return (entry && entry.diff !== undefined) ? String(entry.diff) : ""
    }

    spacing: 10

    function collectSelectedPaths() {
        var paths = []
        if (rVM === null || rVM.files === undefined || !rVM.files)
            return paths
        var filesModel = rVM.files
        if (typeof filesModel.rowCount !== "function" || typeof filesModel.get !== "function")
            return paths
        var n = filesModel.rowCount()
        for (var i = 0; i < n; ++i) {
            var entry = filesModel.get(i)
            if (entry && Util.hasText(entry.path))
                paths.push(String(entry.path))
        }
        return paths
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        Button {
            text: "载入当前任务交付"
            enabled: reviewPage.tVM !== null && Util.hasText(reviewPage.tVM.taskId)
            onClicked: {
                if (reviewPage.rVM !== null && reviewPage.rVM.load && reviewPage.tVM !== null)
                    reviewPage.rVM.load(String(reviewPage.tVM.taskId))
            }
        }
        Text {
            text: reviewPage.rVM !== null && Util.hasText(reviewPage.rVM.taskId)
                  ? ("证据版本 v" + reviewPage.rVM.evidenceVersion
                     + (Util.isTrue(reviewPage.rVM.isLatest) ? "（最新）" : "（非最新，不可决定）")
                     + "　结局：" + Util.outcomeLabel(reviewPage.rVM.outcome))
                  : "尚未载入可审查交付"
            color: Theme.textDim
            font.pixelSize: 12
        }
        Item { Layout.fillWidth: true }
        Text { text: "验证结论："; color: Theme.textDim; font.pixelSize: 12 }
        StatusBadge {
            label: Util.verificationLabel(reviewPage.rVM !== null ? reviewPage.rVM.verificationStatus : undefined)
            tone: Util.verificationTone(reviewPage.rVM !== null ? reviewPage.rVM.verificationStatus : undefined, Theme)
        }
        Text {
            text: Util.verificationSourceLabel(reviewPage.rVM !== null ? reviewPage.rVM.verificationSource : undefined)
            color: Theme.textDim
            font.pixelSize: 12
        }
        StatusBadge {
            label: "归因：" + Util.attributionLabel(reviewPage.rVM !== null ? reviewPage.rVM.attribution : undefined)
            tone: reviewPage.rVM !== null && String(reviewPage.rVM.attribution) === "mixed" ? Theme.warn : Theme.neutral
        }
    }

    Text {
        Layout.fillWidth: true
        visible: text.length > 0
        text: {
            var detail = reviewPage.rVM !== null ? Util.textOr(reviewPage.rVM.verificationDetail, "") : ""
            return detail.length > 0 ? "验证详情：" + detail : ""
        }
        color: Theme.textDim
        wrapMode: Text.Wrap
        font.pixelSize: 12
    }

    Text {
        Layout.fillWidth: true
        visible: text.length > 0
        text: {
            var reasons = Util.listOr(reviewPage.rVM !== null ? reviewPage.rVM.attributionReasons : undefined)
            return reasons.length > 0 ? "归因说明:" + reasons.join("；") : ""
        }
        color: Theme.warn
        wrapMode: Text.Wrap
        font.pixelSize: 12
    }

    Text {
        Layout.fillWidth: true
        text: {
            var dirty = Util.listOr(reviewPage.rVM !== null ? reviewPage.rVM.baselineDirtyFiles : undefined)
            return "任务前已有修改（不归因 Agent）：" + (dirty.length > 0 ? dirty.join("、") : "无")
        }
        color: Theme.textDim
        wrapMode: Text.Wrap
        font.pixelSize: 12
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 10

        SectionCard {
            Layout.preferredWidth: 320
            Layout.fillHeight: true
            title: "文件变更"
            ListView {
                id: fileList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 2
                model: (reviewPage.rVM !== null && reviewPage.rVM.files !== undefined) ? reviewPage.rVM.files : null
                onCurrentIndexChanged: {
                    if (reviewPage.jumpVM === null || reviewPage.rVM === null || currentIndex < 0)
                        return
                    var entry = reviewPage.rVM.files.get(currentIndex)
                    if (entry)
                        reviewPage.jumpVM.setCurrentFile(String(entry.path), String(entry.change), String(entry.diff), Boolean(entry.truncated))
                }
                ScrollBar.vertical: ScrollBar {}
                delegate: Rectangle {
                    width: fileList.width
                    radius: Theme.radius
                    color: ListView.isCurrentItem ? Theme.surfaceAlt : "transparent"
                    implicitHeight: 34
                    MouseArea {
                        anchors.fill: parent
                        onClicked: fileList.currentIndex = index
                    }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        spacing: 6
                        StatusBadge {
                            label: Util.changeLabel(model.change)
                            tone: Util.changeTone(model.change, Theme)
                        }
                        Text {
                            Layout.fillWidth: true
                            text: Util.textOr(model.path, "")
                            color: Theme.text
                            elide: Text.ElideMiddle
                            font.pixelSize: 12
                        }
                        StatusBadge {
                            visible: Util.isTrue(model.truncated)
                            label: "已截断"
                            tone: Theme.warn
                        }
                        ToolButton {
                            readonly property var jumpInfo: reviewPage.jumpVM === null ? ({ "canOpen": false, "reason": "不可用" }) : reviewPage.jumpVM.describe(String(model.path), String(model.change), String(model.diff), Boolean(model.truncated))
                            text: "\ue8a7"
                            font.family: Theme.fontIcon
                            enabled: Boolean(jumpInfo.canOpen)
                            ToolTip.visible: hovered
                            ToolTip.text: Boolean(jumpInfo.canOpen)
                                          ? "定位基于证据版本 v" + (reviewPage.rVM ? reviewPage.rVM.evidenceVersion : 0) + "，文件此后再编辑可能已漂移"
                                          : String(jumpInfo.reason)
                            onClicked: reviewPage.openInEditorRequested(String(jumpInfo.editorPath), Number(jumpInfo.editorLine))
                        }
                    }
                }
            }
            Text {
                visible: fileList.count === 0
                text: "暂无可审查的文件变更"
                color: Theme.textDim
                font.pixelSize: 12
            }
        }

        SectionCard {
            Layout.fillWidth: true
            Layout.fillHeight: true
            title: "只读 Diff"
            DiffViewer {
                Layout.fillWidth: true
                Layout.fillHeight: true
                diffText: reviewPage.currentDiff
            }
        }
    }

    SectionCard {
        Layout.fillWidth: true
        title: "任务摘要"
        Text {
            Layout.fillWidth: true
            text: reviewPage.rVM !== null ? Util.textOr(reviewPage.rVM.summary, "暂无摘要") : "暂无摘要"
            color: Theme.text
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8
        TextField {
            id: rejectReasonInput
            Layout.fillWidth: true
            placeholderText: "拒绝原因（选填）"
        }
        Button {
            text: "接受交付"
            enabled: reviewPage.rVM !== null && Util.isTrue(reviewPage.rVM.isLatest)
            onClicked: if (reviewPage.rVM !== null && reviewPage.rVM.accept) reviewPage.rVM.accept()
        }
        Button {
            text: "拒绝交付"
            enabled: reviewPage.rVM !== null && Util.isTrue(reviewPage.rVM.isLatest)
            onClicked: if (reviewPage.rVM !== null && reviewPage.rVM.reject) reviewPage.rVM.reject(rejectReasonInput.text)
        }
        Button {
            text: "创建交接包…"
            enabled: reviewPage.rVM !== null && Util.hasText(reviewPage.rVM.taskId)
            onClicked: reviewPage.handoffRequested(
                String(reviewPage.rVM !== null ? reviewPage.rVM.taskId : ""),
                reviewPage.collectSelectedPaths())
        }
    }
    Text {
        Layout.fillWidth: true
        visible: reviewPage.rVM !== null && Util.hasText(reviewPage.rVM.decisionKind)
        text: reviewPage.rVM !== null && Util.hasText(reviewPage.rVM.decisionKind)
              ? ("已记录结论：" + Util.decisionLabel(reviewPage.rVM.decisionKind)
                 + "（" + Util.textOr(reviewPage.rVM.decidedAt, "—") + "）"
                 + (Util.hasText(reviewPage.rVM.decisionReason) ? "　原因：" + reviewPage.rVM.decisionReason : ""))
              : ""
        color: Theme.textDim
        wrapMode: Text.Wrap
        font.pixelSize: 12
    }
    ErrorLabel { Layout.fillWidth: true; vm: reviewPage.rVM }
}
