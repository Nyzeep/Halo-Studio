# 10 - IDE 壳层与设计语言

**状态**：已完成（待评审）
**日期**：2026-07-27
**依据**：`requirements-alignment/03-ide-editor-and-reference-alignment.md`、`docs/design/references/R1-vscode-analysis.md`、`R2-zed-analysis.md`、`R5-bitfun-analysis.md`
**裁决权**：现有五个视图（任务/审查/交接/配置/历史）在新壳层中的归位由本文档最终决定；Theme token 全集由本文档定义；其余文档不得与本文冲突。

---

## 1. 目标与范围

### 1.1 目标

1. 把 `Main.qml` 的"TabBar + StackLayout"单页形态重构为 IDE 壳层：**ActivityBar（图标条）+ 可折叠侧栏 + 编辑器组区域（11 号插槽）+ 底部面板 + 状态栏** 的固定区块布局。
2. 定义 QML 主题单例 **`qml/theme/Theme.qml`** 的 token 全集（背景层级、前景层级、强调色、状态色、边框、字体族/字号/行高、间距刻度、圆角），暗色首发；全部 QML 组件只引用 token，禁止裸色值。
3. 布局状态持久化（侧栏宽度/面板高度/最近视图/可见性，QSettings）。
4. 快捷键绑定层：全局 `Shortcut` 挂载 `CommandRegistry`（注册表细节引用 13 号，本文只定义挂载结构与本模块自有命令）。
5. 给出现有五个视图的逐文件迁移改造清单。

### 1.2 范围外

- **编辑器内部**（文档模型、标签页模型、语法高亮、查找替换、脏状态/保存）→ 11 号；本文只预留 `EditorAreaSlot` 插槽与状态栏消费的最小状态位。
- **fs.\* IPC 契约与资源管理器内容** → 12 号；本文只预留 `ExplorerPanel` 槽位（含诚实空态）。
- **CommandRegistry 细节、命令面板/快速打开、和弦解析、用户覆盖** → 13 号。
- **差异化功能的裁决**（人工介入归因、任务上下文选择器、审查→编辑器跳转、基线徽章、归因边栏）→ 15 号；本文只留接口挂点（§5）。
- 自定义标题栏（首发沿用系统原生标题栏）、多窗口、自由停靠/面板拖移、编辑器分屏、zen 模式、banner、密度三档、亮色主题**值表**（token 结构就绪，亮色值远期）。
- 不新增任何 IPC 方法或事件（§3.1）。

---

## 2. 参考结论引用

| # | 采纳结论 | 出处 | 本文落法 |
| --- | --- | --- | --- |
| 1 | 固定 Parts 枚举 + 嵌套 SplitView，不做自由停靠；区块隐藏不销毁、记忆尺寸 | R1 §1.1/§1.2、借鉴清单 1 | §4.1/§4.2：`ActivityBar / SideBarHost / CenterHost(EditorAreaSlot+Surfaces) / BottomPanelHost / StatusBar` 五区块 + 两层 SplitView |
| 2 | LayoutStateModel 式持久化：typed key、批量落盘（非拖动实时写）、默认值按窗口尺寸动态计算、临时状态不污染持久值 | R1 §1.3、借鉴清单 2 | §4.3/§4.7：`ShellViewModel` + QSettings `shell/*` 键空间；侧栏默认 `min(340, w/4)`、面板默认 `h/3` |
| 3 | 活动栏条目 → 面板用注册表（描述对象数组）驱动，不写 if/else | R1 §1.4、R2 §2.1（Panel 协议自描述） | §4.5.1：`ActivityBar.entries` 描述数组（id/icon/title/kind） |
| 4 | 一个 dock 内多面板互斥显示 + 图标切换；面板开关按钮可放状态栏 | R2 §2.1/§2.2 | 侧栏面板互斥（StackLayout）；底部面板开关放状态栏右端 |
| 5 | 颜色令牌：扁平语义表 + 少量根 token 派生；surface 族分层；element/ghost_element 双状态色族 | R1 §5.1/§5.2/§5.3、R2 §3.1 | §4.6 token 全集（约 90 项），ghost 族给透明底控件（图标按钮/标签页） |
| 6 | 双字体体系 + 四档字号 + 离散间距刻度 + 阴影仅表达层级（弹层/模态）+ 编辑器背景独立于面板背景 | R2 §3.2/§3.3/§3.4/§3.5 | §4.6：`fontUi/fontMono` 分离、`fontSizeXSmall..Large` 四档、`spaceXxs..Xxl` 七档、`widgetShadow` 只用于弹层、`editorBackground` 独立 token |
| 7 | 布局状态属界面偏好，不进交付数据存储 | R2 §2.3 | 布局持久化用 QSettings，**不进** Sidecar halo.db |
| 8 | 空视图强制三态（preparing/loading/load-failed），禁止裸空白；一级状态附原因与恢复建议 | R5 §6.1（借鉴清单 6/8） | §4.5：`ExplorerPanel` 占位与 `EditorAreaSlot` 欢迎页均带明确状态文案；状态栏沿用 reason/recovery_hint 三件套 |
| 9 | 键绑定分层权重与和弦解析（细节归 13 号）；命令+标题+键绑定一次声明 | R1 §3.1/§4 | §4.8：仅定义 Shortcut 挂载结构与本模块命令 id/默认键；解析与覆盖机制引用 13 号 |

**明确不借鉴**：视图拖拽重排/自由停靠、多窗口（auxiliary window）、zen mode、banner part、树形 scoped context（13 号用平面键值）、Zed 的面板跨 dock 迁移与 zoom、密度三档（远期）、GPUI/自绘框架、Tauri/WebView 承载。

---

## 3. 与现有契约的关系（契约增量）

### 3.1 对 `docs/ipc-protocol.md`：零增量

壳层是纯 UI 重组：不新增方法、不新增事件、不改任何消息形状。状态栏继续消费既有 `sidecar.hello` 结果与 `runtime.state` / `task.state` / `workspace.changed` 事件（经既有 ViewModel，不新开订阅通道）。`fs.*` 增量属 12 号，`action_request` 闭环等属 14 号。

### 3.2 对 `docs/module-contracts.md`：第 8 节（app/halo_studio）增量

评审通过后按下列条目**追加**进 module-contracts.md 第 8 节：

