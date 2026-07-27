# 11 - 编辑器内核（Editor Core）

**状态**：设计完成，待评审
**日期**：2026-07-27
**依据**：`requirements-alignment/03-ide-editor-and-reference-alignment.md`、`docs/design/references/R1-vscode-analysis.md`、`docs/design/references/R2-zed-analysis.md`
**上下游**：文件 IO 契约由 12 号（`fs.*`）唯一定义，本文只列消费需求；壳层挂载与 Theme 由 10 号定义；命令注册表与快捷键分发由 13 号定义；差异化功能裁决权在 15 号，本文只留挂点。

---

## 1. 目标与范围

### 1.1 目标

为 Halo Studio 提供 QML 原生的多标签文本编辑器内核：

1. **文档模型**：`EditorDocument`（路径、内容、脏状态、行尾风格保留、编码 fallback、磁盘版本冲突检测）与 `EditorService`（打开/保存/关闭编排、打开文档列表模型 = 标签页数据源、每文档独立撤销栈）。
2. **编辑器视图**：`EditorArea`（标签条 + 编辑器栈）与 `EditorPane`（`TextArea` + `QQuickTextDocument`），含行号栏、当前行高亮、光标位置上报状态栏。
3. **语法高亮**：Python 侧 `QSyntaxHighlighter` + Pygments，按扩展名选 lexer、token→Theme 颜色映射、大文件降级。
4. **查找/替换**（当前文件内，正则可选）与**跳转行**（Ctrl+G）。
5. **快捷键**经 `CommandRegistry` 注册（Ctrl+S/W/F/H/G/Tab 等）。
6. **任务感知**：保存动作本身不做归因判断（归因钩子在 Sidecar `fs.write`，属 15 号）；编辑器接收 `task.manual_edit` 事件后在标签上显示提示徽章。

### 1.2 范围外（明确不做，列入远期）

- LSP / 补全 / 诊断 / 悬停提示；
- 多光标、代码折叠、minimap、编辑器分屏（模型保留组抽象口，见 §4.4）、软折行（首期 `NoWrap` + 水平滚动）；
- 自动保存（**有意不做**：显式保存是"人工介入自动归因"的语义锚点，自动保存会污染归因，见 §4.9）；
- 文件监视驱动的实时外部变更提示（冲突检测只发生在保存时 + 窗口重获焦点时的轻量 stat，见 §4.3.5）；
- 全局搜索的实现（属 12 号 `fs.search` 消费，本文只留命令入口，见 §4.8）；
- 交付审查视图的任何改动：审查保持只读（03 号边界），本文的编辑器是独立的人工编辑面。

---

## 2. 参考结论引用

| 参考结论 | 借鉴什么 | 不借鉴什么 |
| --- | --- | --- |
| R1 §2.1 EditorInput/EditorPane/EditorGroup 三层抽象 | 文档身份（path 归一化后作 `matches()` 去重依据）与渲染部件分离；Pane 一次创建、切换标签不重建 | EditorInput 序列化恢复体系、capabilities 位掩码全集（首期只要 readOnly 一个能力位） |
| R1 §2.2 EditorGroupModel（顺序表 + MRU + 单 preview + pinned） | 纯 Python 无 DOM 状态机 `OpenDocumentsModel`：顺序表 + MRU（Ctrl+Tab 与关闭继任者选取）+ 单 preview 语义（可裁剪，见 §4.4.4）；QML `TabBar` 只做渲染 | sticky 固定区、编辑器分屏组网格、组间移动 |
| R1 §2.3 三态关闭确认 + veto | 关闭是可否决异步流程：保存/不保存/取消；保存失败回到确认而非静默关闭；脏判断实时读 `isModified()` | 同资源多组副本的最后副本判定（首期无分屏，无此问题） |
| R2 §1.2(a)/§1.3 状态与渲染解耦、快照单向数据流 | 编辑器状态归 Python（`EditorService`/`EditorDocument`），QML 只读属性/模型、单向提交动作；高亮、gutter 装饰、查找覆盖层各自独立失效 | rope/SumTree/CRDT 自研文本存储（`QTextDocument` 首期够用） |
| R2 §1.2(c)/§1.3 锚定而非行号 | gutter 装饰内部存 `QTextCursor` 锚点，编辑后位置自动维持；行号仅在写入/读出瞬间与锚点互转 | Anchor=插入时间戳的 CRDT 锚点实现 |
| R2 §6 大文件与性能策略 | 高亮永不阻塞输入（后台线程 lex + 代际取消 + 小文件同步快路径）；单行超长跳过高亮；大小分级降级 | Tree-sitter、显示变换管线（Inlay/Fold/Wrap/BlockMap） |
| R1 §6.2 装饰通道 | 标签徽章（脏点、人工编辑、基线变更）为正交通道，互不复用同一 UI 元素 | Decorations 多来源 weight 合并框架（首期各徽章独立布尔/角色即可） |

---

## 3. 与现有契约的关系（契约增量）

### 3.1 对 `docs/ipc-protocol.md`

**本文档不新增任何 IPC 方法与事件。** 编辑器全部文件 IO 经 12 号定义的 `fs.*`。以下是 11 号对 12 号契约的**消费需求清单**（字段与方法命名以 12 号并入 `ipc-protocol.md` 的最终形状为准；若有出入，由 §4.2 的 `FsIo` 适配层单点吸收）：

| 需求 | 说明 |
| --- | --- |
| `fs.read` 分块读取 | **硬性**：协议单行上限 1 MiB（含 base64 膨胀后实际约 700 KiB 内容/行），而编辑器需打开至 8 MiB 文件 → `fs.read` 必须支持 `offset/max_bytes` 续读或分块响应，`FsIo` 负责拼装 |
| `fs.read` 元数据 | 需要：文件总 `size`、全文件 `sha256`、`mtime`、`binary`（二进制检测结论）；内容建议以 base64 字节返回（编码/EOL 探测在 UI 侧做，策略见 §4.3.2/§4.3.3；若 12 号决定 Sidecar 侧解码，则需回传 `encoding/eol/has_bom`，`FsIo` 直通） |
| `fs.stat` | `{path}` → 至少 `{exists, size, mtime, sha256}`；`sha256` 是保存冲突检测的依据（≤8 MiB 文件哈希成本可接受；若 12 号拆为独立 `fs.hash`，`FsIo` 适配） |
| `fs.write` 分块 + 条件写 | 同样受 1 MiB 行限，需分块提交；**强烈要求**可选参数 `expected_sha256`：磁盘现值不匹配时拒绝写入并返回冲突错误（把"stat-再-write"的竞态窗口收敛到 Sidecar 原子判定）；成功返回新 `{sha256, mtime}` |
| 错误码 | 需要可区分：文件超上限 / 路径出牢笼 / 未信任工作区（复用 `WORKSPACE_NOT_TRUSTED`）/ 写冲突 / IO 失败；具体错误码名由 12 号定义，编辑器按 code 分支提示 |
| 归因钩子位置 | `fs.write` 是 15 号"人工介入自动归因"的钩子所在（Sidecar 侧判定运行中任务并 `task.mark_manual_edit`）；**编辑器保存路径不携带、不判断任何归因信息** |

