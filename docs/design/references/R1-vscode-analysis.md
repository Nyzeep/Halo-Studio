# R1 - VS Code 源码分析：工作台布局 / 编辑器组 / 命令面板 / 键绑定 / 主题令牌 / 资源管理器

**参考项目**：`D:\用于参考的开源项目的代码\vscode-main`（MIT 许可；本文只提炼概念、架构与算法思想，不复制任何源码）
**分析日期**：2026-07-27
**服务对象**：`docs/design/10-ide-shell-and-design-language.md`、`11-editor-core.md`、`12-fs-contract-and-explorer.md`、`13-command-palette-and-quick-open.md`
**边界**：遵循 `requirements-alignment/03-ide-editor-and-reference-alignment.md` —— 无 WebView/Monaco、无扩展市场、无嵌入终端；审查保持只读；所有文件访问经 Sidecar `fs.*`。

---

## 1. Workbench 布局体系

### 1.1 Parts：固定枚举的顶层区块

入口 `src/vs/workbench/browser/layout.ts`。工作台把窗口划分为**固定枚举的 Parts**（非自由停靠）：
`titlebar / banner / activitybar / sidebar / editor / panel / auxiliarybar / statusbar`，各自实现于 `src/vs/workbench/browser/parts/` 下的独立目录。

核心抽象（`src/vs/workbench/browser/part.ts`）：

- `Part` 是"标题区 + 内容区"的抽象组件，同时实现 `ISerializableView`——即**每个 Part 直接就是网格布局的一个视图节点**，向布局系统暴露 `minimumWidth/maximumWidth/minimumHeight/maximumHeight` 与 `onDidChange`（尺寸约束变化时触发重排）。
- Part 创建后向 `IWorkbenchLayoutService.registerPart` 注册；布局服务是唯一的布局权威，Part 自身不知道自己在网格中的位置。

关键联动机制：

- **可见性切换不销毁 Part**：`setPartHidden` 走 `workbenchGrid.setViewVisible(view, !hidden)`，网格会**缓存隐藏视图的最后尺寸**（`getViewCachedVisibleSize`），再次显示时按原尺寸恢复。同时给根容器加 CSS 类（如 `nosidebar`）驱动样式联动。
- **Parts 间联动收敛在布局服务**：例如侧栏隐藏时若其含有活动视图，需把焦点交还编辑器；panel 隐藏而编辑器也隐藏时强制恢复编辑器可见（`applyOverrides` 中的启动兜底，layout.ts 约 3082 行）——即"不允许所有中央区块同时不可见"这类**全局不变量由布局服务统一裁决**，而不是各 Part 自查。

### 1.2 Grid/SplitView：嵌套分割 + 可序列化

- 底层是 `src/vs/base/browser/ui/splitview/splitview.ts`（一维分割容器，视图带 `minimumSize/maximumSize/priority/snap`——priority 决定窗口缩放时谁先吸收增量，snap 允许拖到最小值后"吸附收起"）。
- `src/vs/base/browser/ui/grid/grid.ts` 的 `Grid` 是**方向交替嵌套的 SplitView 树**；`SerializableGrid` 增加 `serialize()/deserialize()`：布局是一棵 `{type: 'branch'|'leaf', size, visible, data}` 的 JSON 树，叶子节点存 `{type: Parts.XXX}`，恢复时用 `fromJSON` 回调把类型映射回 Part 实例（layout.ts `createWorkbenchLayout()`，约 1633–1673 行）。
- 初始网格由 `createGridDescriptor()`（约 2638 行）**按当前状态手工构建序列化树**：垂直根 = [titlebar, banner, 中段, statusbar]；中段由 `arrangeMiddleSectionNodes()`（约 2563 行）根据 panel 位置（bottom/left/right/top）、sidebar 位置（left/right）、panel 对齐（center/left/right/justify）组合出活动栏/侧栏/编辑器/panel/辅助栏的嵌套关系。编辑器节点尺寸 = 总宽减去各兄弟可见尺寸（隐藏的算 0）。

### 1.3 尺寸与状态持久化：LayoutStateModel

layout.ts 尾部的 `LayoutStateModel`（存储前缀 `workbench.`）值得整体借鉴：