1. **目录结构增量**：`qml/` 新增 `theme/`（Theme 单例 + qmldir）与 `shell/`（壳层组件，含 `panels/`、`surfaces/` 子目录）；`qml/views/` 目录取消（迁移表见 §6.2）；`qml/components/` 保留为通用小组件目录（Theme 移出）。
2. **ViewModel 增量**：`viewmodels/shell.py` 新增 `ShellViewModel(QObject)`。**唯一例外**：它不持 IPC client（构造签名 `ShellViewModel(settings: QSettings | None = None)`），装配代码对其特判；它绝不发起任何契约请求，纯 UI 布局状态。
3. **QML 上下文属性名册**（root context，`app.py` 统一挂载）：既有 `appVM…historyVM` 九个不变；本文新增 `shellVM`；预留名 **`editorService`**（11 号实现并挂载）、**`commandRegistry`**（13 号实现并挂载）、资源管理器相关属性名由 12 号定义。QML 侧对预留名一律 `typeof x !== "undefined"` 守卫，缺席时功能退化不报错。
4. **EditorService 最小状态位消费面**（提请 11 号在定义 EditorService 时纳入契约）：状态栏需要以下**只读**属性（含 notify 信号）：`activeFilePath: str`（无活动文档为 ""）、`cursorLine: int`（1 基，无为 -1）、`cursorColumn: int`（1 基，无为 -1）、`activeDirty: bool`、`activeReadOnly: bool`、`documentCount: int`。细节与其余 API 归 11 号。
5. **QSettings 键空间**：`shell/*` 归本模块所有（键表见 §4.7）；QSettings 使用应用级默认作用域（org=`HaloStudio`、app=`Halo Studio`，与 `main.py` 现设一致）。
6. **所有权**（module-contracts §10 增量）：本文档的落地代理拥有 `qml/shell/**`、`qml/theme/**`、`app/halo_studio/viewmodels/shell.py`、`app/tests/test_viewmodels_shell.py`，并在集成期修改 `qml/Main.qml`、`app.py`、`qml/components/*`（token 替换）与既有视图文件的迁移。

---

## 4. 详细设计

### 4.1 壳层布局总览

```
┌────────────────────────────────────────────────────────────────┐
│ （系统原生标题栏，首发不自绘）                                     │
├──┬───────────────┬─────────────────────────────────────────────┤
│A │               │                                             │
│c │   SideBar     │   CenterHost                                │
│t │  （可折叠）     │   ├ EditorAreaSlot（11 号编辑器组插槽）        │
│i │  ・Explorer   │   ├ ReviewSurface （交付审查，只读）           │
│v │  ・Task       │   ├ ConfigSurface （启动配置）                │
│i │               │   └ HistorySurface（交付历史）                │
│t │               ├─────────────────────────────────────────────┤
│y │               │   BottomPanelHost（可折叠）                   │
│B │               │   └ TracePanel（运行轨迹 + 任务控制行）         │
│a │               │                                             │
│r │               │                                             │
├──┴───────────────┴─────────────────────────────────────────────┤
│ StatusBar：Sidecar● 协议 v1 │信任│任务态│不可用原因…│ 行:列 │⌄面板 │
└────────────────────────────────────────────────────────────────┘
```

| 区块 | 职责 | 可折叠 | 尺寸持久化 |
| --- | --- | --- | --- |
| ActivityBar | 五个入口图标（explorer/task/review/history + config 沉底），徽章挂点 | 否（常显，宽 48） | 无 |
| SideBarHost | 互斥承载 `ExplorerPanel` / `TaskPanel`，头部显示面板标题+折叠按钮 | 是 | 宽度 |
| CenterHost | `StackLayout` 互斥承载编辑器组插槽与三个全页 Surface | 否（常显） | 无 |
| BottomPanelHost | Tab 化承载 `TracePanel`（首期唯一 tab，留扩展位） | 是 | 高度 |
| StatusBar | 左：Sidecar 连接/协议版本/信任/任务态/不可用原因（**常显**）；右：归因挂点、光标位置（编辑器状态位）、底部面板开关 | 否（常显，高 26） | 无 |

全局不变量（由 Shell.qml 统一裁决，区块不自查，R1 §1.1）：CenterHost 永不隐藏；SideBar 与 BottomPanel 隐藏时**不销毁**（`visible:false`），尺寸由 ShellViewModel 记忆。

### 4.2 QML 组件树与文件规划

```
app/halo_studio/qml/
├── Main.qml                       （改造：装载 Shell + StatusBar，palette 映射 Theme）
├── theme/
│   ├── qmldir                     （singleton Theme 1.0 Theme.qml）
│   └── Theme.qml                  （pragma Singleton；token 全集，§4.6）
├── shell/
│   ├── Shell.qml                  （壳层根：ActivityBar + 两层 SplitView；自动弹出规则）
│   ├── ActivityBar.qml            （条目描述数组 + Column of ActivityBarButton）
│   ├── ActivityBarButton.qml      （图标字形按钮：active 指示条、徽章、tooltip）
│   ├── SideBarHost.qml            （面板头 + StackLayout{ExplorerPanel,TaskPanel}）
│   ├── CenterHost.qml             （StackLayout{EditorAreaSlot,Review,Config,History}）
│   ├── EditorAreaSlot.qml         （11 号插槽；首版为欢迎页/空态）
│   ├── BottomPanelHost.qml        （TabBar("运行轨迹") + TracePanel + 折叠按钮）
│   ├── StatusBar.qml              （左右两组状态位；inline component StatusBarItem）
│   ├── ShortcutHost.qml           （全局 Shortcut 挂载层，§4.8）
│   ├── HandoffDialog.qml          （自 views/ 平移，逻辑不变）
│   ├── panels/
│   │   ├── ExplorerPanel.qml      （12 号槽位：三态占位，见 §4.5.4）
│   │   ├── TaskPanel.qml          （工作区卡+运行时卡+任务创建表单，自 SidebarPane/TaskView 合并迁移）
│   │   └── TracePanel.qml         （任务控制行+轨迹列表，自 TaskView 拆分迁移）
│   └── surfaces/
│       ├── ReviewSurface.qml      （自 ReviewView 迁移；内嵌 HandoffDialog；15 号挂点）
│       ├── ConfigSurface.qml      （自 ConfigView 迁移）
│       └── HistorySurface.qml     （自 HistoryView 迁移）
└── components/                    （保留：SectionCard/StatusBadge/DiffViewer/ErrorLabel/RuntimeCard/util.js；
                                     qmldir 移除 Theme 行；全部 token 化）
```