### 3.2 对 `docs/module-contracts.md`

- §8 `app/halo_studio` 新增 `editor` 包（文件清单见 §6），公共 API = 本文 §4.4 的 `EditorService`；
- 文件所有权矩阵新增一行：`py-editor` → `app/halo_studio/editor/**`、`app/halo_studio/qml/editor/**`、`app/tests/test_editor_*.py`；
- **安全边界纪律入契约**：`app/halo_studio/editor/**` 与 `app/halo_studio/qml/editor/**` 禁止出现 `QFile`、`open()`、`os.replace` 等本地文件 IO——工作区读写只经 `FsIo`→`fs.*`（路径牢笼、信任门禁、归因钩子都在 Sidecar）。测试计划含静态扫描断言（§7.1）；
- `app.py` 装配：新增上下文属性 `editorService`（工厂函数 `halo_studio.editor.create_editor_service(client)`，client 为 `viewmodels/base.py` 约定的鸭子类型）；挂载与装配顺序由 10 号统筹。

### 3.3 对 10 号（壳层与 Theme）的需求

- `Theme` 单例（`qml/theme/Theme.qml`）除 R1 §5.3 令牌外，需含**语法色组**（本文 §4.6.3 列出 12 个 `syntax*` 令牌名）与编辑器令牌（`editorBackground/editorForeground/editorLineHighlight/editorLineNumberForeground/editorActiveLineNumberForeground/editorSelectionBackground/editorFindMatchBackground/editorFindMatchHighlightBackground/editorCursorForeground`、`tabDirtyForeground`）；
- **ThemeBridge**：Python 侧高亮器需要读取 Theme 颜色 → 10 号需提供 Python 可读的主题数据源（建议：主题定义为数据文件，QML 单例与 Python `theme_bridge.color(token) -> str` 同源加载），并有 `changed` 信号驱动 §4.6.5 的重高亮；
- 状态栏绑定点：`editorService.activeDocument` 的 `cursorLine/cursorColumn/eol/encoding/readOnly`；
- `EditorArea` 在中央编辑器区的挂载、窗口关闭前调用 `editorService.requestCloseAll()`、工作区切换前同样调用（见 §4.4.6）。

### 3.4 对 13 号（命令注册表）的需求

- 按跨模块约定消费 `CommandRegistry.register(id, title, category, callback, shortcut=None)`；11 号提供纯数据命令清单 `editor/commands.py`（§4.8），由 10 号在装配时注册；
- 需要的上下文键（13 号 when 体系）：`editorFocus`、`editorOpenCount`、`activeEditorDirty`、`findBarVisible`（键值由 `EditorService` 属性提供，接线在 10 号装配层）。

### 3.5 对 15 号（差异化）暴露的挂点

见 §5；本文不实现任何差异化功能本体。

---

## 4. 详细设计

### 4.1 总体结构与数据流

```
QML (纯渲染 + 动作提交)                    Python (状态权威)                     Sidecar
┌─────────────────────────┐   Slot 调用    ┌──────────────────────────┐  fs.* IPC  ┌─────────┐
│ EditorArea               │ ────────────→ │ EditorService (QObject)  │ ─────────→ │ fs.read │
│  ├ EditorTabBar          │               │  ├ OpenDocumentsModel    │ ←───────── │ fs.stat │
│  ├ FindReplaceBar        │  属性/模型/    │  ├ EditorDocument × N    │            │ fs.write│
│  └ StackLayout           │  信号(单向)    │  │   └ QTextDocument 引用 │            └─────────┘
│     └ EditorPane × N     │ ←──────────── │  ├ SearchController      │      （路径牢笼/信任门禁/
│        ├ EditorGutter    │               │  ├ HighlightEngine × N   │        归因钩子都在 Sidecar）
│        ├ TextArea ───────┼─ attach ────→ │  │   └ QThreadPool worker │
│        └ 弹窗(关闭/冲突)  │  QQuickText   │  └ FsIo（fs.* 单点适配）  │
└─────────────────────────┘   Document     └──────────────────────────┘
```

数据流纪律（R2 借鉴 1）：

- QML 不持有业务状态：标签数据来自 `OpenDocumentsModel`，文档元数据来自 `EditorDocument` 属性，全部单向下行；
- 用户动作（打开/保存/关闭/查找/跳转）一律经 `EditorService` 的 Slot 上行，绝无 QML 侧旁路；
- 唯一的双向对象是 `QTextDocument`（文本内容 + 撤销栈）：由 QML `TextArea` 创建并持有，经 `attachTextDocument` 交给 Python 侧引用（加载内容、挂高亮、读文本保存）。这是 Qt Quick 文本控件的既有结构，不违反单向流——`QTextDocument` 是内容层，状态层仍在 Python。
- **线程模型**：`QTextDocument` 只在主线程访问；IPC 回调经 client 桥保证已在主线程（`viewmodels/base.py` 约定）；唯一的后台工作是高亮 lex（`QThreadPool`，只进出纯 Python 数据，见 §4.6.4）。

### 4.2 `FsIo` —— fs.* 消费的单点适配层

`app/halo_studio/editor/fsio.py`。职责：把 12 号契约的分块/编码细节收敛到一个文件，`EditorDocument`/`EditorService` 只见下面的稳定接口：

```python
@dataclass(frozen=True)
class FileReadResult:
    text: str            # 已解码文本（内部统一 \n）
    encoding: str        # "utf-8" | "utf-8-sig" | "gbk" | "utf-8-replace"
    has_bom: bool
    eol: str             # "crlf" | "lf"
    mixed_eol: bool
    size: int            # 磁盘字节数
    sha256: str
    mtime: str           # RFC3339
    binary: bool         # True 时 text 为空，调用方拒绝打开
    decode_lossy: bool   # replace 解码 → 强制只读

@dataclass(frozen=True)
class FileStatResult:
    exists: bool
    size: int
    mtime: str
    sha256: str

@dataclass(frozen=True)
class FileWriteResult:
    sha256: str
    mtime: str

class FsIo:
    """全部异步；on_ok/on_err 在主线程回调。client 为 viewmodels/base.py 鸭子类型。"""
    def __init__(self, client) -> None: ...
    def read(self, path: str,
             on_ok: Callable[[FileReadResult], None],
             on_err: Callable[[dict], None]) -> None: ...      # 内部分块续读拼装
    def stat(self, path: str,
             on_ok: Callable[[FileStatResult], None],
             on_err: Callable[[dict], None]) -> None: ...
    def write(self, path: str, data: bytes, expected_sha256: str | None,
              on_ok: Callable[[FileWriteResult], None],
              on_err: Callable[[dict], None]) -> None: ...     # 内部分块提交
```

- `on_err` 收到 `{"code","message","details"}`（契约错误体原样，中文 message 直接可显示）；
- 解码/EOL 探测策略属编辑器语义，定义在 §4.3.2/§4.3.3，实现于 `fsio.py`（若 12 号把探测放到 Sidecar，则此处直通字段，策略条文仍以本文为准）。

### 4.3 `EditorDocument` —— 文档模型

`app/halo_studio/editor/document.py`。

