# R2 - Zed 源码分析（原生编辑器 / 设计语言 / 面板停靠 / Agent 面板）

**参考项目**：`D:\用于参考的开源项目的代码\zed-main`（GPL 系许可证：**只提炼概念、架构与协议事实，绝不复制源码**；本文引用的路径均为该仓库内相对路径，仅供回查）
**服务对象**：`docs/design/10-ide-shell-and-design-language.md`、`11-editor-core.md`、`12-fs-contract-and-explorer.md`、`13-command-palette-and-quick-open.md` 及 15 号差异化功能设计
**边界提醒**：依据 03 号对齐记录——审查保持只读、无 WebView/终端/扩展市场；Zed 的协作（CRDT 多副本）、GPUI 渲染器、Vim、扩展体系均**不在借鉴范围**。

---

## 1. 编辑器分层：从 SumTree 到 EditorElement

### 1.1 crate 分层总览

Zed 编辑器是一个自底向上的严格分层栈，每层只依赖下层，且各层都以**不可变快照（Snapshot）**为对外接口：

| 层 | crate | 职责 | 关键类型 |
| --- | --- | --- | --- |
| 通用数据结构 | `crates/sum_tree` | B+ 树（`TREE_BASE=6`），节点带可求和摘要（Summary），支持按任意维度（Dimension）O(log n) 定位 | `SumTree<T>`、`Cursor`、`Summary` |
| 文本存储 | `crates/rope` | `Rope = SumTree<Chunk>`，同时维护字节偏移 / UTF-16 偏移 / 行列点三套坐标（`src/point.rs`、`offset_utf16.rs`） | `Rope`、`Point`、`OffsetUtf16` |
| 编辑语义 | `crates/text` | `text::Buffer`：编辑操作、历史/撤销（`undo_map.rs`）、**锚点**（`anchor.rs`）、编辑订阅（`subscription.rs`）、`Patch`（编辑区间代数） | `Buffer`、`Anchor`、`Patch` |
| 语言感知缓冲区 | `crates/language`（`src/buffer.rs`） | 包装 `text::Buffer`，附加文件关联（`saved_mtime`/`saved_version` → 脏状态与外部冲突检测）、Tree-sitter 语法映射、诊断、自动缩进 | `language::Buffer`、`SyntaxMap` |
| 多缓冲聚合 | `crates/multi_buffer` | 把多个缓冲区的**摘录（Excerpt）**拼成一个逻辑文档（diff 审查、搜索结果、诊断列表都用它） | `MultiBuffer`、`Excerpt`、`ExcerptRange` |
| 显示变换 | `crates/editor/src/display_map.rs` | 缓冲区坐标 → 显示坐标的纯变换管线 | `DisplayMap` 及五层子 map |
| 编辑器状态 | `crates/editor/src/editor.rs` | 选区、滚动、查找、补全等**状态**（不做布局绘制） | `Editor` |
| 渲染元素 | `crates/editor/src/element.rs` | 每帧从快照布局并绘制**仅可见行** | `EditorElement` |

### 1.2 值得记录的三个核心思想

**（a）状态与渲染彻底解耦。** `Editor`（editor.rs，约 1.2 万行）只持有状态；`EditorElement`（element.rs）在每帧渲染时读取 `DisplayMap` 快照，只对 `visible_display_row_range` 内的行做布局与绘制（element.rs 中所有 paint 均以该区间裁剪），行布局结果还有全局 `LineLayoutCache`（`crates/gpui/src/text_system.rs`）复用。渲染路径上没有任何可变编辑状态。

**（b）显示变换是分层纯函数管线。** `display_map.rs` 头部文档明确了管线顺序：`InlayMap → FoldMap → TabMap → WrapMap → BlockMap`。每层都有统一模式：
- 一个 `Transform`（管辖的文本区域，分"透传/替换"两类变体）；
- 一个 `TransformSummary { input, output }`（下层文本摘要 → 本层输出摘要）；
- 一个 `Snapshot` 类型和 `sync(snapshot, edits) -> (new_snapshot, new_edits)` 函数——**edits 在这里的语义是"失效区域"**，逐层换算坐标向上传播，从而做到只重算受影响区间。
- 每层引入自己的坐标 newtype（`FoldPoint`、`WrapRow`、`BlockPoint`…），层间转换函数显式命名（`<A>_point_to_<B>_point`），坐标空间混用在类型层面不可能发生。