- 把布局状态定义为**带类型的键对象**，分两类：
  - `RuntimeStateKey`：运行期可变且要在会话间保留（如 `sideBar.hidden`、`panel.position`、`sideBar.position`、zenMode 状态），带 `scope`（WORKSPACE / PROFILE 两级）与 `target`；
  - `InitializationStateKey`：只在启动时读取的初始化值（如 `sideBar.size`、`panel.size`、`auxiliaryBar.size`）。
- **尺寸不是实时写库**：注册 `storageService.onWillSaveState`，在"将要保存状态"时机一次性把网格里的真实尺寸（含隐藏视图的缓存尺寸）回写到 state key（约 1702–1725 行），避免拖动过程频繁持久化。
- 默认值是**动态计算**的：`SIDEBAR_SIZE.defaultValue = min(300, 容器宽/4)`、`PANEL_SIZE = 高/3`（`load()`，约 2990 行）——首启不同分辨率下都有合理比例。
- zenMode 用 `zenModeIgnore` 标记的键在 zen 期间不落盘，退出后恢复原布局——**"临时布局模式不污染持久状态"** 的通用手法。

### 1.4 侧栏内容组织：ViewContainer / PaneComposite

活动栏图标 ≠ 面板本身：活动栏条目对应 **ViewContainer**（视图容器），侧栏用 `paneCompositePart.ts` 按位置（Sidebar / Panel / AuxiliaryBar）承载当前活动容器，同一容器可在位置间移动。对 Halo 首期的启示是**"活动栏条目 → 侧栏视图"用注册表驱动而非硬编码 if/else**，但无需实现视图拖动重排。

---

## 2. 编辑器组与标签页模型

### 2.1 三层抽象：EditorInput / EditorPane / EditorGroup

- **EditorInput**（`src/vs/workbench/common/editor/editorInput.ts`）：轻量的"待打开内容"句柄。关键成员：`typeId`（序列化恢复用）、`resource: URI`（**仅作身份判断**，不能当显示名）、`capabilities` 位掩码（Readonly/Untitled/Singleton 等）、`isDirty()/isModified()/isSaving()`、`save()/saveAs()/revert()`、`matches()`（同一资源去重复用已开标签的依据）、`onDidChangeDirty/onDidChangeLabel` 事件、可选 `closeHandler`（自定义关闭确认）。
- **EditorPane**（`src/vs/workbench/browser/parts/editor/editorPane.ts`）：真正的渲染部件，生命周期注释写明：`createEditor() → setEditorVisible() → layout() → setInput() → focus() → dispose()`；一生只 create/dispose 一次，中间被反复 `clearInput/setInput` **复用**——同类型编辑器换文件不重建控件，这是标签页切换流畅的关键。
- **EditorGroup**：`editorGroupView.ts`（UI）+ `src/vs/workbench/common/editor/editorGroupModel.ts`（纯模型，可独立测试）。编辑器区自身也是一个 `SerializableGrid`（`editorPart.ts`，组网格独立于 Parts 网格，状态存 `editorpart.state` memento），实现分屏。

### 2.2 EditorGroupModel：pinned / preview / sticky / MRU

`editorGroupModel.ts` 是**无 DOM 依赖的纯状态机**，维护四个核心结构：顺序列表、MRU 列表、单一 `preview` 指针、`sticky` 索引（0..sticky 均为 sticky）。语义：

- **preview（预览标签，斜体）**：单击文件以预览方式打开；**一个组同一时刻至多一个 preview**，打开新预览会**替换**旧预览标签（约 369–383 行）；编辑内容、双击标签或显式 pin 则转正（`pin()` 把 preview 置 null，约 796–803 行）。这让"浏览一串文件"不会堆出一排标签。
- **pinned**：常驻标签。`sticky` 隐含 pinned（`makePinned = options?.pinned || options?.sticky`）。
- **sticky（固定区）**：始终占据标签栏最前段；新打开的非 sticky 编辑器的目标索引会被强制排到 sticky 区之后（约 313–316 行）；移动编辑器跨越 sticky 边界时自动增减 sticky 计数。
- **MRU**：独立维护"最近使用"序，关闭活动标签时按 MRU 而非相邻序选下一个活动者；也是 Ctrl+Tab 快速切换的数据源。