```python
class EditorDocument(QObject):
    # ---- 身份与元数据（QML 可读属性，均带 notify） ----
    documentId: str        # "doc-<uuid4>"，创建即定，不随路径变
    path: str              # 工作区相对路径，'/' 分隔（与 ReviewBundle files[].path 同风格）
    fileName: str          # 末段文件名
    title: str             # 去歧义显示名：同名文件追加父目录（"main.py — app" 风格）
    dirty: bool            # 源于 QTextDocument.modificationChanged，实时值（R1 借鉴：不缓存快照）
    readOnly: bool         # oversized 或 decode_lossy 或 binary 拒绝前的兜底
    oversized: bool        # size > READONLY_MAX_BYTES 时为 True（>8MiB 只读）
    highlightEnabled: bool # size <= HIGHLIGHT_MAX_BYTES（1MiB）时 True
    eol: str               # "crlf" | "lf"（保存时按此还原）
    mixedEol: bool         # 打开时两种行尾并存 → 状态栏显示 "CRLF*"/"LF*"
    encoding: str          # 保存时按此写回；"utf-8-replace" 强制只读不可保存
    hasBom: bool
    lineCount: int         # = QTextDocument.blockCount，gutter 数据源
    manualEditBadge: bool  # task.manual_edit 提示徽章（§4.9）
    baselineChanged: bool  # 15 号"基线感知徽章"挂点，默认 False
    state: str             # "loading" | "ready" | "saving" | "conflict"
    cursorLine: int        # 1 起；EditorPane 上报
    cursorColumn: int      # 1 起

    # ---- 生命周期（仅 EditorService 调用，非 QML API） ----
    def attach(self, qdoc: QTextDocument) -> None: ...
        # setPlainText(读取结果.text) → clearUndoRedoStacks() → setModified(False)
        # → connect modificationChanged/blockCountChanged → 挂 HighlightEngine（若 enabled）
    def is_attached(self) -> bool: ...
    def build_save_payload(self) -> bytes: ...
        # toPlainText() → '\n' 替换为 eol 对应串 → encode(encoding) → 前置 BOM（若 hasBom）
    def undo(self) / redo(self) -> None: ...   # 直接调 QTextDocument.undo()/redo()
    # 内部：_disk_sha256 / _disk_mtime（打开与每次保存成功后更新，冲突检测基准）
```

**撤销栈**：每个打开文档独占一个 `QTextDocument`（见 §4.5 编辑器栈结构），原生 undo/redo 天然按文档隔离；加载与"放弃本地重载"后 `clearUndoRedoStacks()`（重载会丢撤销历史——如实提示，见 §4.3.4）。不自研撤销。

#### 4.3.1 常量（`editor/constants.py`）

```python
HIGHLIGHT_MAX_BYTES   = 1 * 1024 * 1024   # >1MiB 关闭语法高亮
READONLY_MAX_BYTES    = 8 * 1024 * 1024   # >8MiB 只读打开并提示；再大由 12 号 fs.read 上限直接拒绝
SYNC_LEX_MAX_BYTES    = 64 * 1024         # ≤64KiB 打开时同步首次 lex（避免颜色闪烁）
HIGHLIGHT_LINE_MAX    = 4096              # 单行超长（字符）跳过该行高亮（R2 §6 护栏）
HIGHLIGHT_DEBOUNCE_MS = 200
MATCH_COUNT_CAP       = 10_000            # 查找计数上限，超限显示 "10000+"
```

#### 4.3.2 编码策略（UTF-8 与 fallback）

打开（对 `fs.read` 字节流，BOM 优先）：

1. 有 UTF-8 BOM → 剥离，`encoding="utf-8-sig"`, `hasBom=True`；
2. 无 BOM → `utf-8` 严格解码；成功 → `encoding="utf-8"`；
3. 失败 → `gbk` 严格解码（Windows 中文首发环境最常见的遗留编码）；成功 → `encoding="gbk"`，状态栏显示 GBK；
4. 仍失败 → `utf-8` + `errors="replace"` 解码，`encoding="utf-8-replace"`，`decode_lossy=True` → **强制只读**并提示"编码无法识别，已按 UTF-8 替换性解码，为防数据损坏本文件只读"。

保存：一律按打开时记录的 `encoding` 写回（`utf-8-sig` 还原 BOM；`gbk` 编码失败——用户输入了 GBK 无法表示的字符——保存报错并提示"另存需转 UTF-8"，首期不做转码 UI，列远期）。**不提供编码切换 UI**（远期）。

#### 4.3.3 行尾风格保留（CRLF/LF）

- 打开探测：`crlf = text.count("\r\n")`，`bare_lf = text.count("\n") - crlf`；两者皆 >0 → `mixedEol=True`，主导者为 `eol`；全无换行 → `eol="crlf"`（Windows 首发默认）；
- `QTextDocument` 内部统一 `\n`（`setPlainText` 前由 `FsIo` 归一）；保存时 `build_save_payload()` 按 `eol` 还原；
- `mixedEol` 文件保存后**归一为主导行尾**（如实：状态栏 "CRLF*" 悬停提示"首次保存将统一为 CRLF"）。不做逐行保留（成本与收益不成比）。

#### 4.3.4 磁盘版本冲突检测（保存流程）

打开与每次保存成功后记录 `_disk_sha256/_disk_mtime`。保存状态机：

```
save(id):
  1. not dirty → no-op（saveAll 跳过干净文档）
  2. state=saving; payload = build_save_payload()
  3. FsIo.stat(path)
       ├─ sha256 == _disk_sha256 → 4
       └─ 不一致（或 exists=False=磁盘被删）→ state=conflict
            → conflictDetected(documentId, path) → QML ConflictDialog 三选：
              [覆盖保存]  → 4（expected_sha256 = stat 返回的现值，防二次竞态）
              [放弃本地并重载] → FsIo.read → attach 重灌 → 清撤销栈 → setModified(False) → 完成
              [取消]      → state=ready（保持 dirty，不写盘）
  4. FsIo.write(path, payload, expected_sha256=_disk_sha256 或步骤3现值)
       ├─ ok → 更新 _disk_sha256/_disk_mtime ← result；setModified(False)；state=ready
       ├─ 写冲突错误码 → 回到 3 的冲突分支（重新 stat 后弹窗）
       └─ 其他错误 → state=ready（仍 dirty）；saveFailed(documentId, code, message)
```

- mtime 只作展示辅助，**冲突判定只认 sha256**（mtime 精度与拷贝工具行为不可靠）；
- 若 12 号最终未提供 `expected_sha256`，流程退化为 stat-then-write，竞态窗口如实记入风险（§8）。

#### 4.3.5 外部变更的轻量提醒（SHOULD，非首期验收项）

窗口重获焦点时对 `activeDocument` 发一次 `fs.stat`：sha256 变化且本地不脏 → 静默重载；变化且本地脏 → 标签显示"磁盘已变化"小图标（不打断输入，真正的裁决仍在保存时冲突流程）。

### 4.4 `EditorService` 与 `OpenDocumentsModel`

`app/halo_studio/editor/service.py`。

#### 4.4.1 `EditorService(QObject)` 公共 API（跨模块契约面）

