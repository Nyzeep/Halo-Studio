# 13 - 命令面板与快速打开

**状态**：设计完成，待评审
**日期**：2026-07-27
**依据**：`requirements-alignment/03-ide-editor-and-reference-alignment.md`、`docs/design/references/R1-vscode-analysis.md`、`docs/design/references/R2-zed-analysis.md`
**跨模块约定**：本文档是 `CommandRegistry` 与 when 上下文的唯一定义者；QML 主题单例 `Theme`（token 由 10 号定义）、`EditorService`（11 号定义）、`fs.*` IPC（12 号定义）在此仅作消费。

---

## 1. 目标与范围

### 1.1 目标

1. 提供 Python 侧 **`CommandRegistry(QObject)`**：命令的注册、注销、执行、可用性（when）与列表模型暴露；命令面板、快捷键绑定（10 号挂载）、菜单可用性同源于这一份注册信息。
2. 提供纯 Python **模糊匹配器** `fuzzy_score(query, target) -> (score, matched_indices)`：借鉴 vscode fuzzyScorer 思想的简化版，无任何 Qt 依赖，可独立单测。
3. 提供 QML **命令面板覆盖层**（单浮层双模式）：
   - `Ctrl+Shift+P` —— 命令模式（查询前缀 `>`）；
   - `Ctrl+P` —— 快速打开文件模式（数据源：Sidecar `fs.list` 的本地缓存）。
   键盘导航、匹配字符高亮、最近使用置顶（QSettings 持久化）。
4. 提供 **when 上下文极简版**：`hasWorkspace` / `hasActiveEditor` / `taskRunning` 三个布尔键，表达式仅支持 `&&` 与 `!`。

### 1.2 范围外（明确不做）

- **多步 QuickInput 向导**（vscode 的多级 picker / step 流程）——一切命令要么直接执行，要么由目标视图自行弹出常规对话框。
- **`:` 跳转行 Provider** 与 `?` 帮助前缀——首期只有 `''`（文件）与 `>`（命令）两个模式。
- **内容搜索**——快速打开只索引文件名（相对路径字符串），全文搜索走 12 号 `fs.search` 的全局搜索视图。
- **TF-IDF 相似命令补充**（R1 §3.2 明确首期不做）。
- **树形作用域 when 上下文、`||`/括号/比较运算**（R1 §4.3 的完整 ContextKeyExpr 不做；三个平面布尔键 + `&&`/`!` 即止）。
- **命令叠注册**（vscode 同 id LinkedList 覆盖机制）——重复 id 直接拒绝。
- **和弦快捷键的解析与分发**——`shortcut` 字段只存单和弦序列字符串；绑定与分发由 10 号壳层负责。

---

## 2. 参考结论引用

| 来源 | 借鉴 | 不借鉴 |
| --- | --- | --- |
| R1 §3.1（CommandsRegistry / Action2） | 命令 + 标题/分类 + when + 默认快捷键**一次声明**，命令面板/键绑定/菜单共享同一注册信息，避免三处漂移 | 同 id LinkedList 叠注册与自动回退；metadata/args 描述体系 |
| R1 §3.2（QuickInput / QuickAccess） | **单浮层 + 前缀路由**：一个输入框控件，按 `''` / `>` 前缀切换 Provider；最近使用命令分组置顶并持久化；键入即取消上一轮异步计算 | 多步 picker、`:`/`?` 前缀、TF-IDF 相似命令、busy 期间的复杂进度语义 |
| R1 §3.3（fuzzyScorer） | 打分维度全套简化移植：连续匹配 6/3 衰减、首字符 +8、路径分隔符 +5、词分隔符 +4、camelCase +2、大小写一致 +1（不一致即相对惩罚）；按序匹配约束；空格多段查询；文件条目三层基准分与决胜链 | 引号精确段、通配符/省略号剥离、打分结果 hash 缓存（首期以取消 + 截断顶替）、完整决胜链的全部环节 |
| R1 §3.4（anythingQuickAccess） | 历史（已打开文件）优先合并排序；每次键入用取消令牌废弃上一轮 | 编辑器历史与全局搜索的复杂合并策略 |
| R1 §4.3（when 上下文） | "UI 状态抽象为可声明布尔键，命令可用性声明式引用"的思想内核 | ContextKeyExpr AST、序列化解析器、树形 scoped context |
| R2 §5.2（Zed fuzzy/CharBag） | **CharBag 位图预过滤**（Python int 位运算等价实现）；每次输入置位取消标志、旧任务尽快退出；命令面板与文件查找**共用同一 picker 组件 + 两个 delegate** | 按 CPU 分片并行打分（Python GIL 下收益有限，单工作线程 + 预过滤 + 截断已够）、worktree 分集 |
| R2 §5.3 | 快速打开候选 = Sidecar 提供的一次性全量相对路径清单，打分放 Python 工作线程 | UI 层自建文件监视（增量刷新走 12 号事件，首期 TTL 缓存即可） |

---

## 3. 与现有契约的关系（增量逐条）

### 3.1 对 `docs/ipc-protocol.md`

**本文档不新增任何 IPC 方法。** `fs.*` 命名空间由 12 号文档唯一定义并入契约。13 号在此登记对 12 号的**消费需求**（12 号设计时必须满足，最终形状以 12 号并入 ipc-protocol.md 的文本为准）：