**（c）锚点（Anchor）让位置在编辑后保持稳定。** `crates/text/src/anchor.rs`：`Anchor = (插入操作时间戳, 该插入内的字节偏移, bias 左/右, buffer_id)`。它不记录绝对偏移，而是挂靠在"某次插入的文本"上，因此任何后续编辑都不需要修正锚点，解析时按当前快照重算绝对位置。选区、诊断、折叠、diff hunk、Agent 编辑位置全部以 Anchor 表达。

### 1.3 对 QML 原生编辑器（QQuickTextDocument 简化实现）的启示

我们首期用 `QQuickTextDocument`（内部是 QTextDocument 的 piece table + 块链表），**不需要**自研 rope/CRDT，但以下概念应当保留：

1. **快照单向数据流**：编辑器 UI 只读视图模型快照（打开文件列表、脏状态、归因行集合），编辑操作单向提交，避免 QML 双向绑定导致的状态回环；
2. **锚定而非行号**：归因边栏、审查跳转、基线徽章都不要存裸行号——QTextDocument 的 `QTextBlock`/`QTextCursor` 在编辑后自动维持位置，等价于"块级 Anchor"，首期够用；
3. **坐标空间显式命名**：缓冲区行（文件真实行）与显示行（折行后的可视行）在类型/属性名上分开，即便首期不做代码折叠，soft wrap 一开就会出现两套坐标；
4. **显示装饰独立成层**：语法高亮（QSyntaxHighlighter）、归因 gutter、诊断下划线各自独立维护、独立失效，不要互相耦合在一个绘制回调里。

---

## 2. Workspace / Pane / Dock 体系

### 2.1 结构（`crates/workspace/src/`）

```
Workspace (workspace.rs:1379)
├── title_bar
├── left_dock / bottom_dock / right_dock : Dock     (dock.rs)
│     └── panel_entries: Vec<PanelEntry>            // 每个 dock 挂多个 Panel，同一时刻只显示 active_panel_index 一个
├── center: PaneGroup                               (pane_group.rs)
│     └── root: Member = Axis(PaneAxis) | Pane      // 递归二叉/多叉分屏树
│           PaneAxis { axis, members: Vec<Member>, flexes: Vec<f32> }   // 每个成员的伸缩比例
│           Pane { items: Vec<Box<dyn ItemHandle>>, active_item_index,
│                  nav_history, toolbar, preview_item_id, pinned_tab_count }  (pane.rs:398)
├── status_bar : StatusBar                          (status_bar.rs)  // 左右两组 item，dock 开关按钮在这里
├── modal_layer                                      // 命令面板/文件查找等模态浮层
└── toast_layer / notifications
```

- **Item 协议**（`item.rs:170` `trait Item`）：标签内容 `tab_content`/`tab_content_text`、图标、tooltip、脏状态事件、导航恢复 `navigate`；`ItemHandle` 做类型擦除后进 Pane。编辑器、diff 视图、任何中心区内容都实现同一 trait。
- **Panel 协议**（`dock.rs:36` `trait Panel`）：`persistent_name`（持久化键）、`position` / `position_is_valid` / `set_position`（面板可在合法 dock 间移动，位置写回用户设置）、`default_size` / `min_size`、`icon` / `icon_tooltip` / `toggle_action`（全局开关动作）、`activation_priority`、缩放 `set_zoomed`、`starts_open`。面板事件仅四种：`ZoomIn / ZoomOut / Activate / Close`。
- **Pane 细节**：预览标签（`preview_item_id`，单击预览、双击固定——同 VS Code preview tab）、固定标签数 `pinned_tab_count`、每 Pane 独立导航历史 `nav_history`、标签栏渲染可被宿主替换（`render_tab_bar` 闭包）。

### 2.2 与 VS Code 的关键差异

