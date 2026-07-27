import QtQuick
import QtQuick.Controls
import "theme"
import "shell"

// Halo Studio 主窗口：原生暗色桌面工作台。
// 红线：全界面无编辑器、无终端、无浏览器组件。
ApplicationWindow {
    id: root

    readonly property var appRef: (typeof appVM !== "undefined") ? appVM : null

    visible: true
    width: 1280
    height: 800
    minimumWidth: 1024
    minimumHeight: 640
    title: "Halo Studio"
    color: Theme.windowBackground

    palette.window: Theme.windowBackground
    palette.windowText: Theme.foreground
    palette.base: Theme.inputBackground
    palette.alternateBase: Theme.surfaceBackground
    palette.text: Theme.foreground
    palette.button: Theme.surfaceBackground
    palette.buttonText: Theme.foreground
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.foreground
    palette.placeholderText: Theme.descriptionForeground
    palette.mid: Theme.border
    palette.dark: Theme.border
    palette.light: Theme.surfaceBackground

    Shell { anchors.fill: parent }

    // 底部状态条：Sidecar 连接状态、协议版本、不可用原因（常显）。
    footer: ShellStatusBar { shell: (typeof shellVM !== "undefined") ? shellVM : null }
}