1. 需要一次调用获得**受信任工作区全部文件的相对路径清单**（递归、遵循 ignore 规则、路径牢笼内、含条目数上限与 `truncated` 标记）——即 `fs.list` 的递归模式或等价方法；
2. 未信任工作区调用返回 `WORKSPACE_NOT_TRUSTED`（面板据此显示提示而非空列表）；
3. 返回路径为工作区相对路径；分隔符风格由 12 号定，本文档匹配器对 `/` 与 `\` 互认，不依赖具体风格。

### 3.2 对 `docs/module-contracts.md`

第 8 节（app/halo_studio）新增以下条目（评审通过后并入）：

```
halo_studio/commands/when_context.py — WhenContext(QObject)：三布尔键 + evaluate(expr)
halo_studio/commands/registry.py     — Command / CommandListModel / CommandRegistry(QObject)
halo_studio/commands/fuzzy.py        — 纯函数模糊匹配器（无 Qt 依赖）
halo_studio/commands/builtin.py      — register_builtin_commands(registry, actions)
halo_studio/viewmodels/file_index.py — FileIndex(QObject)：fs.list 缓存
halo_studio/viewmodels/palette_vm.py — PaletteViewModel / PaletteResultsModel
halo_studio/qml/palette/*.qml        — CommandPalette / PaletteItemDelegate / HighlightedText
```

文件所有权矩阵新增：`py-palette` → `app/halo_studio/commands/**`、`app/halo_studio/viewmodels/palette_vm.py`、`app/halo_studio/viewmodels/file_index.py`、`app/halo_studio/qml/palette/**`、`app/tests/test_fuzzy*.py`、`app/tests/test_command_registry*.py`、`app/tests/test_when_context*.py`、`app/tests/test_palette*.py`、`app/tests/test_file_index*.py`。

### 3.3 对其他设计文档的接口依赖（消费，不定义）

| 依赖 | 提供方 | 本文档消费的最小 API |
| --- | --- | --- |
| `Theme` 单例 | 10 号 | `quickInputBackground`、`inputBackground/inputForeground/inputBorder`、`listActiveSelectionBackground/Foreground`、`listHoverBackground`、`listHighlightForeground`、`pickerGroupForeground`、`descriptionForeground`、`focusBorder`、`widgetShadow`、字体/间距 token |
| 壳层挂载与快捷键绑定 | 10 号 | 10 号在 `Main.qml` 实例化 `CommandPalette`，并遍历 `CommandRegistry.commands` 模型为每条含 shortcut 的命令生成全局 `Shortcut { onActivated: registry.execute(id) }`；视图切换命令的回调绑定到 10 号布局 API（见 §4.5 `LayoutActions` 协议） |
| `EditorService` | 11 号 | `openFile(path, line=-1)`、`activeDocument`、打开文档列表模型（快速打开空查询时的"打开的标签"分组）、`save/saveAll/closeTab` 等由内置命令回调调用 |
| `fs.list` | 12 号 | 见 §3.1 |
| 差异化命令语义 | 15 号 | 本文档仅保留命令 id 挂点（§5），语义与实现由 15 号裁决 |

---

## 4. 详细设计

### 4.1 命令 id 命名规范

- 格式：`area.action`，正则 `^[a-z]+\.[a-zA-Z][a-zA-Z0-9]*$`（area 全小写单词，action 小驼峰）。
- area 固定清单（注册时校验，超出即 `ValueError`）：
  `app` / `view` / `palette` / `workspace` / `editor` / `task` / `review` / `handoff` / `config` / `history`。
- 示例：`editor.save`、`editor.closeAllTabs`、`task.create`、`view.explorer`。
- 15 号差异化命令沿用同一规范（如 `review.openInEditor`），不得另起前缀。

### 4.2 WhenContext（`app/halo_studio/commands/when_context.py`）

```python
class WhenContext(QObject):
    """极简 when 上下文：三个平面布尔键。10 号在组装期负责接线。"""

    changed = Signal()   # 任一键变化即发射（合并发射，供模型批量刷新 enabled）

    hasWorkspace   = Property(bool, ...)  # 存在活动工作区 且 trust == "trusted"
    hasActiveEditor = Property(bool, ...) # EditorService.activeDocument 非空
    taskRunning    = Property(bool, ...)  # 当前任务 state ∈ {created, running, awaiting_action, finishing}

    def set_key(self, key: str, value: bool) -> None: ...
    def evaluate(self, expr: str | None) -> bool: ...
```

**接线（组装期，`app.py`）**：
- `hasWorkspace` ← `WorkspaceViewModel` 的 `workspace.changed`（active 且 trust=="trusted"；`identity_changed` 降级即 False）；
- `hasActiveEditor` ← `EditorService` 活动文档变化信号；
- `taskRunning` ← `TaskViewModel` 的 `task.state` 事件。

**表达式文法**（仅此，无更多）：

```
expr := term ("&&" term)*
term := "!"? key
key  := "hasWorkspace" | "hasActiveEditor" | "taskRunning"
```

- `evaluate(None)` / `evaluate("")` → `True`（无前置条件）。
- 未知键 → 该 term 求值 `False` 并 `logger.warning`（一次性去重告警），不抛异常——保证坏表达式只导致命令不可用，不炸 UI。
- 不支持 `||`、括号、`==`；解析即按 `&&` 切分、strip、判前缀 `!`，实现 ≤ 30 行。

### 4.3 CommandRegistry（`app/halo_studio/commands/registry.py`）

```python
@dataclass(frozen=True)
class Command:
    id: str
    title: str            # 中文用户可读标题，不含分类前缀
    category: str         # 中文分类名（面板显示为 "分类: 标题"）
    callback: Callable[[], None]
    shortcut: str | None  # QKeySequence 可移植字符串，如 "Ctrl+Shift+P"；单和弦
    when: str | None      # WhenContext 表达式；None = 恒可用


class CommandListModel(QAbstractListModel):
    """按 (category, title) 排序的稳定列表模型；10 号快捷键绑定与命令面板共用。"""
    ROLES = {
        Qt.UserRole + 1: b"commandId",   # str
        Qt.UserRole + 2: b"title",       # str
        Qt.UserRole + 3: b"category",    # str
        Qt.UserRole + 4: b"shortcut",    # str（无则 ""）
        Qt.UserRole + 5: b"enabled",     # bool：WhenContext 实时求值
    }
    # WhenContext.changed → 对全部行发 dataChanged([EnabledRole])


class CommandRegistry(QObject):
    commandsChanged = Signal()
    commandExecuted = Signal(str)          # id；命令面板据此记录 MRU
    executeFailed   = Signal(str, str)     # id, 用户可读原因（未注册/前置不满足/回调异常）

    def __init__(self, when_context: WhenContext, parent=None): ...

    # ---- 契约 API（跨模块约定锁定的签名） ----
    def register(self, id: str, title: str, category: str,
                 callback: Callable[[], None],
                 shortcut: str | None = None,
                 when: str | None = None) -> bool: ...
    def unregister(self, id: str) -> bool: ...
    def execute(self, id: str) -> bool: ...

    commands = Property(QObject, ...)      # CommandListModel，QML 侧只读

    # ---- 辅助查询（Python 侧使用） ----
    def get(self, id: str) -> Command | None: ...
    def is_enabled(self, id: str) -> bool: ...   # when 求值
```

**语义约束**：

1. `register`：id 不合法（§4.1 正则）→ `ValueError`；id 重复 → 返回 `False` + 告警，**不覆盖**；shortcut 与已注册命令冲突 → 该命令按无 shortcut 注册并告警（先注册者保有快捷键）。成功后模型增量插入（`beginInsertRows`）。
2. `unregister`：不存在返回 `False`；存在则移除并更新模型。
3. `execute`：
   - id 未注册 → `executeFailed(id, "命令不存在")`，返回 `False`；
   - `when` 求值 `False` → `executeFailed(id, "当前状态下不可用")`，返回 `False`（快捷键与面板同受此闸）；
   - 回调抛异常 → 捕获、`logger.exception`、`executeFailed(id, str(e))`，返回 `False`——命令异常永不炸 UI 主循环；
   - 成功 → `commandExecuted(id)`，返回 `True`。
   - 首期不支持参数透传（内置命令一律读取当前上下文；需要参数的差异化命令由 15 号直接调用服务 API，不经 `execute`）。
4. 回调在**主线程**同步执行；耗时操作由回调自行转交 IPC 异步（现有 client Future 机制），注册表不管理并发。

### 4.4 内置命令初始清单（`app/halo_studio/commands/builtin.py`）

`register_builtin_commands(registry, actions)` 在组装期调用一次。28 条初始命令：

| # | id | title | category | shortcut | when |
| --- | --- | --- | --- | --- | --- |
| 1 | `palette.commands` | 显示所有命令 | 面板 | `Ctrl+Shift+P` | — |
| 2 | `palette.quickOpen` | 快速打开文件 | 面板 | `Ctrl+P` | `hasWorkspace` |
| 3 | `view.explorer` | 显示资源管理器 | 视图 | `Ctrl+Shift+E` | — |
| 4 | `view.tasks` | 显示任务视图 | 视图 | `Ctrl+Shift+A` | — |
| 5 | `view.review` | 显示审查视图 | 视图 | `Ctrl+Shift+R` | — |
| 6 | `view.handoff` | 显示交接视图 | 视图 | — | — |
| 7 | `view.config` | 显示配置视图 | 视图 | `Ctrl+,` | — |
| 8 | `view.history` | 显示历史视图 | 视图 | — | — |
| 9 | `view.toggleSidebar` | 切换侧栏 | 视图 | `Ctrl+B` | — |
| 10 | `view.toggleBottomPanel` | 切换底部面板 | 视图 | `Ctrl+J` | — |
| 11 | `editor.save` | 保存文件 | 编辑器 | `Ctrl+S` | `hasActiveEditor` |
| 12 | `editor.saveAll` | 保存全部文件 | 编辑器 | `Ctrl+Alt+S` | `hasActiveEditor` |
| 13 | `editor.closeTab` | 关闭当前标签 | 编辑器 | `Ctrl+W` | `hasActiveEditor` |
| 14 | `editor.closeAllTabs` | 关闭全部标签 | 编辑器 | — | `hasActiveEditor` |
| 15 | `editor.nextTab` | 下一个标签 | 编辑器 | `Ctrl+Tab` | `hasActiveEditor` |
| 16 | `editor.previousTab` | 上一个标签 | 编辑器 | `Ctrl+Shift+Tab` | `hasActiveEditor` |
| 17 | `editor.find` | 在文件中查找 | 编辑器 | `Ctrl+F` | `hasActiveEditor` |
| 18 | `workspace.open` | 打开工作区… | 工作区 | — | — |
| 19 | `workspace.trust` | 信任当前工作区 | 工作区 | — | — |
| 20 | `workspace.close` | 关闭工作区 | 工作区 | — | — |
| 21 | `task.create` | 新建 Agent 任务 | 任务 | `Ctrl+Shift+N` | `hasWorkspace && !taskRunning` |
| 22 | `task.cancel` | 取消当前任务 | 任务 | — | `taskRunning` |
| 23 | `task.markVerificationNotRun` | 标记验证未执行 | 任务 | — | `hasWorkspace` |
| 24 | `review.openLatest` | 打开最新交付审查 | 审查 | — | `hasWorkspace` |
| 25 | `review.acceptDelivery` | 接受当前交付 | 审查 | — | `hasWorkspace && !taskRunning` |
| 26 | `review.rejectDelivery` | 拒绝当前交付 | 审查 | — | `hasWorkspace && !taskRunning` |
| 27 | `handoff.preview` | 预览交接包 | 交接 | — | `hasWorkspace && !taskRunning` |
| 28 | `handoff.create` | 创建交接… | 交接 | — | `hasWorkspace && !taskRunning` |

说明：

- **when 只做 UI 层可用性**；真正的业务门禁仍在 Sidecar（如 `EVIDENCE_NOT_LATEST`、`WORKSPACE_NOT_TRUSTED`、`TASK_NOT_REVIEWABLE`），回调必须原样呈现 Sidecar 错误，不做旁路判断。`workspace.trust/close/open` 故意不设 when（未信任工作区也要能执行 trust）。
- `review.accept/rejectDelivery`、`task.markVerificationNotRun` 打开/聚焦对应视图并触发其既有确认流程，不绕过现有 ViewModel。
- 快捷键均为单和弦；`Ctrl+Tab`/`Ctrl+Shift+Tab` 由 10 号绑定在窗口层（需在编辑器获焦时仍生效，见 §8 风险 4）。

**LayoutActions 协议**（视图切换命令与 10 号的解耦挂点；10 号布局 API 的最终命名以 10 号为准，此处为 13 号消费面）：

```python
class LayoutActions(Protocol):
    def show_view(self, view_id: str) -> None: ...     # "explorer"|"tasks"|"review"|"handoff"|"config"|"history"
    def toggle_sidebar(self) -> None: ...
    def toggle_bottom_panel(self) -> None: ...

class PaletteActions(Protocol):
    def open_palette(self, prefill: str) -> None: ...  # palette.commands → ">"；palette.quickOpen → ""

@dataclass
class BuiltinCommandActions:
    layout: LayoutActions
    palette: PaletteActions
    editor: "EditorService"          # 11 号
    workspace: "WorkspaceViewModel"
    task: "TaskViewModel"
    review: "ReviewViewModel"
    handoff: "HandoffViewModel"

def register_builtin_commands(registry: CommandRegistry, actions: BuiltinCommandActions) -> None: ...
```

### 4.5 模糊匹配器（`app/halo_studio/commands/fuzzy.py`，纯函数，无 Qt）

#### 4.5.1 核心函数

```python
def fuzzy_score(query: str, target: str) -> tuple[int, list[int]]:
    """单段模糊打分。query 全部字符必须按序命中 target，否则 (0, [])。
    空 query 返回 (0, [])——调用方自行决定"空查询=不过滤"。
    matched_indices 为 target 中命中位置，严格递增。"""

def fuzzy_match(query: str, target: str) -> tuple[int, list[int]]:
    """多段包装：query 按空白切段，每段独立 fuzzy_score 且都必须命中；
    总分 = 各段分数之和；indices = 各段并集去重升序。任一段不命中 → (0, [])。"""

def char_bag(text: str) -> int:
    """位图预过滤签名：a-z 26 位（忽略大小写）+ 0-9 共用 1 位 + '-' 1 位 + '_' 1 位 + '.' 1 位。
    其他字符（含 CJK、路径分隔符）不入位图。"""

def bag_is_subset(query_bag: int, target_bag: int) -> bool:
    """query_bag & ~target_bag == 0。只用于淘汰，绝不产生假阴性：
    位图未覆盖的字符不参与置位，因此 CJK 查询的 bag 为 0，恒通过预过滤。"""
```

#### 4.5.2 打分规则表（锁定，测试以此为准）

字符比较先经归一化：小写化；`/` 与 `\` 归一为同一字符（互认，且互认视同大小写一致）。

| 维度 | 加分 | 说明 |
| --- | --- | --- |
| 字符命中（忽略大小写） | +1 | 不命中该格记 0 分 |
| 大小写完全一致 | +1 | 不一致即相对惩罚 1 分 |
| 连续匹配 | 连续段第 1–3 个后继字符每个 +6，第 4 个起每个 +3 | 段首字符不算后继 |
| target 首字符（索引 0）命中 | +8 | 与下两行互斥（按位置天然互斥） |
| 前一字符是路径分隔符 `/` `\` | +5 | |
| 前一字符是词分隔符 `_` `-` `.` 空格 `'` `"` `:` | +4 | |
| camelCase 词内边界（target 当前字符大写且前一字符小写） | +2 | 仅在**非连续**匹配时生效 |

**按序约束**：非首个 query 字符只有在前一 query 字符已在更早位置命中（DP 对角线 > 0）时才允许得分——杜绝乱序高分。DP 每格取 `max(左值, 对角线+字符分)`，相等时取匹配分支（倾向更早、更连续的匹配）；矩阵同步记录连续长度；回溯自右下角还原 `matched_indices`。复杂度 O(|query|·|target|)，实现约 120 行。

#### 4.5.3 `fuzzy_score` 单测用例表（`app/tests/test_fuzzy.py`，pytest parametrize 直录）

| # | query | target | 期望 score | 期望 indices | 覆盖点 |
| --- | --- | --- | --- | --- | --- |
| 1 | `""` | `abc` | 0 | `[]` | 空查询 |
| 2 | `x` | `abc` | 0 | `[]` | 无命中 |
| 3 | `a` | `abc` | 10 | `[0]` | 1+1+首字符 8 |
| 4 | `ab` | `abc` | 18 | `[0,1]` | 连续 +6 |
| 5 | `ac` | `abc` | 12 | `[0,2]` | 非连续无加权 |
| 6 | `A` | `abc` | 9 | `[0]` | 大小写惩罚（对比 #3） |
| 7 | `abcde` | `abcde` | 39 | `[0..4]` | 连续衰减：10+8+8+8+5 |
| 8 | `s` | `task_spec.py` | 6 | `[5]` | 词分隔符 +4 胜过词内 `s`(2 分) |
| 9 | `m` | `src/main.py` | 7 | `[4]` | 路径分隔符 +5 |
| 10 | `cr` | `CommandRegistry` | 12 | `[0,7]` | camelCase +2（`C` 处 1+0+8，`R` 处 1+0+2，均无大小写分） |
| 11 | `ba` | `abc` | 0 | `[]` | 按序约束拒绝乱序 |
| 12 | `edi` | `editor.py` | 26 | `[0,1,2]` | 10+8+8 |
| 13 | `ep` | `editor.py` | 16 | `[0,7]` | 首字符 + 词分隔符组合 |
| 14 | `src/m` | `src\main.py` | 44 | `[0,1,2,3,4]` | 分隔符互认：10+8+8+8+10 |
| 15 | `main py` | `src\main.py` | 45 | `[4,5,6,7,9,10]` | 多段（`fuzzy_match`）：31+14 |

另有非表格断言：所有返回 indices 严格递增；`score>0 ⇔ indices 长度 == len(query 去空白段总长)`；`char_bag`/`bag_is_subset` 无假阴性（随机串性质测试）；CJK 查询 bag 为 0 恒通过预过滤但仍可被 `fuzzy_score` 正确命中（如 `保存` vs `编辑器: 保存文件`）。

#### 4.5.4 条目级打分（面板排序用）

```python
@dataclass(frozen=True)
class ScoredItem:
    score: int                 # 含层级基准分
    matched_indices: list[int] # 相对被匹配串（文件=basename 或全路径；命令=展示串或 id）
    matched_on: str            # "basename" | "path" | "label" | "id"

def score_file_candidate(query: str, rel_path: str) -> ScoredItem | None: ...
def score_command_candidate(query: str, title_with_category: str, command_id: str) -> ScoredItem | None: ...
```

**文件三层基准分**（借 R1 §3.3C 简化）：

| 层 | 条件 | 分值 |
| --- | --- | --- |
| T0 | basename 完全等于查询（忽略大小写） | `1<<18` |
| T1 | basename 前缀匹配（忽略大小写） | `1<<17 + round(len(query)/len(basename)*100)`（短名奖励） |
| T2 | basename 上 `fuzzy_match` 命中 | `1<<16 + score` |
| T3 | 查询含 `/` 或 `\` 时，仅对全相对路径 `fuzzy_match` | `score`（无基准分；T0–T2 此时跳过） |

**命令打分**：对展示串 `f"{category}: {title}"` 与 `id` 分别 `fuzzy_match`，取高分者（`matched_on` 相应标记；命中 id 时高亮落在 description 行）。无基准分分层。

**决胜链**（比较器，序依次）：score 降序 → 最近使用/最近打开者优先（MRU 序）→ basename（或 title）更短 → 全路径（或 id）更短 → 字典序。实现为预计算排序键元组 + `heapq.nlargest(100, …)`，只保留前 100 条进模型。

### 4.6 文件索引缓存（`app/halo_studio/viewmodels/file_index.py`）

```python
class FileIndex(QObject):
    """fs.list 递归清单的进程内缓存。唯一数据源是 Sidecar（12 号契约），绝不直接扫盘。"""
    refreshed = Signal()
    failed = Signal(str)           # 用户可读原因（含 WORKSPACE_NOT_TRUSTED 文案）

    def __init__(self, client: IpcClient, when_context: WhenContext, parent=None): ...

    def snapshot(self) -> tuple[int, list[str]]: ...
        # (generation, 相对路径列表)。列表冻结不可变；每行附带的 char_bag 与 basename 在 refresh 完成时一次性预计算
    def ensure_fresh(self, ttl_seconds: int = 30) -> None: ...
        # hasWorkspace 为 False → 直接 failed("工作区未信任，文件索引不可用")，不发 IPC；
        # 缓存未过期 → no-op；否则发起 fs.list（异步 Future），完成后 generation += 1 并 refreshed
    def invalidate(self) -> None: ...   # workspace.changed / 任务终态事件时由组装层调用
    truncated = Property(bool, ...)     # fs.list 结果带 truncated 时为 True，面板显示提示行
```

预计算结构：`list[tuple[rel_path, basename, bag_path, bag_basename]]`——匹配线程零解析开销。

### 4.7 PaletteViewModel 与结果模型（`app/halo_studio/viewmodels/palette_vm.py`）

```python
class PaletteResultsModel(QAbstractListModel):
    ROLES = {
        ...: b"label",           # str：命令为 "分类: 标题"；文件为 basename
        ...: b"description",     # str：命令为快捷键串；文件为所在目录相对路径；id 命中时为命令 id
        ...: b"matchedIndices",  # list[int]（QVariantList），相对 label（matched_on 为 id/path 时相对 description）
        ...: b"matchedOn",       # str："label"|"id"|"basename"|"path"
        ...: b"group",           # str：分组标题（QML section 用），可为 ""
        ...: b"itemKind",        # "command" | "file"
        ...: b"itemId",          # 命令 id 或文件相对路径
    }

class PaletteViewModel(QObject):
    visibleChanged = Signal(); busyChanged = Signal(); selectedIndexChanged = Signal()
    hintChanged = Signal()     # 空态/错误/截断提示文案

    visible  = Property(bool, ...)
    busy     = Property(bool, ...)
    query    = Property(str, ...)          # 双向：QML TextField ↔ VM
    hint     = Property(str, ...)          # ""|"无匹配结果"|"工作区未信任…"|"文件清单已截断…"
    selectedIndex = Property(int, ...)
    results  = Property(QObject, ...)      # PaletteResultsModel

    @Slot(str)
    def open(self, prefill: str) -> None: ...   # ">" = 命令模式；"" = 文件模式；记录先前焦点由 QML 侧负责
    @Slot()
    def close(self) -> None: ...
    @Slot(int)
    def moveSelection(self, delta: int) -> None: ...  # ±1 行、±10 页；循环滚动
    @Slot()
    def acceptSelected(self) -> None: ...
```

**模式判定**：`query.startswith(">")` → 命令模式（有效查询 = `query[1:].strip()`）；否则文件模式。`palette.commands` 即 `open(">")`，`palette.quickOpen` 即 `open("")`——单浮层前缀路由（R1 §3.2），用户删掉 `>` 可原地切到文件模式。

**accept 语义**：
- 命令：先 `close()`（归还焦点），再 `registry.execute(id)`；`commandExecuted` 时把 id 写入 MRU；
- 文件：`EditorService.openFile(path)`（path 形参语义以 11 号为准：若 11 号要求绝对路径，此处用 `WorkspaceStatus.real_path` 拼接），成功后 `close()` 并写文件 MRU。

**最近使用（QSettings，组织/应用名沿用现有 app 设置）**：
- `palette/recentCommands`：`QStringList`，头插去重，截断 20；
- `palette/recentFiles`：`QStringList`（相对路径），头插去重，截断 30；工作区切换时不清空，但展示前按当前索引快照过滤已不存在的路径。

**分组与排序**：

| 模式 | 空查询 | 非空查询 |
| --- | --- | --- |
| 命令 | 组"最近使用"（MRU 序，命中 when 的才显示）→ 组"全部命令"（(category,title) 排序） | 无分组；按 §4.5.4 打分 + 决胜链；`when=False` 的命令**不出现**（与 vscode 一致，且 execute 双重拦截） |
| 文件 | 组"打开的标签"（EditorService 打开文档模型序）→ 组"最近打开"（文件 MRU）| 无分组；按 §4.5.4 打分 + 决胜链，前 100 条 |

**线程与取消（数据流锁定）**：

```
键入 → setQuery → 30ms 防抖定时器（restart）
  → 到期：_generation += 1；旧 job.cancel_event.set()
  → _MatchJob(QRunnable) 提交到专用 QThreadPool(maxThreadCount=1)
       job 持有：generation、有效查询、候选快照（FileIndex.snapshot() 或 registry 命令快照）、cancel_event
       计算：CharBag 预过滤 → fuzzy 打分 → heapq.nlargest(100)；每 512 条检查 cancel_event，置位即 return
  → 完成信号 resultsReady(generation, rows)（跨线程 QueuedConnection 回主线程）
  → VM 校验 generation == self._generation，否则丢弃；命中则 model.beginResetModel 替换
```

- 命令模式候选 ≤ 数十条，走同一管线（代码路径统一），实际耗时可忽略；
- 文件模式打开时调 `FileIndex.ensure_fresh()`：陈旧快照**立即可搜**（stale-while-revalidate），`refreshed` 到达后自动对当前查询重跑一轮；
- `busy = FileIndex 请求在途 or 匹配 job 在途`。

### 4.8 QML 组件树（`app/halo_studio/qml/palette/`）

```
CommandPalette.qml            — Popup（parent: Overlay.overlay; modal: false; dim: false;
│                                closePolicy: Escape | ClickOutside; 打开时记录 window.activeFocusItem，关闭时恢复焦点）
│   位置/尺寸：x 水平居中；y = 窗口高 8%；width = min(600, 窗口宽 60%)
│   background: Rectangle { color: Theme.quickInputBackground; radius: 6;
│                            border.color: Theme.focusBorder; layer 阴影用 Theme.widgetShadow }
├─ ColumnLayout (spacing: Theme 间距 token)
│  ├─ TextField  queryField
│  │     placeholderText: "搜索文件（输入 > 进入命令模式）"
│  │     color/background: Theme.inputForeground / Theme.inputBackground / Theme.inputBorder
│  │     text ↔ paletteVm.query（双向）
│  │     Keys.onPressed: Up/Down → moveSelection(±1)；PageUp/PageDown → moveSelection(±10)；
│  │                     Return/Enter → acceptSelected()；Escape → close()（交给 closePolicy 亦可）
│  ├─ BusyIndicator { visible: paletteVm.busy; 高度 2px 线性样式，不抖动布局 }
│  ├─ ListView    resultsList
│  │     model: paletteVm.results; clip: true; height: min(contentHeight, 12 行)
│  │     currentIndex: paletteVm.selectedIndex; highlightMoveDuration: 0
│  │     keyNavigationEnabled: false     // 键盘统一由 queryField 处理，焦点永不离开输入框
│  │     section.property: "group"; section.delegate: Label { color: Theme.pickerGroupForeground }
│  │     delegate: PaletteItemDelegate { onClicked: paletteVm.selectedIndex = index, paletteVm.acceptSelected() }
│  └─ Label      hintLabel { visible: paletteVm.hint !== ""; text: paletteVm.hint;
│                             color: Theme.descriptionForeground }
├─ PaletteItemDelegate.qml    — ItemDelegate（高 28px；hover: Theme.listHoverBackground；
│   │                            current: Theme.listActiveSelectionBackground/Foreground）
│   ├─ HighlightedText label  { source: model.label; indices: matchedOn ∈ {"label","basename"} ? model.matchedIndices : [] }
│   └─ Label description      { text: model.description; color: Theme.descriptionForeground; elide: ElideMiddle }
│         // matchedOn ∈ {"id","path"} 时 description 也经 HighlightedText 渲染高亮
└─ HighlightedText.qml        — Text { textFormat: Text.StyledText }
      // JS 函数：HTML 转义原文，按 indices 把命中字符包 <b><font color=Theme.listHighlightForeground>；
      // 每行一次性构建，无逐字符 Item
```

**挂载**（10 号执行）：`Main.qml` 中 `CommandPalette { }` 单例实例；上下文属性 `paletteVm`、`commandRegistry` 由 `app.py` 组装注入；全局 Shortcut 生成器遍历 `commandRegistry.commands`。

---

## 5. 差异化点（仅挂点，裁决权在 15 号）

1. **保留命令 id**（15 号实现时按 §4.1 规范注册进同一 registry，无需改本模块）：
   `task.addFileToContext`、`task.addSelectionToContext`（任务上下文选择器）、`review.openInEditor`（审查→编辑器跳转）。
2. 命令面板对差异化功能零特殊逻辑：15 号命令注册后自动出现在面板并参与打分/MRU。
3. 快速打开的文件行**不承载**基线徽章/归因标记（那是 12 号资源管理器与 11 号标签页的装饰通道职责）；若 15 号裁决需要，扩展点是 `PaletteResultsModel` 新增角色 + delegate 追加徽章，不动匹配管线。

---

## 6. 实施计划

### 6.1 文件清单

**新建**：

| 文件 | 内容 | 规模估计 |
| --- | --- | --- |
| `app/halo_studio/commands/__init__.py` | 包导出 | ~10 行 |
| `app/halo_studio/commands/fuzzy.py` | §4.5 全部纯函数 | ~350 行 |
| `app/halo_studio/commands/when_context.py` | WhenContext | ~90 行 |
| `app/halo_studio/commands/registry.py` | Command/CommandListModel/CommandRegistry | ~280 行 |
| `app/halo_studio/commands/builtin.py` | 28 条内置命令 + Actions 协议 | ~220 行 |
| `app/halo_studio/viewmodels/file_index.py` | FileIndex | ~160 行 |
| `app/halo_studio/viewmodels/palette_vm.py` | PaletteViewModel/PaletteResultsModel/_MatchJob | ~420 行 |
| `app/halo_studio/qml/palette/CommandPalette.qml` | 浮层 | ~200 行 |
| `app/halo_studio/qml/palette/PaletteItemDelegate.qml` | 行委托 | ~90 行 |
| `app/halo_studio/qml/palette/HighlightedText.qml` | 高亮文本 | ~60 行 |
| `app/tests/test_fuzzy.py` | §4.5.3 表 + 性质测试 | ~250 行 |
| `app/tests/test_when_context.py` | 文法真值表 | ~80 行 |
| `app/tests/test_command_registry.py` | 注册/执行/模型 | ~220 行 |
| `app/tests/test_file_index.py` | 经 fake_sidecar 的缓存行为 | ~150 行 |
| `app/tests/test_palette_vm.py` | 端到端 VM 行为 | ~280 行 |

**修改**：

| 文件 | 修改 |
| --- | --- |
| `app/halo_studio/app.py` | 组装：WhenContext 接线、CommandRegistry、FileIndex、PaletteViewModel 创建与上下文属性注入、`register_builtin_commands` |
| `app/halo_studio/qml/Main.qml` | 由 10 号壳层重构统一承接：实例化 CommandPalette、生成全局 Shortcut（13 号只提供 §4.8 挂载说明） |
| `app/tests/fake_sidecar.py` | 若 12 号尚未为其添加 `fs.list` 脚本化响应，则由 12 号补齐；13 号测试仅消费 |
| `docs/module-contracts.md` / `docs/design/README.md` | 评审通过后按 §3.2 并入、状态更新（集成阶段执行） |

### 6.2 依赖顺序

1. `fuzzy.py` + `when_context.py` + `registry.py`（零外部依赖，**可立即并行开工**，含全部单测）；
2. `builtin.py`（依赖 10/11 号 API 形状——先以 `Protocol` 桩开发，组装期接真实对象）；
3. `file_index.py`（依赖 12 号 `fs.list` 契约文本冻结 + fake_sidecar 支持）；
4. `palette_vm.py`（依赖 1、3 与 11 号 `EditorService.openFile`）;
5. QML 三件（依赖 10 号 `Theme` token 全集与壳层挂载点）。

与其他模块的关系：10 号（挂载/快捷键/Theme）、11 号（EditorService）、12 号（fs.list）先于本模块集成联调；15 号后于本模块。

---

## 7. 测试计划

| 层 | 文件 | 断言要点 |
| --- | --- | --- |
| 单元（纯 Python，无 Qt） | `test_fuzzy.py` | §4.5.3 全表逐行；indices 严格递增；按序约束；多段并集；分隔符互认；CharBag 无假阴性（随机性质测试 1000 例：凡 `fuzzy_score>0` 必 `bag_is_subset`）；条目三层基准分（T0>T1>T2 序不可逆）；决胜链（同分短名先、MRU 先）；`heapq` 截断 100 |
| 单元 | `test_when_context.py` | 三键 × `&&`/`!` 真值表全覆盖；None/空串恒真；未知键为 False 且仅告警一次；坏表达式不抛异常 |
| 单元（pytest-qt） | `test_command_registry.py` | id 正则拒绝表（`Editor.save`、`editor.`、`x.y.z`…）；重复 id 拒绝；shortcut 冲突降级告警；execute 三种失败路径各自发 `executeFailed` 且返回 False；回调异常不逸出；when 变化 → 模型 `dataChanged(EnabledRole)`；模型角色与排序稳定 |
| 集成（pytest-qt + fake_sidecar） | `test_file_index.py` | TTL 内不重复请求；`invalidate` 后重新拉取；未信任 → 不发 IPC 且 `failed` 文案正确；truncated 透传 |
| 集成（pytest-qt） | `test_palette_vm.py` | `>` 前缀模式切换；空查询分组（最近使用/全部命令、打开的标签/最近打开）；键入 → 结果按分排序且高亮索引正确；**generation 竞态**：先慢后快两轮查询，慢者结果到达被丢弃；accept 命令 → registry.execute 被调 + MRU 头插去重截断（QSettings 用 tmp 配置隔离）；accept 文件 → EditorService.openFile 收到正确 path；when=False 命令不出现在结果；Escape/close 恢复选中态 |
| UI 冒烟 | `test_smoke*.py` 扩展 | `--smoke` 加载含 CommandPalette 的 Main.qml 根对象成功（10 号壳层落地后）；qmllint 通过 |
| 性能护栏（单元，非基准） | `test_fuzzy.py` 内 | 构造 20k 条合成路径，单轮匹配（含预过滤+截断）< 500ms（CI 宽松阈值，防回归量级劣化） |

---

## 8. 风险与缓解

| # | 风险 | 缓解 |
| --- | --- | --- |
| 1 | 大仓库（数万文件）纯 Python 打分卡输入 | 四重防线：CharBag 位过滤（淘汰绝大多数候选，单条一次位与）、30ms 防抖、每 512 条检查取消令牌、top-100 截断；仍不足时的后备（不进首期）：按 basename 首字符分桶索引 |
| 2 | `fs.list` 契约由 12 号定义，可能与本文档登记的消费需求（§3.1）不合（如只有按目录惰性列举） | 需求已逐条登记；若 12 号最终无递归模式，改由 FileIndex 以 BFS 逐目录聚合（接口不变，仅 `ensure_fresh` 内部实现变化），面板行为不受影响 |
| 3 | Qt `Popup` 焦点管理：关闭后焦点丢失、输入法（CJK）在 TextField 中预编辑串触发防抖 | 打开时记录 `activeFocusItem` 关闭时恢复（§4.8 已内建）；`setQuery` 对相同文本 no-op；输入法预编辑不提交不触发 `textChanged`（Qt 默认行为），测试覆盖 CJK 查询 |
| 4 | `Ctrl+Tab`/`Ctrl+W` 等快捷键被编辑器控件（TextArea）默认行为吞掉 | 快捷键绑定归 10 号：全局 `Shortcut` 需用 `Qt.ApplicationShortcut` 上下文；13 号在 registry 层提供 `enabled`（when）联动，冲突排查入 10 号联调清单 |
| 5 | when 求值与真实业务门禁漂移（UI 显示可用但 Sidecar 拒绝） | 设计上接受：when 只是 UI 便利，`execute` 回调必须原样呈现 Sidecar 错误码文案（§4.4 说明 1）；测试断言拒绝路径可视 |
| 6 | MRU 持久化把已删除文件/已注销命令顶在最前 | 展示前按当前索引快照/注册表过滤失效项（§4.7）；过滤不回写，避免抖动 |
| 7 | 打分算法与 R1 描述的 vscode 原版有意偏差（无引号段、无结果缓存等）导致排序体验差 | 偏差已在 §1.2/§2 显式记录为范围外；打分规则表 + 用例表锁定行为，后续调参只改常数不改结构，回归测试兜底 |

---

## 修订记录

- 2026-07-27：首版（对齐记录 03 触发；R1/R2 分析报告为输入）。
