import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../views"

StackLayout {
    id: root
    required property var shell
    signal handoffRequested(string taskId, var selectedFiles)
    signal openInEditorRequested(string path, int line)
    currentIndex: {
        if (!shell)
            return 0
        return ["editor", "review", "config", "history"].indexOf(String(shell.centerMode))
    }

    EditorAreaSlot { shell: root.shell }
    ReviewView {
        onHandoffRequested: function(taskId, selectedFiles) { root.handoffRequested(taskId, selectedFiles) }
        onOpenInEditorRequested: function(path, line) { root.openInEditorRequested(path, line) }
    }
    ConfigView {}
    HistoryView {}
}