### 2.3 脏状态与关闭确认

`editorGroupView.ts` 的 `doHandleCloseConfirmation`（约 1781–1886 行）：

1. 若编辑器开启了"焦点切换即自动保存"类模式，直接按 SAVE 处理**不弹窗**（弹窗本身会抢焦点触发保存，避免自相矛盾）；
2. 否则弹三态对话框：**保存 / 不保存 / 取消**（`ConfirmResult.SAVE / DONT_SAVE / CANCEL`）；
3. SAVE：执行 `editor.save()`，失败则回到第 2 步重新确认（不静默丢失）；
4. DONT_SAVE：先 `revert()` 恢复内容，**若 revert 后仍脏则否决关闭**（veto 机制——关闭流程返回 boolean veto，任何一步失败都能拦住关闭）；
5. 同一资源在多个组打开时只在最后一个副本关闭时确认（`shouldConfirmClose` 结合 `closeHandler`）。

**借鉴要点**：关闭是"可否决的异步流程"而非同步删除；脏判断持续依赖 `isDirty()` 实时值而非缓存快照。

---

## 3. 命令面板与快速打开

### 3.1 命令注册：CommandsRegistry 与 Action2

- `src/vs/platform/commands/common/commands.ts`：`CommandsRegistry` 本质是 `Map<commandId, LinkedList<ICommand>>`——同 id 允许叠注册，**新注册 unshift 到链头生效，销毁后自动回退到旧实现**；`ICommand = {id, handler(accessor, ...args), metadata}`，metadata 的 `description` 即命令面板可搜索文案。执行统一走 `ICommandService.executeCommand(id, ...args)`，配有 onWill/onDid 事件。
- `src/vs/platform/actions/common/actions.ts` 的 `Action2`/`registerAction2` 把**命令 + 标题/分类 + 菜单出现位置 + 默认键绑定 + precondition（when 表达式）**捆绑为一次声明——命令面板、菜单、键绑定三者共享一份注册信息，避免三处漂移。

### 3.2 QuickInput / QuickAccess：一个输入框，多个前缀 Provider

- `src/vs/platform/quickinput/common/quickAccess.ts` + `browser/quickInputController.ts`：只有一个 QuickInput 浮层控件（输入框 + 高亮列表 + busy 指示），**按输入前缀路由到 Provider**：`''` = 文件快速打开（anythingQuickAccess），`>` = 命令面板，`:` = 跳转行，`?` = 帮助（列出全部前缀）。注册见 `src/vs/workbench/contrib/quickaccess/browser/quickAccess.contribution.ts`。
- Provider 契约极简：`provide(picker, token)`——拿到 picker 自己填 `items`/订阅 `onDidChangeValue`/`onDidAccept`，返回 disposable；取消令牌贯穿异步计算。`pickerQuickAccess.ts` 提供了"输入变化 → （可取消地）重算 picks"的模板基类。
- 命令面板（`src/vs/platform/quickinput/browser/commandsQuickAccess.ts`）排序策略：**最近使用过的命令带计数器排最前（"recently used" 分组）→ 其余按模糊得分/字母序（"other commands"）**；输入 ≥3 字符时对未命中模糊匹配的命令跑 TF-IDF 相似度补充"similar commands"分组（首期可不做）。命令使用历史 `CommandsHistory` 持久化。

### 3.3 fuzzyScorer：打分算法核心思想（`src/vs/base/common/fuzzyScorer.ts`）

这是最值得完整移植的算法，全文 ≈900 行，无任何 DOM/平台依赖：

**A. 单串打分 `scoreFuzzy(target, query)`**：query × target 的 DP 矩阵（类 LCS）。每格：字符不匹配得 0；匹配则由 `computeCharScore` 计分后取 `diag + score` 与左值较大者，同时在平行矩阵记录**连续匹配长度**。约束：非首个 query 字符必须 diag 有分才允许得分（保证按序匹配，杜绝 "ede" 中 "de" 的乱序高分，约 92–99 行注释）。回溯从右下角还原匹配位置序列。