组件实例树（运行时）：

```
ApplicationWindow (Main.qml)
├── ShortcutHost {}                                  // 全局快捷键层
├── Shell {                                          // contentItem
│   ├── RowLayout
│   │   ├── ActivityBar { width: Theme.activityBarWidth }
│   │   │   ├── ActivityBarButton ×4（explorer/task/review/history）
│   │   │   └── ActivityBarButton ×1（config，Column 底部对齐）
│   │   └── SplitView (horizontal, id: mainSplit)
│   │       ├── SideBarHost {
│   │       │   visible: shellVM.sideBarVisible
│   │       │   SplitView.preferredWidth: shellVM.sideBarWidth > 0
│   │       │       ? shellVM.sideBarWidth : Math.min(340, window.width / 4)
│   │       │   SplitView.minimumWidth: Theme.sideBarMinWidth
│   │       │   ├── header: RowLayout{ Text(面板标题) + ToolButton(折叠) }
│   │       │   └── StackLayout { ExplorerPanel {} ; TaskPanel {} }
│   │       │   }
│   │       └── SplitView (vertical, id: centerSplit)
│   │           ├── CenterHost {
│   │           │   SplitView.fillHeight: true
│   │           │   └── StackLayout {
│   │           │       currentIndex: ["editor","review","config","history"]
│   │           │                     .indexOf(shellVM.centerMode)
│   │           │       EditorAreaSlot {} ; ReviewSurface {} ;
│   │           │       ConfigSurface {} ; HistorySurface {} }
│   │           │   }
│   │           └── BottomPanelHost {
│   │               visible: shellVM.bottomPanelVisible
│   │               SplitView.preferredHeight: shellVM.bottomPanelHeight > 0
│   │                   ? shellVM.bottomPanelHeight : window.height / 3
│   │               SplitView.minimumHeight: Theme.bottomPanelMinHeight
│   │               └── ColumnLayout{ TabBar("运行轨迹")+折叠按钮 ; TracePanel {} }
│   │               }
│   }
└── footer: StatusBar { height: Theme.statusBarHeight }
```

### 4.3 ShellViewModel（`viewmodels/shell.py`）

纯 UI 布局状态机 + QSettings 持久化。不持 client、不发 IPC。

```python
class ShellViewModel(QObject):
    """IDE 壳层布局状态：面板路由、折叠、尺寸记忆与 QSettings 持久化。"""

    SIDEBAR_ENTRIES = ("explorer", "task")            # kind = sidebar
    CENTER_ENTRIES = ("review", "config", "history")  # kind = center
    CENTER_MODES = ("editor", "review", "config", "history")

    activeSideBarPanelChanged = Signal()
    centerModeChanged = Signal()
    sideBarVisibleChanged = Signal()
    bottomPanelVisibleChanged = Signal()
    sideBarWidthChanged = Signal()
    bottomPanelHeightChanged = Signal()

    def __init__(self, settings: QSettings | None = None,
                 parent: QObject | None = None) -> None: ...
        # settings=None 时内部 QSettings()（org/app 取应用默认）；测试注入 IniFormat 临时文件。
        # 构造时读取持久化值；centerMode 恒从 "editor" 启动（不持久化）。

    # Property(str,  notify=activeSideBarPanelChanged)  activeSideBarPanel  # "explorer"|"task"
    # Property(str,  notify=centerModeChanged)          centerMode          # CENTER_MODES 之一
    # Property(bool, notify=sideBarVisibleChanged)      sideBarVisible
    # Property(bool, notify=bottomPanelVisibleChanged)  bottomPanelVisible
    # Property(int,  notify=sideBarWidthChanged)        sideBarWidth        # -1 = 未设置（QML 用动态默认）
    # Property(int,  notify=bottomPanelHeightChanged)   bottomPanelHeight   # -1 = 未设置

    @Slot(str)
    def activate(self, entry_id: str) -> None: ...   # 语义见下表；非法 id 忽略
    @Slot()
    def toggleSideBar(self) -> None: ...
    @Slot()
    def toggleBottomPanel(self) -> None: ...
    @Slot(int)
    def storeSideBarWidth(self, width: int) -> None: ...       # 拖动结束时调用
    @Slot(int)
    def storeBottomPanelHeight(self, height: int) -> None: ...
    @Slot()
    def flush(self) -> None: ...   # 立即落盘；AppContext.shutdown() 必须调用
```

`activate(entry_id)` 语义（ActivityBar 唯一入口，全部路由收敛于此）：

| entry_id | 当前状态 | 结果 |
| --- | --- | --- |
| explorer / task | 非当前侧栏面板，或侧栏折叠 | `centerMode="editor"`；`activeSideBarPanel=entry_id`；`sideBarVisible=True` |
| explorer / task | 已是当前面板且侧栏可见且 centerMode=="editor" | `sideBarVisible=False`（再点折叠，VS Code 语义） |
| review / config / history | `centerMode != entry_id` | `centerMode = entry_id`（侧栏不动） |
| review / config / history | `centerMode == entry_id` | `centerMode = "editor"`（再点返回编辑器） |

持久化策略（R1 §1.3）：属性变更置脏 + 500ms 单发 QTimer 合并落盘；`flush()` 立即写。**拖动过程不写**——QML 只在 `SplitView.resizing` 变为 false 时调 `storeSideBarWidth/storeBottomPanelHeight`。

### 4.4 视图归位方案（本文档最终裁决）

| 现有视图 | 归位 | 理由 |
| --- | --- | --- |
| SidebarPane（工作区卡+运行时卡） | 侧栏 **TaskPanel** 顶部（面板标题"工作区与任务"） | 信任与运行时就绪是任务创建的前置条件，同面板内聚；状态栏另有信任/连接摘要常显 |
| TaskView·创建表单 | 侧栏 **TaskPanel**（工作区卡之下） | 表单窄（现 420 → 侧栏约 340 可容），创建后视线自然转向中心区与底部轨迹 |
| TaskView·运行轨迹区 | 底部面板 **TracePanel**（含任务态徽章、取消按钮、取消方式、人工介入标记行、轨迹列表） | 对齐任务书"底部面板（运行轨迹等）"；轨迹是横贯性过程视图，不应占用侧栏或中心 |
| ReviewView | 中心全页 **ReviewSurface**（ActivityBar"审查"入口切换） | 文件列表+Diff 需要中心区宽度；审查保持只读、**不进** EditorArea（03 号边界：审查与编辑器互跳但不混合）；15 号在此挂"在编辑器中打开"跳转 |
| ConfigView | 中心全页 **ConfigSurface**（ActivityBar 底部"配置"入口） | 表单宽；对齐 VS Code"设置在中心区打开"的惯例 |
| HistoryView | 中心全页 **HistorySurface**（ActivityBar"历史"入口） | 双列列表需要宽度；只读低频 |
| HandoffDialog | 保持**模态对话框**，实例内嵌于 ReviewSurface（`parent: Overlay.overlay`），不占 ActivityBar | 交接是审查的收尾动作而非常驻视图；原 `handoffRequested` 信号级联取消，ReviewSurface 内部直开 |