```python
def create_editor_service(client) -> EditorService: ...   # app.py 装配入口

class EditorService(QObject):
    # ======== 属性（QML/10 号状态栏/13 号上下文键消费） ========
    documents: OpenDocumentsModel          # Property(QObject, constant) —— 标签页数据源
    activeDocumentId: str                  # notify=activeChanged
    activeDocument: EditorDocument | None  # notify=activeChanged（None=空态占位）
    openCount: int                         # notify（13 号 editorOpenCount 键）
    search: SearchController               # Property(QObject, constant)，§4.7
    currentSelection: dict                 # notify；{"path","startLine","startColumn","endLine","endColumn","hasSelection"}（15 号挂点）

    # ======== 打开 / 激活 ========
    @Slot(str) @Slot(str, int) @Slot(str, int, bool)
    def openFile(self, path: str, line: int = -1, preview: bool = False) -> None
        # 路径归一化（'/'、Windows 下 casefold 作身份键，保留原大小写显示）
        # 已打开（matches）→ activate + 可选跳行；未打开 → FsIo.read 异步加载
        # binary=True → openFailed(path, "EDITOR_BINARY", …)，不建标签
        # line >= 1 → 加载/激活完成后 gotoLineRequested(documentId, line, 1)
    @Slot(str)
    def activate(self, document_id: str) -> None        # 切活动标签 + 更新 MRU
    @Slot()
    def nextTab(self) -> None                           # MRU 序循环（Ctrl+Tab）
    @Slot()
    def prevTab(self) -> None                           # MRU 逆序（Ctrl+Shift+Tab）

    # ======== 保存 ========
    @Slot() @Slot(str)
    def save(self, document_id: str = "") -> None       # 缺省=活动文档；流程 §4.3.4
    @Slot()
    def saveAll(self) -> None                           # 逐个走 save；失败不中断其余
    @Slot(str, str)
    def resolveConflict(self, document_id: str, decision: str) -> None
        # decision: "overwrite" | "reload" | "cancel"（ConflictDialog 回调）

    # ======== 关闭（三态 + veto，R1 借鉴 4） ========
    @Slot(str)
    def closeTab(self, document_id: str) -> None
        # 不脏 → 直接关；脏 → closeConfirmationRequested(documentId, title)
    @Slot(str, str)
    def resolveClose(self, document_id: str, decision: str) -> None
        # "save"    → 走保存流程，成功才关；保存失败/冲突取消 → 标签保留（veto）
        # "discard" → 直接关（关闭即销毁，无需 revert）
        # "cancel"  → 不关
    @Slot()
    def requestCloseAll(self) -> None
        # 逐个对脏文档走确认；任一 cancel → allCloseFinished(False) 且停止
        # 全部关闭 → allCloseFinished(True)。10 号在窗口关闭/工作区切换前调用并等结果

    # ======== 编辑动作路由（命令面板/快捷键 → 活动文档） ========
    @Slot()
    def undo(self) / redo(self) -> None                 # activeDocument.undo()/redo()
    @Slot(int) @Slot(int, int)
    def gotoLine(self, line: int, column: int = 1) -> None
        # 夹取到 [1, lineCount] → gotoLineRequested(activeDocumentId, line, column)

    # ======== QML 视图接线 ========
    @Slot(str, QQuickTextDocument)
    def attachTextDocument(self, document_id: str, quick_doc) -> None
        # EditorPane Component.onCompleted 调用；取 quick_doc.textDocument() 交 EditorDocument.attach
    @Slot(str, int, int)
    def reportCursor(self, document_id: str, line: int, column: int) -> None
    @Slot(str, "QVariantMap")
    def reportSelection(self, document_id: str, sel: dict) -> None

    # ======== 15 号差异化挂点（本期只有存取与信号，无消费者） ========
    @Slot(str, "QVariantList")
    def setGutterDecorations(self, document_id: str, decorations: list) -> None
        # decorations: [{"line": int(1起), "kind": str, "colorToken": str, "tooltip": str}]
        # 内部转 QTextCursor 锚点存储（R2 借鉴 2）；文档编辑后行号自动维持
    @Slot("QVariantList")
    def setBaselineChangedPaths(self, paths: list) -> None   # 命中标签 → baselineChanged=True

    # ======== 信号 ========
    activeChanged            = Signal()
    gotoLineRequested        = Signal(str, int, int)     # documentId, line, column（EditorPane 消费）
    closeConfirmationRequested = Signal(str, str)        # documentId, title
    conflictDetected         = Signal(str, str)          # documentId, path
    saveFailed               = Signal(str, str, str)     # documentId, code, message(中文)
    openFailed               = Signal(str, str, str)     # path, code, message(中文)
    allCloseFinished         = Signal(bool)              # True=全部关闭 / False=用户取消
    manualEditMarked         = Signal("QVariantList")    # 命中路径列表（可空，§4.9）
```

行/列约定：**对外 API 与显示一律 1 起**；`QTextDocument.blockNumber` 0 起仅内部使用。

#### 4.4.2 `OpenDocumentsModel(QAbstractListModel)` —— 标签页数据源

纯状态机（R1 借鉴 3），角色：

| 角色 | 类型 | 说明 |
| --- | --- | --- |
| `documentId` | str | 委托与 Service API 的关联键 |
| `path` / `fileName` / `title` | str | title 含同名去歧义 |
| `dirty` | bool | 脏点 ●（`tabDirtyForeground`） |
| `readOnly` | bool | 锁图标 |
| `preview` | bool | 预览标签斜体（§4.4.4） |
| `manualEditBadge` | bool | 人工编辑提示徽章（§4.9） |
| `baselineChanged` | bool | 15 号基线徽章挂点 |
| `eol` / `encoding` | str | tooltip 展示 |

内部维护：顺序表（标签显示序）、MRU 表（`activate` 时置顶）。关闭活动标签的继任者 = MRU 次位（R1 §2.2）。模型只由 `EditorService` 变更，QML `EditorTabBar` 纯渲染。

#### 4.4.3 打开去重（matches 语义）

身份键 = 归一化路径（`/` 分隔 + Windows `casefold`）。`openFile` 命中已开文档 → 激活 + 跳行，不建新标签（R1 EditorInput.matches 思想）。

#### 4.4.4 预览标签（R1 借鉴 3，可裁剪项）

`openFile(..., preview=True)`（12 号资源管理器单击时传入）：同组至多一个 preview 标签，新的 preview **替换**旧 preview（复用同一标签位）；文档被编辑（首次 `modificationChanged(True)`）、双击标签、或再次以 `preview=False` 打开 → 转正。**裁剪线**：若实施进度紧张，`preview` 参数保留但恒转正（全部 pinned），模型角色与 API 不变，12/13 号不受影响。

#### 4.4.5 加载状态

`openFile` 建标签即入模型（`state="loading"`，Pane 显示细进度条），`FsIo.read` 完成后 `attach`；读失败 → 移除标签 + `openFailed`。避免"点击无反应"。

#### 4.4.6 与工作区生命周期的关系

- 订阅 `workspace.changed`：工作区关闭或切换成功事件到达时，若仍有打开文档（正常流程 10 号已先 `requestCloseAll`），**强制静默关闭全部**（此时旧工作区 fs 能力已失效，保留标签只会产生误导）；
- 订阅 `client.disconnected`：所有文档转 `readOnly=True` + 状态栏原因（Sidecar 不在 → 无 fs 能力 → 不允许继续编辑造成"保存必失败"的假象）。