**B. 单字符得分维度**（`computeCharScore`，约 158–236 行）：

| 维度 | 加分 |
| --- | --- |
| 字符命中（忽略大小写；`/` 与 `\` 互认） | +1 |
| 大小写也一致 | +1 |
| 连续匹配加权 | 前 3 个连续 +6/个，之后 +3/个（长串衰减） |
| target 首字符命中 | +8 |
| 前一字符是路径分隔符 `/` `\` | +5 |
| 前一字符是 `_ - . 空格 ' " :` | +4 |
| 词内大写（camelCase，如 NPE→NullPointerException），仅在非连续段生效 | +2 |

**C. 条目打分 `scoreItemFuzzy(label, description, path)`**（约 396–555 行）：分层基准分把类别彻底隔开——
路径完全一致 `1<<18` ≫ label 前缀匹配 `1<<17`（再加 `round(query长度/label长度*100)` 的"短名奖励"，使 window 查询下 window.ts 胜过 windowActions.ts）≫ label 内模糊命中 `1<<16` ≫ 仅在 description+label 拼接串上命中（无基准分）。查询含路径分隔符时才优先在全路径上匹配。结果缓存以 hash(label+description+query) 为键。

**D. 排序比较器 `compareItemsByFuzzyScore`**（约 623–683 行）的决胜链：身份分 → 高分 → 更紧凑的匹配区间 → 更短的 label → 有 label 命中者优先 → 匹配距离 → label+description 更短 → 路径更短 → 字典序。