### 4.5 各组件设计要点

#### 4.5.1 ActivityBar.qml / ActivityBarButton.qml

条目为注册表式描述数组（R1 §1.4），新增入口只改数组：

```qml
// ActivityBar.qml
readonly property var entries: [
    { id: "explorer", icon: "", title: "资源管理器",  kind: "sidebar" },
    { id: "task",     icon: "", title: "工作区与任务", kind: "sidebar" },
    { id: "review",   icon: "", title: "交付审查",    kind: "center"  },
    { id: "history",  icon: "", title: "交付历史",    kind: "center"  }
]
readonly property var bottomEntries: [
    { id: "config",   icon: "", title: "启动配置",    kind: "center"  }
]
```

- 图标用 `Theme.fontIcon`（"Segoe Fluent Icons"，Windows 10/11 内置；字形码以实测渲染为准，落地时可调，仅经 Theme 引用）。无图片资产、无 SVG 管线。
- `ActivityBarButton` 属性：`entryId/iconGlyph/title/active/badgeVisible`；active 视觉 = 左侧 2px `activityBarActiveBorder` 指示条 + `activityBarForeground` 前景；非 active 用 `activityBarInactiveForeground`；hover 背景用 `ghostElementHoverBackground`（透明底控件一律 ghost 族）。点击 → `shellVM.activate(entryId)`；tooltip 显示 `title` 与快捷键。
- active 判定：sidebar 类 = `shellVM.activeSideBarPanel === id && shellVM.sideBarVisible && shellVM.centerMode === "editor"`；center 类 = `shellVM.centerMode === id`。
- **徽章（壳层内建，非差异化）**：`review` 条目 `badgeVisible: taskVM.state === "review_ready"`；`task` 条目 `badgeVisible: taskVM.state === "awaiting_action"`。徽章为 `activityBarBadgeBackground` 实心圆点（直径 8）。15 号如需数字徽章在此扩展 `badgeCount`。

#### 4.5.2 SideBarHost.qml

- 头部（高 `Theme.sideBarHeaderHeight`）：当前面板标题（`sideBarTitleForeground`，`fontSizeSmall` 加粗）+ 右侧折叠 ToolButton（字形 ``，ghost 族）→ `shellVM.toggleSideBar()`。
- 内容：`StackLayout { currentIndex: shellVM.activeSideBarPanel === "explorer" ? 0 : 1 }`，两面板常驻不销毁（保表单输入状态）。
- 背景 `sideBarBackground`；与中心区之间仅 1px `border` 分隔，无阴影（R2 §3.4：平面区域靠边框弱分隔）。

#### 4.5.3 CenterHost.qml 与 EditorAreaSlot.qml

- CenterHost 为 `StackLayout`，四页常驻不销毁；页序固定 `editor(0) / review(1) / config(2) / history(3)`。
- **EditorAreaSlot 是 11 号的唯一插槽契约**：本组件占满 editor 页；首版内容为欢迎页；11 号落地时把欢迎页替换为 `EditorArea {}`（`qml/editor/EditorArea.qml`，11 号所有），除本文件外壳层不感知编辑器内部。

```qml
// EditorAreaSlot.qml（首版）
Rectangle {
    color: Theme.editorBackground
    // 11 号落地后：本占位内容整体替换为
    //   EditorArea { anchors.fill: parent }
    // 且此替换是 11 号对壳层的唯一改动点。
    ColumnLayout { // 欢迎页（诚实空态，R5 §6.1：不渲染裸空白）
        anchors.centerIn: parent
        Text { text: "Halo Studio"; color: Theme.descriptionForeground; font.pixelSize: Theme.fontSizeLarge }
        Text { text: wsHint }   // 三态：未打开工作区 → "在「工作区与任务」面板打开 Git 仓库"
                                //       未信任      → "工作区未信任，请先确认信任"
                                //       已信任      → "编辑器模块尚未就绪（11 号交付后此处为编辑器组）"
        Text { text: "Ctrl+Shift+E 资源管理器 · Ctrl+B 折叠侧栏 · Ctrl+J 运行轨迹" ... }
    }
}
```

#### 4.5.4 ExplorerPanel.qml（12 号槽位）

首版占位，三态诚实空态（不渲染裸空白）：无活动工作区 → 提示 + "转到工作区与任务"按钮（`shellVM.activate("task")`）；未信任 → 提示信任（同按钮）；已信任 → "资源管理器由文件系统模块提供（12 号交付后生效）"。12 号交付时整体替换本文件内容，文件路径与组件名不变（`qml/shell/panels/ExplorerPanel.qml` 即 12 号的挂载点）。

#### 4.5.5 BottomPanelHost.qml 与 TracePanel.qml

- 头部行：左 `TabBar`（首期唯一 `TabButton "运行轨迹"`；tab 序列即扩展位，后续模块以追加 TabButton + StackLayout 页方式接入）；右折叠 ToolButton（``）→ `shellVM.toggleBottomPanel()`。
- TracePanel = 现 TaskView 右半区整体迁移：任务态徽章 + 任务标题 + 取消按钮、最终取消方式行、人工介入标记行（TextField+按钮）、轨迹 ListView（含空态文案）。ViewModel 绑定不变（taskVM/traceVM）。
- **自动弹出规则**（Shell.qml 实现）：任务进入活跃态（`created/running/awaiting_action/finishing`）且底部面板隐藏时自动 `bottomPanelVisible=true`；同一 `taskVM.taskId` 内用户手动折叠后不再自动弹出（Shell.qml 持 `property string traceAutoRaiseSuppressedTaskId`）。不抢焦点。

#### 4.5.6 StatusBar.qml