| 维度 | VS Code | Zed | 对我们的含义 |
| --- | --- | --- | --- |
| 侧边入口 | Activity Bar 固定竖条 | **无 Activity Bar**：dock 开关按钮放在**状态栏**两端（workspace.rs:1747-1759 把 `PanelButtons` 加进 StatusBar） | 两种都成立；03 号已定 Activity Bar（随 R1），但 Zed 证明"状态栏承载面板开关"能省一列宽度，可作远期紧凑模式 |
| 面板归属 | 视图容器绑定位置，拖拽重排较重 | 面板自报 `position_is_valid`，可整体在左/右/底 dock 间迁移，且**同一 dock 内多面板互斥显示** | 我们的侧栏视图（任务/审查/交接/配置/历史）适合 Zed 式"一 dock 多面板互斥 + 图标切换"，模型更简单 |
| 组织机制 | Contribution points + DI 注册 | Rust trait（`Panel`/`Item`）+ 显式注册 | QML 里对应"面板描述对象数组 + Loader"，无需插件式注册框架 |
| 分屏模型 | Grid（editorGroupsService） | `Member/PaneAxis` 递归树 + `flexes` 比例数组 | 树形模型更容易在 QML 里用嵌套 SplitView 表达 |
| 面板缩放 | 最大化编辑器/面板命令 | 任意 Panel/Pane 可 `zoom`（占满中心区，`zoom_layer_open`） | 远期锦上添花 |

### 2.3 持久化

`workspace/src/persistence.rs` 把 dock 开关状态、面板尺寸、分屏树（含 flexes）、每 Pane 的 item 列表序列化进 SQLite，按工作区恢复。我们的等价物：按 `workspace_id` 在 UI 层本地存储布局 JSON（不进 Sidecar 的 halo.db，属界面偏好而非交付数据）。

---

## 3. 设计语言：令牌、字体、间距、层级

### 3.1 语义颜色令牌（`crates/theme/src/styles/colors.rs`）

`ThemeColors` 是一张**扁平的语义令牌表**（百余项），命名分族，组件代码禁止出现裸色值：

- **border 族**：`border / border_variant（弱化分隔）/ border_focused / border_selected / border_transparent / border_disabled`；
- **surface 族**：`background`（应用底）/ `surface_background`（面板、标签）/ `elevated_surface_background`（菜单、弹窗）/ `editor_background` 独立；
- **element 族与 ghost_element 族**：真正的设计洞察——有底色的控件（element_*）与**透明底控件**（ghost_element_*）各配一整套 `hover / active / selected / disabled` 状态色。工具栏图标按钮、标签页这类"贴在表面上的"控件用 ghost 族，永不与实底按钮混用；
- **text/icon 族**：`text / text_muted / text_placeholder / text_disabled / text_accent`，icon 同构；
- **状态与部件**：`status_bar.background`、`tab.active_background / tab.inactive_background`、`editor.line_number / editor.active_line_number`、`scrollbar.*`、`search.match_background` 等（见 `assets/themes/one/one.json`）。

主题文件（`assets/themes/one/one.json`）就是这张表的 JSON 赋值 + `syntax` 语法表（每个语法 token 一个 `{color, font_style, font_weight}`，token 名如 `comment / comment.doc / keyword / string / function / type / attribute`），主题=数据，无代码。

### 3.2 字体与排版（`crates/ui/src/styles/typography.rs`、`units.rs`）

- **双字体体系**：UI 字体与缓冲区（等宽）字体是两个独立设置，`font_ui()` / `font_buffer()` 两个入口；Agent 面板里的代码/diff 用 buffer 字体，其余用 UI 字体；
- **rem 基准缩放**：`BASE_REM_SIZE_IN_PX = 16`，一切尺寸经 `rems_from_px(px)` 表达，用户改 `ui_font_size` / `buffer_font_size` 即全局等比缩放；
- **UI 字号只有四档**：Large 16 / Default 14 / Small 12 / XSmall 10（px @ 默认缩放）。没有更多档位——这是"极简"的可操作定义之一。

### 3.3 间距与密度（`crates/ui/src/styles/spacing.rs`）

`DynamicSpacing` 宏生成三档密度（Compact / Default / Comfortable）的间距枚举，变体按"默认密度下的像素值"命名（`Base08` = 默认 8px）。小间距逐项手调（如 `(1,2,4)`、`(4,8,10)`），大间距按公式 `(n-4, n, n+4)` 推导。要点：**间距是离散刻度表，不是自由数值**。

### 3.4 层级（`crates/ui/src/styles/elevation.rs`）

五层语义 z 轴：`Background → Surface → EditorSurface → ElevatedSurface（菜单/弹层）→ ModalSurface（对话框/命令面板）`。Surface 与 EditorSurface **无阴影**，只有 Elevated/Modal 才有投影——阴影表达"物理距离"，不做装饰。

### 3.5 可抽取的设计原则（供 10 号文档引用）