### 4.5 QML 组件树

新目录 `app/halo_studio/qml/editor/`（qmldir 单列，Theme 经 `qml/theme` 引入）：

```
EditorArea.qml                          ── ColumnLayout（10 号挂到中央编辑器区）
├── EditorTabBar.qml                    ── 横向 ListView，model: editorService.documents
│    └── delegate: EditorTab
│         ├── Text: title（preview → font.italic）
│         ├── 徽章行：dirty ●（tabDirtyForeground）｜readOnly 锁｜manualEditBadge ⚑｜baselineChanged M
│         ├── 关闭按钮（hover 显示；dirty 时常显 ●，hover 变 ×）
│         └── MouseArea：单击 activate / 双击转正 preview / 中键 closeTab
├── FindReplaceBar.qml                  ── visible: editorService.search.active（§4.7）
└── StackLayout                         ── currentIndex 绑定 activeDocumentId 对应行
     ├── Repeater { model: editorService.documents
     │    delegate: EditorPane.qml      ── 每文档一个实例，标签存续期内不销毁（R1 借鉴 5 的 QML 适配：
     │  }                                  TextArea 的 QTextDocument 不可换绑 → "一文档一 Pane + 栈切换"
     │                                     同样达成"切换不重建"）
     └── EmptyEditorPlaceholder.qml     ── openCount === 0：产品名 + 常用命令提示

EditorPane.qml
├── RowLayout (spacing: 0)
│    ├── EditorGutter.qml               ── 行号 + 装饰列（§4.5.2）
│    └── Flickable (id: flick, clip)    ── TextArea.flickable 附加集成，横向滚动（NoWrap）
│         └── TextArea (id: textArea)
│              ├── font: Theme.fontMono；wrapMode: NoWrap；readOnly: doc.readOnly
│              ├── tabStopDistance: 4 字符宽；selectionColor: Theme.editorSelectionBackground
│              ├── Component.onCompleted:
│              │     editorService.attachTextDocument(doc.documentId, textArea.textDocument)
│              ├── Rectangle (z:-1) 当前行高亮：y/height 随 cursorRectangle，
│              │     color: Theme.editorLineHighlight（负 z 子项绘制在文本之下）
│              ├── FindMatchOverlay (z:-1)  ── Repeater: search.visibleMatches(可见首末位置)
│              │     → positionToRectangle 命中矩形，color: Theme.editorFindMatchHighlightBackground
│              ├── onCursorPositionChanged → editorService.reportCursor(id, line, column)
│              └── onSelectedTextChanged  → editorService.reportSelection(id, {…})
├── Connections { target: editorService  ── gotoLineRequested(id==本文档) →
│    设置 cursorPosition = 该行列的 document 位置，flick 滚动至可见并短暂闪烁当前行 }
└── GotoLinePopup.qml                   ── Ctrl+G 弹出："行[:列]" 输入，实时夹取预览

EditorArea 级弹窗（单例，按信号弹出）：
├── CloseConfirmDialog.qml              ── closeConfirmationRequested → [保存] [不保存] [取消]
│                                          → editorService.resolveClose(id, decision)
└── ConflictDialog.qml                  ── conflictDetected → [覆盖保存] [放弃本地并重载] [取消]
                                           → editorService.resolveConflict(id, decision)
```

#### 4.5.1 坐标空间

首期 `NoWrap` → 缓冲区行 == 显示行，行高恒定 = `fontMetrics.lineSpacing`。属性命名仍区分 `bufferLine`（文件真实行，对外 API 用）以便远期软折行不破坏接口（R2 借鉴 3）。

#### 4.5.2 `EditorGutter.qml` —— 行号栏与装饰列

- 纯计算渲染：`firstVisible = floor(flick.contentY / lineHeight)`，`visibleCount = ceil(height / lineHeight) + 1`，`Repeater` 只生成可见行号（R2 借鉴：只布局可见行）；
- 宽度 = `max(3, digits(lineCount)) * charWidth + 12px 内边距 + 6px 装饰列`；
- 当前行行号用 `Theme.editorActiveLineNumberForeground`，其余 `editorLineNumberForeground`；
- 点击行号 → 选中整行；
- **装饰列（15 号 gutter 装饰接口的渲染面）**：`doc.gutterDecorations`（由 `setGutterDecorations` 写入、锚点解析回行号的只读列表属性，`gutterChanged` 通知）中命中可见行者，画 6px 色条（`color: Theme[dec.colorToken]`）+ tooltip。kind 仅作 15 号语义扩展位，渲染只认 colorToken。

### 4.6 语法高亮

`app/halo_studio/editor/highlight.py`。venv 已含 Pygments。

#### 4.6.1 结构

```python
def pick_lexer(path: str) -> pygments.lexer.Lexer | None
    # 常见扩展名直查表 LEXER_BY_EXT（避免 Pygments 猜测开销）：
    # .py .pyw→python  .rs→rust  .toml→toml  .json→json  .md→markdown  .qml→qml
    # .js .mjs→javascript  .ts→typescript  .html→html  .css→css  .yaml .yml→yaml
    # .xml→xml  .sh→bash  .ps1→powershell  .c .h→c  .cpp .hpp→cpp  .go→go  .java→java
    # 未命中 → get_lexer_for_filename 兜底；再未命中 → None（纯文本，不建引擎）

class HighlightWorker(QRunnable):
    # 输入：generation:int, text:str, lexer 名；纯 Python，不触任何 QObject
    # 输出（经跨线程信号回主线程）：generation, line_runs: list[list[tuple[col,len,style_key]]]
    # 单行 len(line) > HIGHLIGHT_LINE_MAX → 该行输出空 run（护栏）

class DocumentHighlighter(QSyntaxHighlighter):
    # highlightBlock(text)：按 currentBlock().blockNumber() 查 line_runs 缓存 → setFormat 序列
    # apply_line_runs(new_runs)：与旧缓存逐行 diff，仅对变化行 rehighlightBlock（避免全量重刷）

class HighlightEngine(QObject):
    def __init__(self, doc: EditorDocument, qdoc: QTextDocument, theme_bridge): ...
    # attach 时：文件 ≤ SYNC_LEX_MAX_BYTES → 主线程同步首刷（打开即有色，R2 同步快路径）
    #            否则直接走后台
    # qdoc.contentsChanged → QTimer 去抖 HIGHLIGHT_DEBOUNCE_MS → snapshot toPlainText()
    #   → generation += 1 → QThreadPool.globalInstance().start(HighlightWorker)
    # worker 回包 generation != 当前 → 丢弃（代际取消，R2 借鉴）
    def set_enabled(self, enabled: bool): ...   # False → 清格式，摘除 highlighter
```

去抖窗口内的中间态：变化块由 `QSyntaxHighlighter` 立即用**旧缓存行**着色（可能短暂错位 ≤ 去抖 + lex 时长），worker 回包后 diff 修正——高亮永不阻塞输入是首要不变量。

#### 4.6.2 大文件降级