高 `Theme.statusBarHeight`（26），背景 `statusBarBackground`，顶边 1px `border`。左右两组，项为 inline component `StatusBarItem`（Text/徽点，hover 用 ghost 族，可选 onClicked）：

| 位置 | 项 | 数据源 | 说明 |
| --- | --- | --- | --- |
| 左 1 | Sidecar 连接圆点 + "已连接/未连接" | `appVM.sidecarConnected` | **常显**（保留现状） |
| 左 2 | "协议 v1 / —" | `appVM.protocolVersion` | **常显**（保留现状） |
| 左 3 | 工作区信任徽章（受信任/未信任/无工作区） | `workspaceVM.active/trustState` | 新增摘要位；点击 → `shellVM.activate("task")` |
| 左 4 | 任务状态徽章（利用 `Util.taskStateLabel/Tone`） | `taskVM.state` | 点击 → 显示底部面板 |
| 左 5 | "不可用原因：…"（fillWidth，ElideRight） | `appVM.unavailableReason` | **常显**（保留现状） |
| 右 1 | 归因/差异化挂点（空 Item，objectName `statusBarDifferentiationSlot`） | 15 号 | §5 |
| 右 2 | "行 N，列 M"（编辑器状态位） | `editorService.cursorLine/cursorColumn` | `typeof editorService === "undefined" || cursorLine < 1` 时隐藏 |
| 右 3 | 底部面板开关 ToolButton（``，active 高亮） | `shellVM.bottomPanelVisible` | 点击 → toggle |

#### 4.5.7 Shell.qml

- 组合 §4.2 实例树；持有 `mainSplit/centerSplit` 并实现尺寸回写：`onResizingChanged: if (!resizing) { shellVM.storeSideBarWidth(sideBarHost.width); shellVM.storeBottomPanelHeight(bottomPanel.height) }`。
- 实现 §4.5.5 自动弹出规则（`Connections { target: taskVM }`）。
- 预留命令面板浮层挂点：`property Item modalOverlayHost`（objectName `shellModalOverlayHost`，一个盖满 Shell 的空 Item）——13 号的 QuickInput 浮层以此为父项，本文不设计其内容。

### 4.6 设计语言与 Theme token 全集

#### 4.6.1 原则（自 R1 §5 / R2 §3.5 提炼，对全部 QML 生效）

1. 组件只引用 `Theme.*` token；**禁止裸色值、禁止自由字号/间距数值**（新代码硬约束，迁移代码逐步收敛）。
2. 暗色首发：token 结构即"语义名 → 值"平表；亮色远期以第二张值表切换，属性名不变。
3. 实底控件用 `element*` 族，透明底控件（图标按钮、标签页、状态栏项、树行）用 `ghostElement*` 族，两族状态色不得混用。
4. 阴影只表达层级：仅弹层/模态（elevated/quickInput/Dialog）可用 `widgetShadow`；平面区块之间一律 1px `border`/`borderVariant` 分隔。
5. 编辑器背景（`editorBackground`）独立于面板背景，视觉突出内容区。
6. 派生色在 Theme.qml 内以 `Qt.alpha/Qt.darker/Qt.lighter` 从根 token 计算或用 `#AARRGGBB` 字面量，不在使用方现配。

#### 4.6.2 `qml/theme/Theme.qml` 属性清单（全集，暗色值）

文件骨架：

```qml
pragma Singleton
import QtQuick

QtObject {
    // ---- 字体与排版 ----
    readonly property string fontUi: "Segoe UI"
    readonly property string fontMono: "Consolas"
    readonly property string fontIcon: "Segoe Fluent Icons"
    readonly property int fontSizeXSmall: 10
    readonly property int fontSizeSmall: 12
    readonly property int fontSizeDefault: 14
    readonly property int fontSizeLarge: 16
    readonly property real lineHeightUi: 1.45
    readonly property real lineHeightMono: 1.5
    // ---- 间距 / 圆角 / 尺寸 ----（见表）
    // ---- 颜色 token ----（见表；一族一段，字母序）
}
```

**排版 / 间距 / 圆角 / 尺寸（22 项）**

| token | 值 | 说明 |
| --- | --- | --- |
| fontUi / fontMono / fontIcon | "Segoe UI" / "Consolas" / "Segoe Fluent Icons" | 双字体体系 + 图标字体（R2 §3.2） |
| fontSizeXSmall / Small / Default / Large | 10 / 12 / 14 / 16 | 仅四档（R2）；密集面板正文用 Small=12（延续现状） |
| lineHeightUi / lineHeightMono | 1.45 / 1.5 | 相对行高；lineHeightMono 供 11 号编辑器 |
| spaceXxs / Xs / Sm / Md / Lg / Xl / Xxl | 2 / 4 / 6 / 8 / 12 / 16 / 24 | 离散间距刻度，禁止自由数值（R2 §3.3） |
| radiusSmall / radius / radiusLarge | 4 / 6 / 8 | radius 与现状一致 |
| activityBarWidth | 48 | |
| statusBarHeight | 26 | 现 32 → 26，IDE 密度 |
| sideBarMinWidth | 180 | |
| sideBarHeaderHeight | 34 | |
| bottomPanelMinHeight | 96 | |
| tabHeight | 32 | 供 11 号标签页 |

**基础色（12 项）**

| token | 值 |
| --- | --- |
| foreground | #e6e8eb |
| descriptionForeground | #9aa1a9 |
| placeholderForeground | #6f767e |
| disabledForeground | #5c636b |
| iconForeground | #c8cdd2 |
| accent | #4f8cff |
| focusBorder | #4f8cff |
| border | #3a4048 |
| borderVariant | #2c313a |
| widgetShadow | #59000000 |
| selectionBackground | #404f8cff |
| linkForeground | #6ca8ff |

**状态色（4 项，沿用现名）**：`ok #3fb950`、`warn #d29922`、`danger #f85149`、`neutral #8b949e`

**表面层级（5 项，R2 surface 族）**

| token | 值 | 用途 |
| --- | --- | --- |
| appBackground | #1b1e23 | 窗口底 |
| editorBackground | #14161a | 编辑器/只读 Diff/输入底（最深，突出内容区） |
| panelBackground | #1e2126 | 底部面板 |
| surfaceBackground | #22262d | 卡片/侧栏区块 |
| elevatedSurfaceBackground | #2b3038 | 菜单/弹层/QuickInput |

**element / ghostElement 双状态族（8 项，R2 §3.1）**

