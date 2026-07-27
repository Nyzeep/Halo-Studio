pragma Singleton
import QtQuick

// IDE 壳层唯一的视觉令牌表。除这里外，新壳层组件不写裸色值。
QtObject {
    readonly property color windowBackground: "#17191d"
    readonly property color activityBarBackground: "#1d2026"
    readonly property color sideBarBackground: "#20242b"
    readonly property color editorBackground: "#14171b"
    readonly property color panelBackground: "#1b1f25"
    readonly property color surfaceBackground: "#222831"
    readonly property color surfaceHoverBackground: "#2a323c"
    readonly property color inputBackground: "#12151a"
    readonly property color inputBorder: "#3c4653"
    readonly property color border: "#343d49"
    readonly property color focusBorder: "#52b6d8"
    readonly property color foreground: "#e6ebf0"
    readonly property color mutedForeground: "#a5b0bc"
    readonly property color descriptionForeground: "#8090a0"
    readonly property color activityBarForeground: "#f0f5f9"
    readonly property color activityBarInactiveForeground: "#8e9aa7"
    readonly property color activityBarActiveBorder: "#52b6d8"
    readonly property color ghostElementHoverBackground: "#2d3742"
    readonly property color listActiveSelectionBackground: "#294459"
    readonly property color listHoverBackground: "#28323c"
    readonly property color listHighlightForeground: "#79d7ec"
    readonly property color statusBarBackground: "#1b303d"
    readonly property color statusBarForeground: "#dbeaf0"
    readonly property color accent: "#52b6d8"
    readonly property color success: "#55c28a"
    readonly property color warning: "#e5b85c"
    readonly property color danger: "#e4726b"
    readonly property color neutral: "#96a4b2"
    readonly property color editorLineHighlight: "#1d2831"
    readonly property color editorSelectionBackground: "#31546a"
    readonly property color editorLineNumberForeground: "#637180"
    readonly property color editorActiveLineNumberForeground: "#d4e2eb"
    readonly property color syntaxKeyword: "#d98bd0"
    readonly property color syntaxString: "#9ed17b"
    readonly property color syntaxComment: "#748292"
    readonly property color syntaxNumber: "#e4bf73"
    readonly property color syntaxFunction: "#75c9e8"
    readonly property color syntaxType: "#70d1b7"
    readonly property color syntaxAttribute: "#d8a77c"
    readonly property color syntaxBuiltin: "#a6b9ee"
    readonly property color syntaxConstant: "#e2c07b"
    readonly property color syntaxOperator: "#b4c3d0"
    readonly property color syntaxVariable: "#e6ebf0"
    readonly property color syntaxError: "#e4726b"
    readonly property color decorationModifiedForeground: "#e5b85c"
    readonly property color decorationAddedForeground: "#55c28a"
    readonly property color decorationDeletedForeground: "#e4726b"
    readonly property color gutterAgentChangeBackground: "#52b6d822"
    readonly property color gutterMixedChangeBackground: "#8cd29922"
    readonly property color transparentBackground: "transparent"
    readonly property color editorFindMatchBackground: "#5f4d1f"
    readonly property color tabDirtyForeground: "#e5b85c"
    readonly property color baselineChangedBadgeForeground: "#55c28a"

    readonly property string fontUi: "Segoe UI"
    readonly property string fontMono: "Cascadia Mono"
    readonly property string fontIcon: "Segoe Fluent Icons"
    readonly property int fontSizeXSmall: 11
    readonly property int fontSizeSmall: 12
    readonly property int fontSizeMedium: 14
    readonly property int fontSizeLarge: 18
    readonly property int activityBarWidth: 48
    readonly property int statusBarHeight: 26
    readonly property int sideBarHeaderHeight: 34
    readonly property int sideBarMinWidth: 260
    readonly property int bottomPanelMinHeight: 130
    readonly property int explorerRowHeight: 26
    readonly property int treeIndentWidth: 14
    readonly property int spaceXxs: 4
    readonly property int spaceXs: 6
    readonly property int spaceSm: 8
    readonly property int spaceMd: 12
    readonly property int spaceLg: 16
    readonly property int radius: 4
}