| 条件 | 行为 |
| --- | --- |
| `size > HIGHLIGHT_MAX_BYTES`（1 MiB） | 不建 `HighlightEngine`，`highlightEnabled=False`，状态栏提示"大文件已关闭语法高亮" |
| `size > READONLY_MAX_BYTES`（8 MiB） | 只读打开 + 顶部条提示"文件过大，已只读打开"；同时必然无高亮 |
| 超过 12 号 `fs.read` 绝对上限 | 打开失败，`openFailed` 透传契约错误文案 |
| 单行 > `HIGHLIGHT_LINE_MAX`（4096 字符） | 仅该行跳过高亮 |

#### 4.6.3 token → Theme 颜色映射

对 10 号 Theme 的语法令牌需求（12 个）：`syntaxKeyword` `syntaxString` `syntaxComment` `syntaxNumber` `syntaxFunction` `syntaxType` `syntaxAttribute` `syntaxBuiltin` `syntaxConstant` `syntaxOperator` `syntaxVariable` `syntaxError`。

映射表（有序，取第一个命中；Pygments token 用 `in`（含义为"属于该族"）沿父链回退——`Token.String.Doc in Token.String` 成立）：

| Pygments token 族 | Theme 令牌 | 附加样式 |
| --- | --- | --- |
| `Token.Comment` | `syntaxComment` | italic |
| `Token.Keyword.Constant` | `syntaxConstant` | |
| `Token.Keyword` | `syntaxKeyword` | |
| `Token.String` | `syntaxString` | |
| `Token.Number` | `syntaxNumber` | |
| `Token.Name.Function` | `syntaxFunction` | |
| `Token.Name.Class` | `syntaxType` | |
| `Token.Name.Decorator` / `Token.Name.Attribute` / `Token.Name.Tag` | `syntaxAttribute` | |
| `Token.Name.Builtin` | `syntaxBuiltin` | |
| `Token.Name.Constant` | `syntaxConstant` | |
| `Token.Operator` | `syntaxOperator` | |
| `Token.Error` | `syntaxError` | 波浪下划线不做，仅前景色 |
| 其余（`Token.Text`/`Punctuation`/`Generic.*`…） | `editorForeground` | 不 setFormat（省调用） |

`style_key` 即 Theme 令牌名字符串；`DocumentHighlighter` 持 `dict[style_key, QTextCharFormat]`，由 ThemeBridge 颜色构建。

#### 4.6.4 线程纪律

worker 只接收 `str` 与 lexer 名、只返回纯 list/tuple；`QTextDocument`/`QTextCharFormat` 绝不跨线程。回主线程经 `HighlightEngine` 的 Signal（AutoConnection → QueuedConnection）。

#### 4.6.5 主题切换

ThemeBridge `changed` → 重建全部格式表 → 各引擎 `rehighlight()`（全量，一次性操作可接受）。

### 4.7 查找 / 替换 / 跳转行

`app/halo_studio/editor/search.py`。作用域 = **当前活动文档**（跨文件搜索属 12 号 `fs.search`，见 §4.8 入口）。

```python
class SearchController(QObject):
    # 属性（notify 略）：active: bool; replaceVisible: bool; query: str; replaceText: str
    #                  useRegex: bool; caseSensitive: bool; wholeWord: bool
    #                  matchCount: int   # -1=未计算；MATCH_COUNT_CAP 处截断（UI 显示 "10000+"）
    #                  currentIndex: int # 1 起，0=无当前
    #                  regexError: str   # 非法正则的中文提示（QRegularExpression.errorString）

    @Slot(bool) def open(self, with_replace: bool) -> None
        # 打开搜索条；有选区 → 预填 query（选区≤256字符时）；聚焦输入框
    @Slot()     def close(self) -> None      # Esc；清覆盖层，焦点还给编辑器
    @Slot(str)  def setQuery(self, q) -> None    # 去抖 100ms 重算 matchCount 与首个命中
    @Slot()     def findNext(self) / findPrevious(self) -> None
        # QTextDocument.find(QRegularExpression|str, from, flags)，端部回绕；
        # 命中 → TextArea.select(start,end)（当前命中即原生选区）
    @Slot()     def replaceCurrent(self) -> None
        # 当前选区 == 当前命中 → QTextCursor.insertText（单步撤销）→ findNext
        # 正则替换支持 $1..$9 反向引用（QRegularExpressionMatch.captured 展开）
    @Slot()     def replaceAll(self) -> None
        # 全文迭代命中 → 一个 beginEditBlock/endEditBlock 内逐个替换 = 单次撤销；
        # 完成提示"已替换 N 处"
    @Slot(int, int, result="QVariantList")
    def visibleMatches(self, from_pos: int, to_pos: int) -> list  # [{"start","length"}...]
        # FindMatchOverlay 数据源：只算可见区间，滚动/键入时重取；
        # 不用 QTextCursor.setCharFormat 做全量命中着色——那会污染撤销栈
```

- 匹配实现统一走 `QTextDocument.find`（字面量含 `FindCaseSensitively/FindWholeWords` 标志；正则用 `QRegularExpression`，caseSensitive 映射 PatternOption）；
- `wholeWord + useRegex` 组合：正则外包 `\b(?:…)\b`；
- 文档变更且搜索条开着 → 去抖重算 matchCount / 覆盖层；
- **跳转行**：`GotoLinePopup`（Ctrl+G）输入 `行` 或 `行:列` → `editorService.gotoLine(line, column)`；越界夹取；输入过程实时预览滚动（取消则回原位）。

`FindReplaceBar.qml`：单行紧凑条（query 输入、大小写/整词/正则三个 toggle、上一个/下一个、计数 `currentIndex/matchCount`）+ 可展开的替换行（replace 输入、替换当前/全部替换）+ 「在文件中搜索…」入口按钮（执行命令 `workbench.searchInFiles`，实现属 12 号）。

### 4.8 命令与快捷键（经 CommandRegistry）

`app/halo_studio/editor/commands.py` 导出纯数据清单，10 号装配时逐条 `CommandRegistry.register(id, title, category, callback, shortcut)`（注册表细节属 13 号）：

```python
@dataclass(frozen=True)
class EditorCommandSpec:
    id: str; title: str; category: str; shortcut: str | None
    method: str                 # EditorService/SearchController 上的方法名
    precondition: str | None    # 13 号上下文键表达式（平面 and/not）

def editor_commands(service: EditorService) -> list[tuple[EditorCommandSpec, Callable]]: ...
```

| 命令 id | 标题 | 快捷键 | 回调 | precondition |
| --- | --- | --- | --- | --- |
| `editor.save` | 保存文件 | `Ctrl+S` | `service.save()` | `editorOpenCount > 0` |
| `editor.saveAll` | 全部保存 | `Ctrl+Shift+S` | `service.saveAll()` | 同上 |
| `editor.closeTab` | 关闭标签页 | `Ctrl+W` | `service.closeTab(activeDocumentId)` | 同上 |
| `editor.nextTab` | 下一个标签页（最近使用序） | `Ctrl+Tab` | `service.nextTab()` | 同上 |
| `editor.prevTab` | 上一个标签页 | `Ctrl+Shift+Tab` | `service.prevTab()` | 同上 |
| `editor.find` | 查找 | `Ctrl+F` | `service.search.open(False)` | `editorFocus` |
| `editor.replace` | 替换 | `Ctrl+H` | `service.search.open(True)` | `editorFocus && !activeEditorReadOnly` |
| `editor.gotoLine` | 跳转到行… | `Ctrl+G` | GotoLinePopup 打开 | `editorFocus` |
| `editor.undo` | 撤销 | `Ctrl+Z` | `service.undo()` | `editorFocus`（TextArea 原生已处理，注册供命令面板） |
| `editor.redo` | 重做 | `Ctrl+Y` | `service.redo()` | 同上 |
| `editor.reopenFromDisk` | 放弃修改并重新加载 | — | `service.resolveConflict(active, "reload")` 的独立入口 | `editorOpenCount > 0` |