| token | 值 |
| --- | --- |
| elementBackground | #2b3038 |
| elementHoverBackground | #343b46 |
| elementActiveBackground | #3c4452 |
| elementSelectedBackground | #31405c |
| elementDisabledBackground | #23272e |
| ghostElementHoverBackground | #0fffffff |
| ghostElementActiveBackground | #1affffff |
| ghostElementSelectedBackground | #244f8cff |

**区块（14 项）**

| token | 值 |
| --- | --- |
| activityBarBackground | #171a1f |
| activityBarForeground | #e6e8eb |
| activityBarInactiveForeground | #7d8590 |
| activityBarActiveBorder | #4f8cff |
| activityBarBadgeBackground | #4f8cff |
| activityBarBadgeForeground | #ffffff |
| sideBarBackground | #1e2126 |
| sideBarSectionHeaderBackground | #23272e |
| sideBarTitleForeground | #c8cdd2 |
| statusBarBackground | #22262d |
| statusBarForeground | #c8cdd2 |
| editorGroupHeaderTabsBackground | #1b1e23 |
| editorGroupBorder | #3a4048 |
| panelBorder | #3a4048 |

**标签页（7 项，供 11 号消费）**

| token | 值 |
| --- | --- |
| tabActiveBackground | #14161a |
| tabInactiveBackground | #1b1e23 |
| tabActiveForeground | #e6e8eb |
| tabInactiveForeground | #9aa1a9 |
| tabActiveBorderTop | #4f8cff |
| tabBorder | #2c313a |
| tabDirtyForeground | #d29922 |

**编辑器（10 项，供 11 号消费；11 号不得另设色值）**

| token | 值 |
| --- | --- |
| editorForeground | #e6e8eb |
| editorLineHighlight | #0affffff |
| editorLineNumberForeground | #5c6773 |
| editorActiveLineNumberForeground | #c8cdd2 |
| editorSelectionBackground | #404f8cff |
| editorFindMatchBackground | #66d29922 |
| editorFindMatchHighlightBackground | #33d29922 |
| editorCursorForeground | #e6e8eb |
| editorWhitespaceForeground | #3a4048 |
| editorGutterBackground | #14161a |

**列表 / 树（8 项，供 12 号资源管理器与各列表消费）**

| token | 值 |
| --- | --- |
| listActiveSelectionBackground | #2d3a52 |
| listActiveSelectionForeground | #e6e8eb |
| listInactiveSelectionBackground | #262b33 |
| listHoverBackground | #14ffffff |
| listFocusOutline | #4f8cff |
| listHighlightForeground | #6ca8ff |
| treeIndentGuidesStroke | #2c313a |
| listWarningForeground | #d29922 |

**输入 / 快速打开（6 项，供 13 号消费）**

| token | 值 |
| --- | --- |
| inputBackground | #14161a |
| inputForeground | #e6e8eb |
| inputBorder | #3a4048 |
| inputValidationErrorBorder | #f85149 |
| quickInputBackground | #2b3038 |
| pickerGroupForeground | #6ca8ff |

**差异 / 归因（4 项，供审查与 15 号消费）**

| token | 值 |
| --- | --- |
| diffInsertedBackground | #263fb950 |
| diffRemovedBackground | #26f85149 |
| gutterAgentChangeBackground | #8c4f8cff |
| baselineChangedBadgeForeground | #d29922 |

**文件装饰（4 项，供 12/15 号徽章通道消费，语义对应 vscode gitDecoration.\*）**

| token | 值 |
| --- | --- |
| decorationModifiedForeground | #6ca8ff |
| decorationAddedForeground | #3fb950 |
| decorationDeletedForeground | #f85149 |
| decorationIgnoredForeground | #5c636b |

合计约 90 项。

#### 4.6.3 旧 Theme → 新 token 机械替换表（迁移用）

| 旧（components/Theme.qml） | 新 | 备注 |
| --- | --- | --- |
| background | appBackground | |
| deep | editorBackground（Diff/只读区）或 inputBackground（输入底） | 按语义二选一 |
| surface | surfaceBackground | |
| surfaceAlt | elevatedSurfaceBackground（弹层）或 elementBackground / listActiveSelectionBackground（控件/选中行） | 按语义选择 |
| text | foreground | |
| textDim | descriptionForeground | |
| border / accent / ok / warn / danger / neutral / radius | 同名保留 | |
| monoFont | fontMono | |
| 裸 `font.pixelSize: 11/12/13/14` | fontSizeXSmall(10~11)/fontSizeSmall(12~13)/fontSizeDefault(14) | 就近归档 |

`components/qmldir` 删除 `singleton Theme 1.0 Theme.qml` 行；各文件新增 `import "../theme"`（components 内为 `import "../theme"`，shell/panels、shell/surfaces 内为 `import "../../theme"`）。Main.qml 的 `palette.*` 全部改为映射 Theme token（window→appBackground、base→editorBackground、button→elementBackground、highlight→accent 等），保证 Fusion 控件与 token 单一来源。

### 4.7 布局状态持久化（QSettings）

| 键 | 类型 | 默认 | 写入时机 |
| --- | --- | --- | --- |
| `shell/activeSideBarPanel` | str | "task"（首启引导打开工作区） | activate 变更后（500ms 合并） |
| `shell/sideBarVisible` | bool | true | 同上 |
| `shell/bottomPanelVisible` | bool | false | 同上 |
| `shell/sideBarWidth` | int | -1（QML 动态默认 `min(340, w/4)`） | 拖动结束（`resizing→false`） |
| `shell/bottomPanelHeight` | int | -1（QML 动态默认 `h/3`） | 同上 |

- `centerMode` **不持久化**：每次启动回到 "editor"（诚实、可预期；避免启动即落在过期审查页）。
- `AppContext.shutdown()` 追加调用 `shellVM.flush()`。
- 测试经构造注入 `QSettings(tmpfile, QSettings.IniFormat)`，不污染注册表。
- 布局状态属界面偏好，**不进** halo.db（R2 §2.3）。

### 4.8 快捷键绑定层（细节引用 13 号，不重复设计）

**挂载结构**：`qml/shell/ShortcutHost.qml`（Main.qml 顶层实例化）。