1. 组件只引用语义令牌；主题是纯数据文件；
2. 实底控件与透明底控件两套状态色族，杜绝"悬停色到处现配"；
3. 双字体 + rem 缩放 + 四档字号 + 离散间距刻度；
4. 阴影仅表达层级（弹层/模态），平面区域靠 `border_variant` 弱分隔，不加投影；
5. 编辑器背景独立于面板背景（`editor.background` vs `surface.background`），视觉上突出"内容区"。

---

## 4. Agent 面板：编辑器优先产品中的会话 / 工具调用 / diff 审查

### 4.1 呈现形态

- `AgentPanel`（`crates/agent_ui/src/agent_panel.rs:1153`）实现 `Panel` trait 停靠在左/右 dock（`position_is_valid: != Bottom`），默认右侧，宽度可调可 zoom——**会话是舷窗，编辑器是主体**；
- 面板序列化"当前活动线程 + 选中 agent"，重启恢复（`serialize()`），与我们"中断如实标记、不自动恢复任务"的语义不同但不冲突（它恢复的是视图，不是运行）。

### 4.2 会话数据模型（`crates/acp_thread/src/acp_thread.rs`）

会话是**类型化条目序列**，不是消息流文本：

```
AgentThreadEntry = UserMessage | AssistantMessage | ToolCall
                 | Elicitation（结构化询问，Pending{respond_tx}/Accepted/Declined）
                 | CompletedPlan(Vec<PlanEntry>) | ContextCompaction
ToolCall { id, label(Markdown), kind, content, status, locations,
           raw_input, raw_output, … }
ToolCallStatus = Pending | WaitingForConfirmation{options, respond_tx}
               | InProgress | Completed | Failed | Rejected | Canceled
```

渲染规则（`conversation_view/thread_view.rs:8084 render_tool_call`）：
- 编辑类 / 终端类 / 待确认的工具调用用**卡片布局**，其余轻量行内展示；
- 卡片默认折叠，`Disclosure` 展开原始输入/输出；`WaitingForConfirmation` 强制展开并内联渲染授权按钮；
- 编辑类工具调用的卡片内**内嵌真实 diff 编辑器**（每条目缓存 `entry_view_state`），`locations` 支持点击跳转到编辑器对应位置。

### 4.3 diff 审查（`agent_ui/src/agent_diff.rs` + `crates/action_log`）

- `ActionLog`（`action_log/src/action_log.rs`）按缓冲区跟踪 `unreviewed_edits: Patch`——Agent 每次编辑记入未审集合，人工在编辑器里直接改动会相应合并/抵销；
- `AgentDiffPane` 用 **MultiBuffer** 把所有已变更缓冲区的 diff hunk 摘录聚合为单一可滚动审查视图（`ExcerptRange { context, primary }`：context 是带上下文行的展示区间，primary 是真正变更区间）；
- 审查动作是 hunk 粒度的 `Keep / Reject / KeepAll / RejectAll`，Reject 会**写回缓冲区**恢复原文。

### 4.4 与 Halo Studio 任务/轨迹/审查视图的对照

| Zed | Halo Studio | 结论 |
| --- | --- | --- |
| AgentThreadEntry 类型化条目 | 运行轨迹（task.phase / 操作请求 / 验证状态事件） | **借鉴**：轨迹视图模型应是类型化条目 ListModel + 分类渲染（阶段行 / 工具卡片 / 请求卡片），而非日志文本流 |
| ToolCallStatus.WaitingForConfirmation 内联授权 | Agent 操作请求（awaiting_action） | **部分借鉴**：借鉴"驻留卡片 + 高亮待办"的呈现；但 Halo 不代理决策——按 CONTEXT.md，用户经 Pi/OpenCode **原生通道**响应，我们的卡片只展示与引导，不放批准按钮 |
| ActionLog 未审编辑集 + Keep/Reject 写回缓冲区 | 交付审查（只读 Diff + 接受/拒绝整份交付） | **明确不借鉴写回**：03 号边界锁定审查只读、接受/拒绝不动 Git/文件；hunk 级 Keep/Reject 与我们的交付粒度结论冲突 |
| MultiBuffer 聚合多文件 diff 摘录 | 审查视图文件列表 + 逐文件 Diff | **借鉴概念**：单滚动流内"文件分节 + 带上下文行的变更摘录"，比逐文件切换的审查效率高；QML 可用分节 ListView 实现，无需真正的 MultiBuffer |
| 编辑内嵌 diff + locations 跳转 | 审查→编辑器跳转（03 号差异化 5.3） | **印证既有决定**：跳转定位用文件+行锚，落在编辑器而非审查视图内编辑 |
| 面板停靠可移可缩 | 任务/审查等侧栏视图 | **借鉴**：Agent 相关视图作为右 dock 面板，编辑器始终居中 |