- 快捷键均为单和弦（避开 13 号双和弦语法依赖）；`Ctrl+Tab` 循环 MRU（首期无浮层预览列表）；
- 全局搜索 `workbench.searchInFiles`（`Ctrl+Shift+F`）**由 12 号注册与实现**，本表不含，仅 FindReplaceBar 提供入口按钮；
- 上下文键 `editorFocus/editorOpenCount/activeEditorDirty/activeEditorReadOnly/findBarVisible` 的值由 `EditorService`/`SearchController` 属性供给，接线在 10 号装配层，键语义归 13 号。

### 4.9 与运行中任务的关系

- **保存 = 纯文件写入**：`save` 路径不查询任务状态、不调用 `task.mark_manual_edit`、不携带归因参数。归因判定完全在 Sidecar `fs.write` 处理器内（运行中任务 + 写入命中工作区 → 自动 `task.mark_manual_edit`，属 15 号设计），编辑器无感知。这保证归因逻辑单点、不可被 UI 旁路。
- **`task.manual_edit` 事件消费**：`EditorService` 订阅该事件（client 鸭子类型 `subscribe`）。payload 现契约仅 `{"note"}`；若 15 号追加式扩展出 `files: [path...]`（相对路径）：
  - 有 `files` → 命中打开文档 `manualEditBadge=True`（标签 ⚑ 徽章，tooltip="本任务期间该文件发生人工编辑，归因已转 Mixed"）；
  - 无 `files` → 不猜测具体文件，发 `manualEditMarked([])`，由 10 号状态栏显示全局提示"已记录人工编辑（归因 Mixed）"。
  - 徽章生命周期：任务进入终态（订阅 `task.state`，`is_terminal` 状态）→ 清除全部 `manualEditBadge`。
- 审查视图只读不变；"审查→编辑器跳转"（15 号）只是 `openFile(path, line)` 的调用方。

---

## 5. 差异化点（仅挂点，裁决权在 15 号）

| 03 号差异化功能 | 本文提供的挂点 | 挂点位置 |
| --- | --- | --- |
| 归因边栏（gutter） | `setGutterDecorations(documentId, [{line,kind,colorToken,tooltip}])`，锚点存储、编辑后行号自动维持；渲染在 `EditorGutter` 装饰列 | §4.4.1 / §4.5.2 |
| 人工介入自动归因 | 归因钩子在 Sidecar `fs.write`（15 号）；编辑器侧 = `task.manual_edit` 徽章 + `manualEditMarked` 信号 | §4.9 |
| 基线感知徽章 | `setBaselineChangedPaths(paths)` → 标签 `baselineChanged` 角色（M 字徽章，色令牌 `baselineChangedBadgeForeground`） | §4.4.1 / §4.4.2 |
| 任务上下文选择器 | `currentSelection` 属性（path + 1 起行列区间 + hasSelection），15 号读取后自行拼任务说明 | §4.4.1 |
| 审查→编辑器跳转 | `openFile(path, line)` 即跳转 API，无需新增 | §4.4.1 |

---

## 6. 实施计划

### 6.1 新建文件

| 文件 | 内容 | 预估规模 |
| --- | --- | --- |
| `app/halo_studio/editor/__init__.py` | `create_editor_service` 工厂导出 | ~30 行 |
| `app/halo_studio/editor/constants.py` | §4.3.1 常量 | ~20 行 |
| `app/halo_studio/editor/fsio.py` | `FsIo` + 三个 Result dataclass + 编码/EOL 探测 | ~250 行 |
| `app/halo_studio/editor/document.py` | `EditorDocument` | ~300 行 |
| `app/halo_studio/editor/service.py` | `EditorService` + `OpenDocumentsModel` | ~550 行 |
| `app/halo_studio/editor/highlight.py` | `pick_lexer` / `HighlightWorker` / `DocumentHighlighter` / `HighlightEngine` | ~350 行 |
| `app/halo_studio/editor/search.py` | `SearchController` | ~300 行 |
| `app/halo_studio/editor/commands.py` | `EditorCommandSpec` + 命令清单 | ~80 行 |
| `app/halo_studio/qml/editor/qmldir` | 模块声明 | ~15 行 |
| `app/halo_studio/qml/editor/EditorArea.qml` | 区域装配 | ~150 行 |
| `app/halo_studio/qml/editor/EditorTabBar.qml` | 标签条 + 徽章 | ~180 行 |
| `app/halo_studio/qml/editor/EditorPane.qml` | TextArea + 附加集成 + 当前行高亮 + 覆盖层 | ~250 行 |
| `app/halo_studio/qml/editor/EditorGutter.qml` | 行号 + 装饰列 | ~120 行 |
| `app/halo_studio/qml/editor/FindReplaceBar.qml` | 查找替换条 | ~180 行 |
| `app/halo_studio/qml/editor/GotoLinePopup.qml` | 跳转行 | ~80 行 |
| `app/halo_studio/qml/editor/CloseConfirmDialog.qml` | 三态关闭确认 | ~70 行 |
| `app/halo_studio/qml/editor/ConflictDialog.qml` | 冲突三选 | ~70 行 |
| `app/halo_studio/qml/editor/EmptyEditorPlaceholder.qml` | 空态 | ~50 行 |
| `app/tests/test_editor_fsio.py` `test_editor_document.py` `test_editor_service.py` `test_editor_highlight.py` `test_editor_search.py` `test_editor_no_local_io.py` | §7 测试 | ~900 行合计 |

### 6.2 修改文件

| 文件 | 修改 | 协调方 |
| --- | --- | --- |
| `app/halo_studio/app.py` | 装配 `editorService` 上下文属性（工厂调用 + 强引用入 `AppContext`） | 10 号统筹 |
| `docs/module-contracts.md` | §3.2 所列增量 | 设计评审后 |
| `docs/design/README.md` | 11 号状态 → 已完成 | 本文交付时 |
| `app/tests/fake_sidecar.py` | 增加 `fs.*` 可脚本化响应（分块/冲突/超限/二进制剧本） | **py-ipc 所有**，随 12 号契约落定后扩展；11 号单测先用测试内鸭子 FakeClient，不依赖此项 |

### 6.3 依赖顺序

