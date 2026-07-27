import QtQuick
import QtQuick.Layouts
import "../theme"

Rectangle {
    id: bar
    required property var shell
    readonly property var taskRef: (typeof taskVM !== "undefined") ? taskVM : null
    width: Theme.activityBarWidth
    color: Theme.activityBarBackground

    readonly property var entries: [
        { id: "explorer", icon: "\ue773", title: "资源管理器" },
        { id: "task", icon: "\ue7c4", title: "工作区与任务" },
        { id: "review", icon: "\ue9d5", title: "交付审查" },
        { id: "history", icon: "\ue81c", title: "交付历史" }
    ]

    function isActive(entryId) {
        if (entryId === "explorer" || entryId === "task")
            return shell && shell.activeSideBarPanel === entryId && shell.sideBarVisible && shell.centerMode === "editor"
        return shell && shell.centerMode === entryId
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        Repeater {
            model: bar.entries
            delegate: ActivityBarButton {
                Layout.alignment: Qt.AlignHCenter
                entryId: modelData.id
                iconGlyph: modelData.icon
                tooltipText: modelData.title
                active: bar.isActive(entryId)
                badgeVisible: (entryId === "review" && bar.taskRef && String(bar.taskRef.state) === "review_ready")
                              || (entryId === "task" && bar.taskRef && String(bar.taskRef.state) === "awaiting_action")
                onClicked: if (bar.shell) bar.shell.activate(entryId)
            }
        }
        Item { Layout.fillHeight: true }
        ActivityBarButton {
            Layout.alignment: Qt.AlignHCenter
            entryId: "config"
            iconGlyph: "\ue713"
            tooltipText: "启动配置"
            active: bar.isActive(entryId)
            onClicked: if (bar.shell) bar.shell.activate(entryId)
        }
    }
}