---

## 5. 项目模型：worktree 扫描与模糊查找

### 5.1 Worktree（`crates/worktree/src/worktree.rs`）

- `Snapshot`（:176）：`entries_by_path: SumTree<Entry>`（按路径序）+ `entries_by_id`，双索引；`scan_id` / `completed_scan_id` 两个水位表达"扫描进行中/已完成"，UI 永远读快照，不等扫描；
- `Entry`（:3901）：`id / kind / path / inode / mtime / size / is_ignored / is_hidden / is_external / is_private / char_bag`——注意 `char_bag` 在扫描时就预计算好，为模糊匹配服务；
- `BackgroundScanner`：初扫用 `num_cpus` 个并行任务（:5108），此后由文件系统事件驱动增量重扫（`process_events`）；**gitignore 目录与隐藏目录懒扫描**——仅当用户在资源管理器展开时才扫（Entry 注释明确此策略），并被排除出搜索；
- `EntryKind::UnloadedDir / PendingDir` 把"未加载"建模进树里，资源管理器可先渲染再补数据。

### 5.2 模糊匹配（`crates/fuzzy/src/`）

- **CharBag 预过滤**（`char_bag.rs`）：把候选串压成一个 `u64` 位图（每个字母 2 bit 计数、数字 1 bit、`-` 1 bit）；查询同样成 bag，`is_superset` 一次位运算即可排除绝大多数候选，之后才进昂贵的打分；
- **打分**（`matcher.rs`）：动态规划矩阵，距离惩罚（`BASE_DISTANCE_PENALTY 0.6`，递增 0.05 封顶 0.2）鼓励连续匹配，`smart_case`、路径末段加权；
- **工程化**（`paths.rs`、`file_finder/src/file_finder.rs`）：候选按 worktree 分集、按 CPU 分片并行打分；每次输入变更把上一轮的 `cancel_flag: Arc<AtomicBool>` 置位（file_finder.rs:1075-1086），旧任务尽快退出——**取消令牌 + 后台并行**是打字不卡的关键；
- 命令面板（`crates/command_palette`）与文件查找共用同一 `Picker/PickerDelegate` 抽象与同一打分器：**一个 picker 组件 + 多个 delegate**。

### 5.3 对 Halo 的映射

我们的文件访问必须走 Sidecar `fs.*`（03 号），因此：目录树 = `fs.list` 增量 + 变更事件；快速打开候选 = Sidecar 提供一次性全量相对路径清单（受工作区牢笼与 ignore 规则约束），匹配打分放在 **Python 线程池**（每键取消）即可支撑万级文件；`char_bag` 用 Python int 位运算等价实现，成本极低。

---

## 6. 大文件与性能策略要点

1. **只布局可见行**：`EditorElement` 一切布局/绘制以 `visible_display_row_range` 裁剪；行布局有 `LineLayoutCache` 跨帧复用（gpui/src/text_system.rs:365）；
2. **异步解析 + 同步快路径**：Tree-sitter 重解析先在 `sync_parse_timeout` 内同步尝试（小编辑立即高亮），超时则丢到后台线程，完成后若版本已前进就再排一次（language/src/buffer.rs:1915-1961）——高亮永不阻塞输入；
3. **软折行后台计算**：`WrapMap` 持有 `background_task`，大文件折行不阻塞渲染（display_map/wrap_map.rs:224）；
4. **单行长度护栏**：`MAX_LINE_LEN = 1024`（editor/src/editor.rs:296）限制若干逐行操作的成本；超长行靠折行兜底；
5. **快照 + 持久化数据结构**：SumTree 节点 `Arc` 共享，克隆快照 O(1)，后台任务各拿各的快照，无锁竞争；
6. **编辑即失效区间**：`Patch/Edit` 沿显示管线逐层换算，只重算受影响区间（见 1.2b）；
7. **扫描与搜索的懒与断**：ignored/隐藏目录懒扫描；模糊匹配 CharBag 预过滤 + 取消令牌（见 5.1/5.2）。