1. **硬依赖 12 号契约文本落定**（`fs.*` 形状并入 ipc-protocol.md）→ 在此之前 `fsio.py` 依据 §3.1 需求清单先行开发，对齐期只改 `fsio.py`；
2. **10 号**：Theme 语法令牌 + ThemeBridge + EditorArea 挂载点 + 状态栏绑定（高亮与壳层集成前，编辑器可用 pytest-qt 独立驱动）；
3. **13 号**：CommandRegistry 可用后由 10 号注册 §4.8 清单（此前快捷键不生效，不阻塞编辑器本体开发与测试）；
4. **15 号**：消费 §5 挂点，位于本模块之后；
5. 先行验证（实施第一步）：**S1 技术探针**——最小 QML 页验证 PySide6 当前版本 `TextArea.textDocument → QQuickTextDocument.textDocument()` 取 `QTextDocument`、挂 `QSyntaxHighlighter`、`setPlainText` 后 undo 栈行为（§8 风险 1 的消除动作）。

---

## 7. 测试计划

### 7.1 单元测试（pytest-qt，QCoreApplication，测试内 FakeClient 鸭子类型脚本化 fs 响应）

- **fsio**：分块拼装（多块 + 单块 + 空文件）；BOM/UTF-8/GBK/不可解码四路 fallback 与 `decode_lossy` 只读标记；CRLF/LF/混合/无换行的 EOL 探测；错误体直通；
- **document**：CRLF 文件"打开→不改→保存"字节级 round-trip（含 BOM 还原）；混合行尾保存归一为主导；GBK 写回；`build_save_payload` 的 `\n`→EOL 还原；dirty 随 `modificationChanged`；attach 后撤销栈已清空、两文档撤销互不串扰；
- **service**：同路径（含大小写变体）打开去重 → 激活；`openFile(path, line)` 发出 `gotoLineRequested`；保存冲突全流程（stat 哈希不一致 → `conflictDetected`；overwrite / reload / cancel 三分支；reload 后不脏且撤销栈清空）；写失败 → `saveFailed` 且仍 dirty；三态关闭（save 成功关/save 失败 veto 保留/discard/cancel）；`requestCloseAll` 中途 cancel → `allCloseFinished(False)`；MRU：关闭活动标签继任者 = 次近使用；nextTab/prevTab 循环；preview 替换与转正；`task.manual_edit`（带/不带 files）→ 徽章与信号；任务终态清徽章；`client.disconnected` → 全员只读；oversized/binary/超限错误的打开分支；
- **highlight**：`pick_lexer` 扩展名表 + 兜底 + 未知 → None；token 族回退映射（`Token.String.Doc`→syntaxString）；代际取消（旧 generation 回包被丢弃）；>1MiB 不建引擎；单行超长跳过；行级 diff 只重刷变化块（以 `rehighlightBlock` 调用计数断言）；
- **search**：字面量/大小写/整词/正则四模式命中序列；回绕；`$n` 反向引用替换；replaceAll 为单步撤销（undo 一次全回滚）；matchCount 截断为 `MATCH_COUNT_CAP`；非法正则 → `regexError` 且不抛异常；文档编辑后计数刷新；
- **纪律扫描**（`test_editor_no_local_io.py`）：遍历 `app/halo_studio/editor/*.py` 源文本，断言不出现 `QFile`、`open(`（安全边界回归护栏；测试文件自身豁免）。

### 7.2 集成测试

- `fake_sidecar.py` 扩展 `fs.*` 后（依赖 12 号）：真子进程链路的 打开→编辑→保存 round-trip；冲突剧本（脚本在 stat 与 write 之间改哈希 → 断言进入冲突分支）；分块读写与 1 MiB 行限不超限；`WORKSPACE_NOT_TRUSTED` 拒绝路径的用户可读文案透传。

### 7.3 UI / 冒烟

- `--smoke` 保持通过（EditorArea 并入 Main.qml 后根对象可加载，属 10 号集成验收）；
- pytest-qt + QQuickView 实例化 `EditorPane`：attach 成功、键入后 dirty 徽章出现、Ctrl+F 打开搜索条（`qmlbot` 式按键注入）；
- 手工核查清单（视觉项）：行号与文本行对齐（含缩放字号）、当前行高亮不遮字、查找命中矩形随滚动正确、GBK 文件中文正常显示、8 MiB 文件打开流畅只读。

---

## 8. 风险与缓解

| # | 风险 | 缓解 |
| --- | --- | --- |
| 1 | PySide6 的 `QQuickTextDocument.textDocument()` 取文档/挂 `QSyntaxHighlighter` 的可用性与细节行为（信号时序、setPlainText 触发 modification）随版本有差异 | 实施第一步做 S1 技术探针（§6.3）；attach 全路径判空防御；探针失败的后备方案为 `QQuickItem`+Python 侧 `QTextDocument` 自绘（代价大，仅作决策预案，不预实现） |
| 2 | IPC 单行 1 MiB 上限 vs 8 MiB 文件 | §3.1 已把分块读写列为对 12 号的硬性需求；`FsIo` 拼装/分块提交；若 12 号最终上限低于 8 MiB，编辑器如实按 `openFailed` 提示，`READONLY_MAX_BYTES` 相应下调（单常量） |
| 3 | stat-then-write 竞态（12 号若不提供 `expected_sha256`） | 强烈要求条件写（§3.1）；退化路径下窗口极小且后果 = 覆盖他方写入，冲突弹窗文案如实告知"检查时点"语义；风险记录于此供 12 号评审裁决 |
| 4 | 全文 Pygments lex 在 1 MiB 边界文件上的耗时（数百 ms）导致高亮滞后 | 后台线程 + 代际取消，输入永不阻塞；滞后只表现为颜色晚到；阈值为单常量可下调；行级 diff 限制回刷量 |
| 5 | 大量标签 → 每文档一个 TextArea 实例的内存占用 | 文档内容本身是主要占用（QTextDocument 不可避免）；Pane 壳开销小；状态栏在 >30 标签时提示；远期虚拟化（关闭不活动 Pane 的高亮引擎）已留 `set_enabled` 接口 |
| 6 | `replaceAll`/超多命中覆盖层在极端文件（minified 单行 MB 级）上的卡顿 | 单行超长跳过高亮；`visibleMatches` 只算可见区间；matchCount 截断；replaceAll 在单 edit block 内批量执行（一次布局重算） |
| 7 | GBK fallback 误判（二进制或其他编码恰好可作 GBK 解码） | Sidecar 二进制检测在前（NUL 启发）；GBK 为严格解码非 replace；误判残余风险 = 显示乱码但保存按原字节编码回写、不破坏文件；用户可读提示编码于状态栏 |
| 8 | 混合行尾/重载丢撤销栈等"如实但可能意外"的行为引发用户困惑 | 全部走显式提示（CRLF* 悬停、重载确认文案写明"将丢弃本地修改与撤销历史"）；不做静默行为 |
| 9 | `task.manual_edit` 现契约无文件信息 → 徽章只能全局提示 | 与 15 号对齐 payload 追加式扩展 `files`；编辑器两种形态都已实现（§4.9），契约演进零改动 |
| 10 | Ctrl+Tab 被 QML 焦点链吞掉 | 快捷键统一经 13 号分发（应用级 Shortcut/事件过滤，非控件级），11 号不自装 Shortcut；S1 探针顺带验证 TextArea 对 Tab 键的默认吞噬需 `Keys.onPressed` 放行策略，结论回填 13 号 |

---

## 修订记录

- 2026-07-27：首版。