**E. 查询预处理 `prepareQuery`**（约 853–916 行）：空格切多段（**每段都必须命中**，段间分数相加、高亮合并去重）；`"引号"` 包裹段要求连续匹配（禁用模糊）；Windows 下把 `/` 归一为 `\`（反之亦然）；剥离通配符/省略号/空白。

### 3.4 文件快速打开的工程细节（`anythingQuickAccess.ts` 思想）

编辑器历史（已打开过的文件）与全工作区文件搜索**合并排序**，历史命中优先；打分缓存跨键入复用；每次键入用 CancellationToken 废弃上一轮异步搜索。

---

## 4. 键绑定体系

### 4.1 注册与权重

`src/vs/platform/keybinding/common/keybindingsRegistry.ts`：

- 规则 = `{id(命令), primary/secondary 键码, win/mac/linux 平台特化, weight, when}`；注册时即按当前平台展开。
- `KeybindingWeight` 分层：`EditorCore(0) < EditorContrib(100) < WorkbenchContrib(200) < BuiltinExtension(300) < ExternalExtension(400)`，用户绑定永远最高。**冲突不报错，靠权重 + 注册序决定胜者**。
- `registerCommandAndKeybindingRule` = 命令处理器与键绑定一次注册。
- 用户可用 `-commandId` 规则**移除**默认绑定（`KeybindingResolver.handleRemovals`）；移除规则的 when 与默认规则的 when 用"蕴含"而非全等比较，默认规则收紧后移除仍生效。

### 4.2 解析与分发（`keybindingResolver.ts`）

- 构建 `Map<首个和弦按键串, 候选规则[]>`；`resolve(context, currentChords, keypress)` 返回三态：**无匹配 / 还需更多和弦（MoreChordsNeeded）/ 命中命令**。多和弦（如 `Ctrl+K Ctrl+S`）即"已按和弦序列是某规则前缀"时进入等待态。
- `_findCommand` **从候选列表尾部向前**找第一个 when 满足者——后注册者（用户覆盖）天然优先（约 380–392 行）。
- when 为 `false` 常量的规则在建表时即剔除。

### 4.3 when 上下文（`src/vs/platform/contextkey/common/contextkey.ts`）

- `ContextKeyExpr`：小型布尔表达式 AST（`and/or/not/==/!=/>/正则`），支持字符串序列化/反序列化（如 `"editorFocus && !readonly"`）。
- `RawContextKey<T>` 声明一个键（带默认值与说明），经 `IContextKeyService` 绑定后 `set/reset`；上下文服务是**树形作用域**——子部件（某个编辑器、某棵树）创建 scoped context，求值时沿父链回退。键值变化 → 重估相关表达式 → 驱动键绑定可用性与菜单可见性。
- **思想内核**：把"UI 处于什么状态"（焦点在哪、面板开没开、任务是否运行中）统一抽象为可声明、可组合的键值快照，键绑定/命令/菜单全部声明式引用它，杜绝散落的 if。

---

## 5. 主题与颜色令牌

### 5.1 注册模型（`src/vs/platform/theme/common/colorUtils.ts`）

- `registerColor(id, defaults, description)` → 全局颜色注册表。`id` 用 `区域.语义` 命名（`editor.background`、`tab.activeBackground`）。
- `defaults` 按四种基调分别给默认值：`{light, dark, hcDark, hcLight}`；默认值可以是：具体颜色、**另一个令牌的引用**（形成派生链，如 `panel.background` 直接引用 `editorBackground`）、或 **ColorTransform 变换**：`Darken/Lighten/Transparent/Opaque/OneOf/LessProminent/IfDefinedThenElse/Mix`（带系数）。主题只需覆盖少量根令牌，其余令牌沿引用链与变换自动派生——**这是少量定义覆盖全 UI 的关键设计**。
- 每个令牌自动生成 CSS 变量 `--vscode-<id 的 . 换 ->`；部件样式只引用变量，换主题零重绘逻辑。

### 5.2 令牌分层

- 平台层 `src/vs/platform/theme/common/colors/`：`baseColors.ts`（foreground/focusBorder/contrastBorder/描述文本等 20 个）、`editorColors.ts`（96 个）、`listColors.ts`（37）、`inputColors.ts`（49）、`menuColors.ts`、`quickpickColors.ts`、`minimapColors.ts` 等——**控件语义**令牌。
- 工作台层 `src/vs/workbench/common/theme.ts`：**区块语义**令牌——`tab.activeBackground`（默认= editorBackground）、`tab.inactiveBackground`、`editorGroupHeader.tabsBackground`、`sideBar.background`、`activityBar.background`、`statusBar.background`（区分有无工作区/调试态）、`panel.background`、`titleBar.activeBackground` 等。

### 5.3 Halo QML Theme 单例应抽取的令牌集合

按"根令牌 + 派生"思想，首期建议 **~55 个令牌**（名称沿用 vscode 语义便于回查，camelCase 化）：

- **基础（8）**：`foreground`、`descriptionForeground`、`errorForeground`、`focusBorder`、`contrastBorder`、`widgetShadow`、`selectionBackground`、`iconForeground`
- **编辑器（10）**：`editorBackground`、`editorForeground`、`editorLineHighlight`、`editorLineNumberForeground`、`editorActiveLineNumberForeground`、`editorSelectionBackground`、`editorFindMatchBackground`、`editorFindMatchHighlightBackground`、`editorCursorForeground`、`editorWhitespaceForeground`
- **区块（12）**：`titleBarActiveBackground`、`activityBarBackground`、`activityBarForeground`、`activityBarActiveBorder`、`sideBarBackground`、`sideBarSectionHeaderBackground`、`panelBackground`、`panelBorder`、`statusBarBackground`、`statusBarForeground`、`editorGroupHeaderTabsBackground`、`editorGroupBorder`
- **标签页（7）**：`tabActiveBackground`、`tabInactiveBackground`、`tabActiveForeground`、`tabInactiveForeground`、`tabActiveBorderTop`、`tabBorder`、`tabDirtyForeground`（Halo 自定名，承载脏点）
- **列表/树（8）**：`listActiveSelectionBackground`、`listActiveSelectionForeground`、`listInactiveSelectionBackground`、`listHoverBackground`、`listFocusOutline`、`listHighlightForeground`（模糊匹配高亮）、`treeIndentGuidesStroke`、`listWarningForeground`
- **输入/快速打开（6）**：`inputBackground`、`inputForeground`、`inputBorder`、`inputValidationErrorBorder`、`quickInputBackground`、`pickerGroupForeground`（分组分隔文字）
- **差异/归因（4，对接审查与归因边栏）**：`diffInsertedBackground`、`diffRemovedBackground`、`gutterAgentChangeBackground`（Halo 自定）、`baselineChangedBadgeForeground`（Halo 自定）
- **Git/状态徽章（4）**：`decorationModifiedForeground`、`decorationAddedForeground`、`decorationDeletedForeground`、`decorationIgnoredForeground`（对应 vscode `gitDecoration.*` 语义）

QML 落法：`Theme` 单例（`pragma Singleton`）暴露以上属性 + `light/dark` 两组默认表；派生令牌用 JS 函数（`Qt.darker/Qt.lighter/Qt.alpha`）对根令牌变换实现，不逐个写死。

---

## 6. 文件资源管理器 UX

### 6.1 树交互（`src/vs/workbench/contrib/files/browser/views/explorerViewer.ts`）

- 基于**异步数据树**（`WorkbenchCompressibleAsyncDataTree`）：目录内容按需解析、展开时才拉子项——契合 Halo "一切经 `fs.*` IPC"的模式（`fs.list_dir` 惰性调用）。
- **压缩空链目录**（`CompressedNavigationController`）：`a/b/c` 单链折叠为一行，行内可键盘左右在段间移动。首期可不做，但树模型宜预留"一行多段"能力。
- **就地重命名/新建**：不弹对话框，在树行内把 label 替换为 `InputBox`（`renderInputBox`，约 1032 行起），实时校验非法名并在行下方浮出错误消息。
- 键入即过滤高亮：树自带 FuzzyScore 高亮（`filterData` 传入渲染器，命中字符加粗）。
- 单击预览（对应 preview 标签）、双击/回车固定打开——树与编辑器组的 preview 语义联动。

### 6.2 徽章与装饰：统一 Decorations 通道

- `src/vs/workbench/services/decorations/common/decorations.ts`：任何来源（Git、诊断、Halo 自有状态）都以 `IDecorationData = {weight, color(颜色令牌 id), letter(单字符徽章), tooltip, strikethrough, bubble}` 贡献装饰；`bubble: true` 使**子项徽章上浮到祖先目录**（折叠的目录也能看出内部有变更）。多来源装饰按 weight 合并。
- Git 侧（`extensions/git/src/decorationProvider.ts`）用字母徽章 `M/A/D/U/C` + `gitDecoration.*` 前景色同时作用于资源管理器行与编辑器标签；忽略文件用降饱和前景色。
- 打开的脏文件：标签页关闭按钮位置显示实心圆点（●），资源管理器行不重复显示脏点——脏与 Git 状态是两个正交通道。

**对 Halo 的映射**：装饰通道天然承载 03 号记录的差异化功能——"基线感知徽章"（任务基线以来已变更 → letter `M` + 颜色令牌 + bubble 上浮）与"运行中任务的 Agent 关联变更"标记，数据源自 Sidecar 的基线 diff 事件而非 UI 自算 git status。

---

## 7. 对 Halo Studio 的可落地借鉴清单（≤10 条）

> 规模评估口径：S ≤ 300 行；M ≈ 300–1000 行；L ≈ 1000–2500 行（含测试；QML+Python 合计）。

1. **固定 Parts + 嵌套 SplitView 壳层**（§1.1/1.2）——`Main.qml` 重构为 `TitleBar / ActivityBar / SideBar / EditorArea / BottomPanel / StatusBar` 六个固定区块，用 QML `SplitView` 两层嵌套（垂直根 + 中段水平）实现；各区块可隐藏但不销毁（`visible:false` + 记忆宽度），不做自由停靠/多窗口/banner/zen。→ 设计文档 10。**规模 L**（壳层 QML ~800 行 + LayoutViewModel ~300 行）。
2. **LayoutStateModel 式布局持久化**（§1.3）——Python 侧 `LayoutState`：typed key（含 workspace/global 两个 scope）、隐藏区块缓存尺寸、退出/失焦时批量落盘（QSettings 或经 store）、默认值按窗口尺寸动态计算（侧栏 `min(300, w/4)`、面板 `h/3`）。→ 设计文档 10。**规模 S–M**。
3. **EditorGroupModel 纯 Python 状态机**（§2.2）——顺序表 + MRU + 单 preview + pinned 语义（首期不做 sticky、不做分屏，但模型留组抽象）：单击预览复用同一标签（斜体）、编辑/双击转正、同资源 `matches()` 去重、关闭活动标签按 MRU 选继任。配 pytest 全迁移表测试，QML `TabBar` 只做渲染。→ 设计文档 11。**规模 M**（模型 ~400 行 + 测试 ~300 行 + QML ~200 行）。
4. **三态关闭确认 + veto 流程**（§2.3）——关闭标签/关闭工作区是可否决异步流程：脏 → 保存/不保存/取消；保存失败回到确认而非静默关闭；`fs.write` 失败必须拦住关闭。Python 协程 + QML Dialog。→ 设计文档 11。**规模 S**。
5. **EditorPane 复用生命周期**（§2.1）——QML 编辑器控件一次创建、`setDocument()` 换内容复用，文档模型（文本 + 脏 + 版本）与视图分离；只读审查视图与可编辑器共用文档抽象（capabilities=readonly 即审查跳转打开的形态）。→ 设计文档 11。**规模 M**。
6. **CommandRegistry + when 上下文迷你版**（§3.1/4.3）——Python 单例注册表：`{id, title, category, handler, precondition, keybinding}` 一次声明；上下文服务首期只要**平面键值 + and/not 求值**（`editorFocus`、`explorerFocus`、`quickOpenVisible`、`taskRunning`、`workspaceTrusted` ~10 个键），不做树形作用域与序列化解析器。命令面板/快捷键/菜单可用性同源。→ 设计文档 13。**规模 M**（~500 行 + 测试）。
7. **fuzzyScorer 算法移植（Python）**（§3.3）——完整移植打分维度（连续匹配 6/3 衰减、首字符 +8、路径分隔符 +5、其他分隔符 +4、camelCase +2）、三层基准分（路径一致 ≫ label 前缀+短名奖励 ≫ label 模糊 ≫ 路径模糊）、决胜链、空格多段与引号精确段、Windows 分隔符归一。纯函数无 Qt 依赖，可对照 vscode 用例出测试。→ 设计文档 13。**规模 M**（~400 行 + 测试 ~300 行）。
8. **单浮层 QuickInput + 前缀 Provider 路由**（§3.2）——一个 QML 浮层（输入框 + 高亮列表 + busy），Provider 协议 `provide(picker, cancel_token)`；首期三个 Provider：`''` 文件快速打开（`fs.list_tree` + 已打开历史优先）、`>` 命令面板（最近使用分组置顶 + 持久化历史，不做 TF-IDF）、`:` 跳转行。键入即取消上一轮异步。→ 设计文档 13。**规模 L**（QML ~400 行 + Python ~600 行）。
9. **两级权重键绑定 + 和弦缓冲**（§4.1/4.2）——注册表分 `core < user` 两级权重，同键后注册胜出；解析器返回 无匹配/等待和弦/命中 三态（支持 `Ctrl+K Ctrl+O` 类双和弦，状态栏提示等待中）；用户覆盖走配置文件，支持 `-command` 移除语法。不做平台特化展开（Windows 首发）。→ 设计文档 10/13。**规模 M**。
10. **颜色令牌注册表 + 装饰通道**（§5/6.2）——QML `Theme` 单例落地 §5.3 的 ~55 个令牌（根令牌 + `Qt.darker/lighter/alpha` 派生，light/dark 双表）；资源管理器与标签页共用 `Decoration{letter,colorToken,tooltip,bubble}` 模型，数据源为 Sidecar 基线 diff 与任务证据（承载"基线感知徽章"与"归因边栏"差异化功能）。→ 设计文档 10/12/15。**规模 M**（Theme ~250 行 + 装饰模型 ~300 行）。

### 明确不借鉴

扩展宿主与贡献点机制、多窗口（auxiliary window）、视图拖拽重排/自由停靠、WebView 类编辑器、TF-IDF 相似命令、zen mode、banner part、遥测埋点、树形 scoped context 与 when 序列化解析器（首期用平面键值即可）。

---

## 修订记录

- 2026-07-27：首版（对齐记录 03 触发的 R1 分析）。
