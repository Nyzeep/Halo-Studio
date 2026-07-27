pragma Singleton
import QtQuick

// 全局暗色主题（原生桌面风格）；颜色只在此定义，避免散落魔法值。
QtObject {
    readonly property color background: "#1b1e23"
    readonly property color deep: "#14161a"
    readonly property color surface: "#22262d"
    readonly property color surfaceAlt: "#2b3038"
    readonly property color border: "#3a4048"
    readonly property color text: "#e6e8eb"
    readonly property color textDim: "#9aa1a9"
    readonly property color accent: "#4f8cff"
    readonly property color ok: "#3fb950"
    readonly property color warn: "#d29922"
    readonly property color danger: "#f85149"
    readonly property color neutral: "#8b949e"
    readonly property int radius: 6
    readonly property string monoFont: "Consolas"
}