- **首版（10 号落地时，13 号未到位）**：内联 `Shortcut { context: Qt.ApplicationShortcut }` 列表，直连 `shellVM`，数据源为本文件内的静态表。
- **13 号落地后**：本模块命令改在 Python 侧经约定 API `commandRegistry.register(id, title, category, callback, shortcut)` 注册（callback 闭包捕获 shellVM）；ShortcutHost 全文替换为 `Instantiator { model: commandRegistry.commands; delegate: Shortcut { sequence: model.shortcut; onActivated: commandRegistry.execute(model.id) } }`，内联表删除。该替换由 13 号实施，本文预先把快捷键表收敛在单文件内以便搬迁。
- 冲突解决、和弦、用户覆盖、when 上下文一律 13 号裁决，本文不设计。

本模块命令 id 与默认键（category "视图"，13 号注册表沿用）：

| 命令 id | 标题 | 默认键 | 行为 |
| --- | --- | --- | --- |
| `view.toggleSideBar` | 切换侧栏 | Ctrl+B | `shellVM.toggleSideBar()` |
| `view.togglePanel` | 切换底部面板 | Ctrl+J | `shellVM.toggleBottomPanel()` |
| `view.showExplorer` | 显示资源管理器 | Ctrl+Shift+E | `shellVM.activate("explorer")` |
| `view.showTask` | 显示工作区与任务 | Ctrl+Shift+A | `shellVM.activate("task")` |
| `view.showReview` | 显示交付审查 | Ctrl+Shift+R | `shellVM.activate("review")` |
| `view.showHistory` | 显示交付历史 | Ctrl+Shift+H | `shellVM.activate("history")` |
| `view.showConfig` | 显示启动配置 | Ctrl+, | `shellVM.activate("config")` |

为 13 号**预留**（本文不实现、不占用）：`palette.show`（Ctrl+Shift+P）、`quickOpen.show`（Ctrl+P）、跳转行（Ctrl+G）；浮层父项挂点见 §4.5.7。

### 4.9 线程与数据流

- 壳层状态全部在 UI 主线程：ShellViewModel 无 IPC、无后台线程；QSettings 写为本地小量同步写（合并后频率极低）。
- 数据流单向：ActivityBar/快捷键 → `shellVM.activate/toggle*` → 属性 notify → QML 绑定重算。QML 不直接写 shellVM 属性（尺寸经 `store*` slot）。
- 既有 ViewModel 的绑定关系不变（各面板/Surface 仍经 root context 属性取 VM）；Shell 只做布局路由，不碰契约数据——**无业务旁路**纪律不变。

---

## 5. 差异化点（仅接口挂点，裁决权归 15 号）

| 差异化功能（03 号 §范围内 5） | 本文预留挂点 |
| --- | --- |
| 审查→编辑器跳转 | `ReviewSurface` 增加 `signal openInEditorRequested(string path, int line)`（文件行动作触发；首版不接线）。15 号裁决 UX 后连接 `editorService.openFile(path, line)` |
| 基线感知徽章 | 装饰类 token（`decoration*Foreground`、`baselineChangedBadgeForeground`）已就绪；徽章渲染归 12 号树/11 号标签页，数据与裁决归 15 号 |
| 归因边栏（gutter） | `gutterAgentChangeBackground` token 已就绪；渲染归 11 号，数据与裁决归 15 号 |
| 任务上下文选择器 | `TaskPanel` 任务说明表单区预留空 Item 挂点（objectName `taskContextSelectorSlot`），15 号在此注入"从资源管理器/编辑器添加"入口 |
| 人工介入自动归因 | 无壳层挂点（属 11 号保存链路 + Sidecar）；状态栏右侧 `statusBarDifferentiationSlot`（§4.5.6）供 15 号放置归因提示位 |

---

## 6. 实施计划

### 6.1 阶段与依赖顺序

| 阶段 | 内容 | 依赖 |
| --- | --- | --- |
| P1 | `qml/theme/`（Theme.qml + qmldir）落地；删除 `components/Theme.qml`；全量 import 与 token 机械替换（§4.6.3） | 无 |
| P2 | `viewmodels/shell.py` + `app/tests/test_viewmodels_shell.py`；`app.py` 装配（shellVM 特判 + shutdown flush） | 无（与 P1 并行可） |
| P3 | `qml/shell/` 骨架：Shell/ActivityBar(+Button)/SideBarHost/CenterHost/EditorAreaSlot/BottomPanelHost/StatusBar/ShortcutHost | P1、P2 |
| P4 | 视图迁移（§6.2）：panels/surfaces/HandoffDialog + Main.qml 改造 + 删除 `qml/views/` | P3 |
| P5 | 冒烟与回归：`--smoke`、pytest 全绿、`scripts/test-all.ps1` 全绿 | P4 |

对其他模块：**10 号先行落地**。11 号唯一改动点 = EditorAreaSlot 内容替换 + 挂载 `editorService`（含 §3.2-4 状态位）；12 号唯一挂载点 = ExplorerPanel 内容替换；13 号接管 ShortcutHost + 使用 `shellModalOverlayHost`；15 号使用 §5 挂点。均不改 Shell 结构。

### 6.2 迁移改造清单（逐文件）