**Halo 首期对应**：`fs.read` 大小上限 + 二进制检测已在 03 号契约中（超限只读提示，不进编辑器）；QSyntaxHighlighter 天然按块增量高亮，但须给"单块超长"设护栏（超过阈值该行跳过高亮）；资源管理器与快速打开照第 5 节做懒加载与取消。

---

## 7. 对 Halo Studio 的可落地借鉴清单（≤10 条）

| # | 借鉴点（来源小节） | QML / PySide6 映射 | 分级 |
| --- | --- | --- | --- |
| 1 | **状态/渲染解耦 + 快照单向数据流**（1.2a、1.3）：编辑器状态归视图模型，QML 只读快照、单向提交编辑 | `EditorViewModel`（Python，持文档句柄/脏状态/查找状态）+ QML `TextArea/QQuickTextDocument` 纯渲染；装饰（高亮、gutter）各自独立失效 | **首期** |
| 2 | **锚定而非行号**（1.2c、1.3）：归因边栏、审查跳转、基线徽章挂 QTextBlock 级锚点，编辑后位置自动维持 | `QTextCursor`/`QTextBlock.userData` 存归因标记；证据行号仅在打开文件瞬间解析为锚点一次 | **首期**（差异化 15 号依赖） |
| 3 | **语义颜色令牌表 + element/ghost_element 双状态族 + 五层 elevation**（3.1、3.4） | QML `Theme` singleton：扁平令牌属性 + 主题 JSON 加载；控件只引令牌；仅弹层/模态有阴影 | **首期**（10 号文档核心输入） |
| 4 | **双字体体系 + 基准缩放 + 四档字号 + 离散间距刻度**（3.2、3.3） | `Theme.fontUi` / `Theme.fontMono` 两字体；尺寸经 `Theme.scale(px)` 统一缩放；间距/字号枚举常量，禁止自由数值 | **首期**（密度三档为**远期**） |
| 5 | **Panel 协议化停靠**（2.1）：面板自描述 `name/icon/toggleAction/defaultSize/position` | 侧栏视图（资源管理器/任务/审查/交接/配置/历史）定义为 QML 面板描述对象数组，`DockHost` 按描述渲染图标条与互斥切换；布局状态按工作区存 UI 本地 JSON | **首期**（面板跨 dock 拖移为**远期**） |
| 6 | **中心区递归分屏树 `Member/PaneAxis + flexes`**（2.1、2.2） | 编辑器组模型定义为树（首期恒为单 Pane），QML 嵌套 `SplitView` 渲染；标签页含预览标签（单击预览/双击固定）与每组导航历史 | 模型**首期**，实际分屏**远期** |
| 7 | **类型化会话条目 + 工具调用卡片**（4.2、4.4）：轨迹视图 = 类型化条目 ListModel，工具/操作请求渲染为可折叠卡片，待办卡片驻留高亮 | 轨迹事件（task.phase / 操作请求 / 验证状态）→ Python ListModel（role=条目类型），QML DelegateChooser 分派卡片；操作请求卡片只展示与引导原生通道，**不代理批准** | **首期** |
| 8 | **MultiBuffer 式审查聚合**（4.3、4.4）：单滚动流内"文件分节 + 带上下文行的变更摘录（context/primary 区分展示与高亮区间）" | 审查视图 QML 分节 ListView：节头=文件+统计，节体=只读 diff 摘录；保持只读，接受/拒绝仍是交付粒度 | **首期**（增强现有审查视图） |
| 9 | **CharBag 预过滤 + 可取消后台模糊匹配 + 统一 Picker**（5.2、5.3） | 快速打开/命令面板共用一个 QML `Picker` 组件 + Python delegate；打分在线程池，`int` 位图预过滤，每次击键置位取消令牌 | **首期**（13 号文档核心输入） |
| 10 | **懒扫描快照式文件树**（5.1、6）：目录懒加载、"未加载"入模型、UI 读快照不等 IO | 资源管理器 TreeModel 按需 `fs.list`，节点带 loading 态；Sidecar 变更事件驱动增量刷新；ignored 目录折叠不扫 | **首期**（全量后台索引与文件监视增强为**远期**） |

**明确不借鉴**：CRDT/协作与多副本时钟（单机产品无此复杂度）、GPUI/自绘 UI 框架、hunk 级 Keep/Reject 写回工作区（违背审查只读与交付粒度结论）、面板内嵌终端、扩展体系、会话运行状态自动恢复（与"中断如实标记"冲突）。