| 现文件（qml/ 下） | 处置 | 具体改造点 |
| --- | --- | --- |
| `Main.qml` | **改造** | 删除 TabBar/StackLayout/RowLayout 布局与 HandoffDialog 实例；contentItem 换 `Shell {}`、顶层加 `ShortcutHost {}`、footer 换 `shell/StatusBar.qml`；palette 改映射新 token；窗口尺寸/最小尺寸/标题保留 |
| `components/Theme.qml` | **移动+扩展** | → `theme/Theme.qml`：按 §4.6.2 扩展为 token 全集；旧属性按 §4.6.3 处理（保留同名项，删除改名项） |
| `components/qmldir` | **修改** | 移除 `singleton Theme` 行，其余保留 |
| `components/util.js` | **保留** | 无改动（既有映射函数继续被 TracePanel/StatusBar/各 Surface 使用） |
| `components/SectionCard.qml` | **保留+token 化** | `import "../theme"`；surface→surfaceBackground；标题字号 fontSizeSmall 加粗 |
| `components/StatusBadge.qml` | **保留+token 化** | 字号→fontSizeSmall |
| `components/DiffViewer.qml` | **保留+token 化** | deep→editorBackground；monoFont→fontMono；只读红线注释与 readOnly 恒真不动 |
| `components/ErrorLabel.qml` | **保留+token 化** | danger 不变，字号归档 |
| `components/RuntimeCard.qml` | **保留+token 化** | 被 TaskPanel 使用；逻辑不变 |
| `views/SidebarPane.qml` | **拆分迁移后删除** | 工作区卡（含 identity_changed 警示、信任/撤销/打开按钮、ErrorLabel）与两张 RuntimeCard、刷新按钮 → `shell/panels/TaskPanel.qml` 顶部区；布局宽度适配侧栏（去 330/360 硬宽，Layout.fillWidth） |
| `views/TaskView.qml` | **拆分迁移后删除** | 左半（创建表单全部控件与创建逻辑）→ `TaskPanel.qml` 下半区（表单纵向排列，预留 `taskContextSelectorSlot`）；右半（任务态行/取消/人工介入/轨迹 ListView/空态）→ `shell/panels/TracePanel.qml`，改横向利用底部面板宽度（轨迹列表 fillWidth，控制行置顶） |
| `views/ReviewView.qml` | **移动改造** → `shell/surfaces/ReviewSurface.qml` | 根改 `ColumnLayout` 外加 `anchors.margins: Theme.spaceLg` 的容器；删除 `signal handoffRequested`，内嵌 `HandoffDialog { id: handoffDialog }` 并在"创建交接包…"onClicked 直接赋参并 open；新增 `signal openInEditorRequested(string path, int line)`（§5，首版不接线）；只读红线（DiffViewer readOnly）与"仅最新证据可决定"逻辑不动；token 化 |
| `views/ConfigView.qml` | **移动** → `shell/surfaces/ConfigSurface.qml` | 仅加外边距容器 + token 化；凭据红线文案与逻辑一字不动 |
| `views/HistoryView.qml` | **移动** → `shell/surfaces/HistorySurface.qml` | 同上 |
| `views/HandoffDialog.qml` | **移动** → `shell/HandoffDialog.qml` | import 路径改（`"../components"`、`"../theme"`）；逻辑不变；实例点从 Main.qml 移入 ReviewSurface |

### 6.3 新建文件清单（Python / 测试 / 文档）

- `app/halo_studio/viewmodels/shell.py`（§4.3）
- `app/halo_studio/app.py`：`VIEWMODEL_SPECS` 追加 `("shellVM", "ShellViewModel")`；装配循环对 shellVM 走无 client 构造分支；`AppContext.shutdown()` 先 `shellVM.flush()` 再关 client
- `app/tests/test_viewmodels_shell.py`
- 评审通过后：module-contracts.md 第 8/10 节按 §3.2 追加；`docs/design/README.md` 状态更新

---

## 7. 测试计划

**单元（pytest，无 Qt 界面）**

- `ShellViewModel.activate` 全迁移表（§4.3 表 4 行 × 各入口，含非法 id 忽略、重复激活折叠、center 再点返回 editor）。
- 持久化 round-trip：注入 IniFormat 临时 QSettings → 改属性 → `flush()` → 新实例恢复一致；`sideBarWidth=-1` 默认语义；`centerMode` 不落盘、重启恒 "editor"。
- 合并落盘：连续变更只在 flush/定时器后产生一次写（可直接断言 flush 后键值，定时器路径用 `QTimer` 触发或直接调私有落盘方法）。

**装配（pytest-qt + fake_sidecar）**

- `assemble()` 产出含 shellVM 的 10 个 context 属性；shellVM 无 client 构造成功；`shutdown()` 调用 flush 不抛异常。
- 既有 viewmodel/connection 测试全绿（回归线：`scripts/test-all.ps1`，cargo 248 + pytest 57 基线上只增不减）。

**QML/冒烟**

- `python -m halo_studio --smoke` 保持 SMOKE-OK（Main.qml 装载 Shell 全树成功即根对象存在）。
- 冒烟增强：Main.qml 根对象暴露 `readonly property int shellProbeActivityCount`（绑定 ActivityBar 条目数=5）与 `readonly property bool shellProbeStatusBarReady`，`--smoke` 路径断言（在 main.py 冒烟分支读根对象属性），防"加载成功但壳层空白"。
- 手动验收脚本（scripts/dev.ps1 起真身）核对清单：五入口切换/再点折叠与返回、侧栏与面板尺寸拖动后重启恢复、任务运行自动弹出轨迹且手动折叠后不再弹、状态栏三件套常显、editorService 缺席时光标位隐藏、全部快捷键。

**红线回归**

- 全仓文本断言：`qml/**` 无 `#`+6/8 位十六进制裸色值（Theme.qml 除外）——加一个 pytest 静态扫描测试固化 §4.6.1-1。
- 审查只读：ReviewSurface 的 DiffViewer readOnly 恒真断言保留（既有测试路径迁移后更新引用路径）。

---

## 8. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| QSettings 默认写注册表，测试污染环境 | 测试脏状态、并行冲突 | 构造注入 IniFormat 临时文件（§4.7）；生产路径不变 |
| SplitView 附加属性恢复时机（preferredWidth 在首帧后写入无效） | 尺寸恢复失灵 | preferredWidth 用声明式绑定（`shellVM.sideBarWidth>0 ? … : 动态默认`），不在 onCompleted 命令式写；拖动只在 `resizing→false` 回写，避免绑定回环 |
| import 路径与 token 改名的大面积机械替换出错 | 视图静默丢样式/加载失败 | §4.6.3 机械表 + 冒烟探针（§7）+ 裸色值静态扫描；一次提交内完成，不留混用中间态 |
| 11/12/13 号落地顺序倒挂（editorService/commandRegistry/Explorer 缺席） | 壳层空槽报错 | 全部 `typeof` 守卫 + 三态诚实空态（R5）；缺席=功能隐藏而非异常 |
| Segoe Fluent Icons 字形码与预期不符 | 图标显示为豆腐块 | 字形仅经 `Theme.fontIcon`+条目数组引用，实测调整只改数组；最坏回退单字文本 |
| TaskView 拆分（表单/轨迹分居两区）破坏既有 pytest-qt 测试选择器 | 回归失败 | 迁移保持控件 id/objectName 与 VM 绑定不变；测试只更新文件路径引用 |
| 底部面板自动弹出打扰用户 | 体验反感 | 仅"隐藏→显示"单向、不抢焦点、同任务被手动折叠后抑制（§4.5.5） |
| statusBarHeight 32→26 与既有截图/验收材料不一致 | 验收争议 | 属设计语言统一的显式决定，记录于本文；验收以本文为准 |

---

## 修订记录

- 2026-07-27：首版（03 号对齐记录触发；R1/R2/R5 分析为输入）。
