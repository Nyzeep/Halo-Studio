# 12 - 文件系统契约与资源管理器

**状态：** 设计完成，待评审
**日期：** 2026-07-27
**依据：** `requirements-alignment/03-ide-editor-and-reference-alignment.md`、`docs/design/README.md` 统一提纲、`docs/ipc-protocol.md`（v1）、R1/R2 参考分析
**本文档职责：** `fs.*` IPC 的**唯一定义者**；资源管理器（Explorer）视图模型与 QML 组件；Python 侧 `FsClient` 薄封装（供 11 号 EditorService 与 13 号快速打开共用）。

---

## 1. 目标与范围

### 1.1 目标

1. 以**追加方式**扩展 IPC v1：新增 `fs.*` 方法族与 `"fs"` capability，使编辑器（11 号）与资源管理器的全部工作区文件访问经 Sidecar 完成——路径牢笼、信任门禁、大小上限、二进制检测、人工编辑归因钩子都收敛在 Sidecar 一处（这是安全边界，UI 侧不得用 `QFile`/`open()` 读写工作区）。
2. 在 `halo-sidecar` 新增 `fs` 模块（牢笼校验、受限文件操作、有界搜索）与 `halo-protocol` 新增 typed DTO。
3. 在 Python 侧提供 `FsClient` 薄封装与资源管理器 `ExplorerViewModel` + `FsTreeModel`（惰性树），QML 侧提供 `ExplorerPanel` 组件（挂载位置由 10 号裁决）。

### 1.2 范围外（明确不做）

- **不提供 `fs.delete`**：首期避免任何破坏性文件操作；删除走系统资源管理器（上下文菜单提供"在系统资源管理器中显示"）。也不做回收站删除。
- **不做全量文件监听**：不引入 `notify` crate 与 `fs.watch` 事件。理由：(a) Windows 上 `ReadDirectoryChangesW` 在大仓库（node_modules 级）事件风暴下需要去抖/合并/溢出恢复一整套机制，复杂度与首期收益不成比例；(b) 资源管理器的刷新需求可用"手动 + 任务终态自动 + 可选定时"三通道覆盖（见 §5.5）；(c) 编辑器外部修改冲突已由 `fs.write` 的 `expected_hash` 乐观锁兜底。**将来方案**：Sidecar 内嵌 `notify` crate（跨平台抽象 ReadDirectoryChangesW/FSEvents/inotify），以追加式事件 `fs.changed`（payload 含去抖合并后的路径批次与 `overflow: true` 溢出标记）推送，UI 收到 overflow 时整树刷新——属 v1 追加式扩展，不破坏本契约。
- 不做资源管理器内拖拽移动、多选批量操作、压缩空链目录（R1 §6.1 的 CompressedNavigation，树模型预留 path 字段即可远期支持）。
- 不做文件内容索引/符号搜索；`fs.search` 只是相对路径 glob + 内容正则的有界扫描。
- 审查视图保持只读；本文不改动 review.*/task.* 既有消息形状（仅对 `task.manual_edit` 事件 payload 做**追加字段**扩展，见 §4.3）。

---

## 2. 参考结论引用

| 来源 | 借鉴 | 不借鉴 |
| --- | --- | --- |
| R1 §6.1 树交互 | 异步数据树按需展开（对应 `fs.list` 惰性调用）；**就地重命名/新建**（行内 InputBox + 实时非法名校验）；单击预览/双击固定打开与编辑器组 preview 语义联动 | 压缩空链目录（首期不做）；键入过滤高亮（归 13 号 picker） |
| R1 §6.2 装饰通道 | `Decoration{letter, colorToken, tooltip, bubble}` 统一装饰模型；bubble 子项徽章上浮祖先；脏点归标签页、Git/基线徽章归资源管理器的正交通道划分 | 多来源 weight 合并框架（首期只有 15 号一个装饰来源） |
| R1 §1.4 | "活动栏条目 → 侧栏视图"注册表驱动：本文只交付 `ExplorerPanel` 组件与面板描述对象，归位由 10 号裁决 | — |
| R2 §5.1 worktree | 懒扫描 + "未加载"入模型（`FsNode.children_loaded` 标志、loading 态行）；UI 永远读模型快照不等 IO | SumTree/scan_id 水位机制（无监听则无增量扫描） |
| R2 §5.3 | 快速打开候选 = Sidecar 一次性全量相对路径清单（本文以 `fs.search` 的 paths-only 模式承载，候选集尊重 gitignore）；匹配打分放 Python 线程池（13 号实现） | — |
| R2 §6 | `fs.read` 大小上限 + 二进制嗅探（git 风格 NUL 检测）；搜索的"懒与断"（候选上限、时间预算、截断标记） | CharBag 预过滤（打分在 13 号 Python 侧做） |
| R2 §4.3 ActionLog | — | **明确不借鉴** hunk 级写回；`fs.write` 只是人工编辑面，与审查只读边界无交集 |

---

## 3. 与现有契约的关系（增量逐条）

对 `docs/ipc-protocol.md`：

1. **第 2 节 `sidecar.hello`**：`capabilities` 数组追加 `"fs"`。capability 表示 Sidecar 支持该方法族，不表示当前可用（未信任工作区下仍上报 `"fs"`，调用时按 §4.2 门禁拒绝）。
2. **第 3 节新增 3.8 文件系统（fs.*）**：8 个方法（list/read/write/create_file/create_dir/rename/stat/search），全文见 §4。
3. **第 4 节事件目录**：`task.manual_edit` payload 追加可选字段 `source`、`path`（既有消费者不受影响）。
4. **第 5 节错误码**：追加 7 个 `FS_` 前缀错误码。
5. `protocol/v1/envelope.schema.json` 错误码枚举同步追加。

对 `docs/module-contracts.md`：

6. 第 1 节 `halo-protocol`：新增 `methods::fs` 模块（typed DTO）。
7. 第 6 节 `halo-sidecar`：新增 `src/fs/` 模块（cage/ops/search）；`GitClient` 追加只读方法 `ls_candidate_files()`；dispatch 追加 fs 路由与 `"fs"` capability。
8. 第 8 节 app：新增 `ipc/fs_client.py`、`viewmodels/explorer_viewmodel.py`、`qml/panels/ExplorerPanel.qml` 等（详见 §7）。
9. 全部为**追加式**：不改任何既有消息形状、方法语义、crate 依赖关系（`halo-sidecar` 仍是唯一聚合者；六个业务 crate 之间保持零依赖——fs 模块整体落在 `halo-sidecar` 内，不新建 crate）。

---

## 4. 契约增量全文（可直接并入 docs/ipc-protocol.md）

> 以下内容按 `docs/ipc-protocol.md` 的现行格式书写；评审通过后**原样并入**该文档（3.8 节插入第 3 节末尾；4.3 小节替换事件表对应行；4.4 小节追加到第 5 节错误码清单；capabilities 修改第 2 节示例值）。

### 4.1 第 2 节修改：capabilities

`sidecar.hello` 的 result 示例更新为：

`{"protocol_version": 1, "sidecar_version": "0.1.0", "capabilities": ["workspace","config","pi","opencode","task","review","handoff","history","fs"]}`

### 4.2 新增 3.8 文件系统（fs.*）

以下为并入正文：

---

### 3.8 文件系统（fs.*）

编辑器与资源管理器的**唯一**工作区文件访问通道。UI 进程不得绕过本节直接读写工作区文件。

**通用前置条件（全部 fs.* 方法）**：

- 必须存在活动工作区，否则 `WORKSPACE_NOT_ACTIVE`；
- 工作区必须已确认信任，否则 `WORKSPACE_NOT_TRUSTED`（未信任工作区**一律拒绝** fs.*，包括只读方法；比 config.* 的"未信任可读"更严格——文件内容本身就是需要信任门禁保护的对象）。

**路径规则（牢笼）**：

- 一切路径为**工作区相对路径**（相对 `WorkspaceStatus.real_path`），协议内统一使用 `/` 分隔符；Sidecar 接受 `\` 输入但输出一律归一化为 `/`。`""` 与 `"."` 均表示工作区根。
- 拒绝：绝对路径（含盘符、UNC、`\\?\`）、任何 `..` 路径组件、NUL 字节 → `FS_PATH_OUTSIDE_WORKSPACE`。
- canonicalize 后必须仍在工作区根内（对读类操作 canonicalize 目标本身；对写类操作 canonicalize 现存的父目录），符号链接/junction 指向根外即拒绝 → `FS_PATH_OUTSIDE_WORKSPACE`。
- Windows 保留设备名（`CON`、`PRN`、`AUX`、`NUL`、`COM1-9`、`LPT1-9`）与以空格/点结尾的名字在创建/重命名时拒绝 → `INVALID_PARAMS`。
- **`.git` 保护**：`fs.list` 输出中直接省略工作区根下的 `.git` 条目；`fs.write` / `fs.create_file` / `fs.create_dir` / `fs.rename`（from 或 to）落在 `.git` 内一律 `FS_GIT_PROTECTED`（路径比较在 canonicalize 后进行，Windows 下不区分大小写）。

| 方法 | params | result | 说明 |
| --- | --- | --- | --- |
| `fs.list` | `{"path": "src", "depth": 1}` | `FsListResult` | `depth` 可省略（默认 1，仅列一层）；`1..=8`；递归结果按父先子后的先序排列。条目总数上限 10000，超限截断并 `truncated: true`。目录在前、文件在后，同类按名称不区分大小写排序。symlink 条目 `kind: "symlink"`，**不下钻**。 |
| `fs.read` | `{"path": "src/a.rs"}` | `FsReadResult` | 文件 > 8 MiB → `FS_TOO_LARGE`（details 带 `size`）；前 8 KiB 含 NUL 字节判为二进制 → `FS_BINARY`（details 带 `size`，UI 呈现"二进制文件"占位，不进编辑器）；编码探测见下。 |
| `fs.write` | `{"path": "src/a.rs", "content": "…", "expected_hash": "sha256:…", "encoding": "utf-8"}` | `FsWriteResult` | 乐观锁：`expected_hash` **必填**，与磁盘当前内容哈希不一致 → `FS_CONFLICT`（details 带 `current_hash`、`mtime`）；覆盖流程 = UI 重新 `fs.read` 取新哈希并向用户展示差异后重写。文件不存在 → `FS_NOT_FOUND`（新文件走 `fs.create_file`）。content 编码后 > 8 MiB → `FS_TOO_LARGE`。写入为原子操作（同目录临时文件 + rename）。`encoding` 可省略（默认 `"utf-8"`），取值限 `"utf-8" \| "utf-8-bom" \| "utf-16le" \| "utf-16be"`（回写 `fs.read` 探测到的原编码；`"unknown"` 不可写，见下）。**归因钩子触发点**：写入成功且当前存在非终态任务时，Sidecar 触发人工编辑归因（语义同 `task.mark_manual_edit`：归因转 mixed + `task.manual_edit` 事件，`source: "fs_write"`；去抖与文案细则见 15 号设计文档）。 |
| `fs.create_file` | `{"path": "src/new.rs", "content": ""}` | `{"entry": FsEntry}` | 目标已存在 → `FS_ALREADY_EXISTS`；父目录不存在 → `FS_NOT_FOUND`。`content` 可省略（默认空，UTF-8）。成功且存在非终态任务 → 触发归因钩子（同上）。 |
| `fs.create_dir` | `{"path": "src/util"}` | `{"entry": FsEntry}` | 单层创建，父目录不存在 → `FS_NOT_FOUND`；已存在 → `FS_ALREADY_EXISTS`。成功触发归因钩子（同上）。 |
| `fs.rename` | `{"from": "src/a.rs", "to": "src/b.rs"}` | `{"entry": FsEntry}` | from 不存在 → `FS_NOT_FOUND`；to 已存在 → `FS_ALREADY_EXISTS`；from/to 均须过牢笼校验。to 可位于其他目录（即支持移动）。成功触发归因钩子（同上）。 |
| `fs.stat` | `{"path": "src/a.rs"}` | `{"entry": FsEntry}` | 不存在 → `FS_NOT_FOUND`。 |
| `fs.search` | `{"glob": "**/*.rs", "query": "fn\\s+main", "case_sensitive": false, "max_results": 500}` | `FsSearchResult` | 候选集 = `git ls-files --cached --others --exclude-standard`（含未跟踪未忽略文件；天然不含 `.git` 与被忽略文件）。`glob` 可省略（null = 全部候选）；`query` 为正则（Rust `regex` 语法），**可省略**——省略时为"仅按 glob 返回路径"模式（items 只含 `path`，供快速打开等场景取全量路径清单）。`max_results` 可省略（默认 500，上限 20000）。内容模式跳过 > 8 MiB 与二进制嗅探命中的文件；单文件命中上限 100 条；总扫描时间预算 5 s。达到任一上限 → `truncated: true`。非法正则 → `INVALID_PARAMS`。 |

`FsEntry`：

```jsonc
{
  "name": "a.rs",
  "path": "src/a.rs",              // 工作区相对路径，/ 分隔
  "kind": "file" | "dir" | "symlink",
  "size": 1234,                     // 目录与 symlink 为 0
  "mtime": "2026-07-27T08:00:00Z",
  "readonly": false                 // Windows 只读属性
}
```

`FsListResult`：

```jsonc
{
  "path": "src",
  "entries": [FsEntry, …],
  "truncated": false
}
```

`FsReadResult`：

```jsonc
{
  "path": "src/a.rs",
  "content": "…",                          // 已解码为 UTF-8 字符串；BOM 已剥离
  "encoding": "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be" | "unknown",
  "lossy": false,                           // encoding=unknown 时 true：内容为 UTF-8 lossy 解码，
                                            // UI 必须以只读方式打开，禁止 fs.write 回写（防破坏原文件）
  "line_ending": "lf" | "crlf" | "mixed" | "none",
  "hash": "sha256:<64位hex>",              // 对磁盘原始字节计算，作 fs.write 乐观锁
  "size": 1234,
  "mtime": "2026-07-27T08:00:00Z",
  "readonly": false
}
```

编码探测顺序：UTF-8 BOM → UTF-16 LE/BE BOM → 严格 UTF-8 校验 → 全部失败且前 8 KiB 含 NUL 则 `FS_BINARY`，否则 `encoding: "unknown"` + lossy 解码（典型为 GBK 等本地编码文件，只读打开，不误报为二进制）。

`FsWriteResult`：

```jsonc
{
  "path": "src/a.rs",
  "hash": "sha256:<新内容哈希>",           // 作为编辑器下一次保存的 expected_hash
  "size": 1240,
  "mtime": "2026-07-27T08:01:00Z"
}
```

`FsSearchResult`：

```jsonc
{
  "items": [
    {"path": "src/main.rs", "line": 12, "column": 5, "preview": "fn main() {", "preview_truncated": false}
    // query 省略（paths-only 模式）时 item 仅含 path 字段
  ],
  "truncated": false,
  "scanned_files": 1234
}
```

约束：

- `preview` 为命中行文本（UTF-8，单行 ≤ 512 字节，超长截断并 `preview_truncated: true`）；`line`/`column` 均从 1 起。
- fs.* 全部方法不产生事件（归因钩子触发的 `task.manual_edit` 除外）；资源管理器刷新由 UI 侧策略驱动。
- fs.read / fs.stat 允许读取 `.git` 内路径（只读观察，与 GitClient 一致）；仅写类操作受 `FS_GIT_PROTECTED` 保护。

---

### 4.3 第 4 节事件目录修改：task.manual_edit

原行：

| event | payload | 说明 |
| --- | --- | --- |
| `task.manual_edit` | `{"note": "…"}` | 人工介入被标记 |

替换为（**追加可选字段**，既有消费者不受影响）：

| event | payload | 说明 |
| --- | --- | --- |
| `task.manual_edit` | `{"note": "…", "source": "user_marked" \| "fs_write", "path": "src/a.rs" \| null}` | 人工介入被标记；`source: "fs_write"` 表示由 fs 写类方法在运行中任务期间自动触发（15 号差异化"人工介入自动归因"），`path` 为触发写入的工作区相对路径；`task.mark_manual_edit` 手动标记时 `source: "user_marked"`、`path: null` |

### 4.4 第 5 节错误码清单追加

在既有清单末尾追加一行：

`FS_PATH_OUTSIDE_WORKSPACE` · `FS_TOO_LARGE` · `FS_BINARY` · `FS_CONFLICT` · `FS_NOT_FOUND` · `FS_ALREADY_EXISTS` · `FS_GIT_PROTECTED`

语义：

| 错误码 | 含义 | details |
| --- | --- | --- |
| `FS_PATH_OUTSIDE_WORKSPACE` | 路径为绝对路径、含 `..`、或 canonicalize 后逃逸工作区根（含符号链接/junction 逃逸） | `{"path": "…"}` |
| `FS_TOO_LARGE` | 读/写内容超过 8 MiB 上限 | `{"size": n, "max": 8388608}` |
| `FS_BINARY` | 二进制文件不支持读入编辑器 | `{"size": n}` |
| `FS_CONFLICT` | `expected_hash` 与磁盘当前内容不一致（外部或 Agent 已修改） | `{"current_hash": "sha256:…", "mtime": "…"}` |
| `FS_NOT_FOUND` | 目标（或写类操作的父目录 / rename 的 from）不存在 | `{"path": "…"}` |
| `FS_ALREADY_EXISTS` | create/rename 目标已存在 | `{"path": "…"}` |
| `FS_GIT_PROTECTED` | 写类操作落在 `.git` 目录内 | `{"path": "…"}` |

---

## 5. 详细设计

### 5.1 halo-protocol：`src/methods/fs.rs`（新增）

与既有 methods 模块同风格：typed struct、`serde(rename_all = "snake_case")`、字段与 §4 一字不差。

```rust
// methods/fs.rs（结构体清单；serde 派生与文档注释省略）
pub struct FsListParams { pub path: String, #[serde(default = "default_depth")] pub depth: u32 }
pub struct FsEntry { pub name: String, pub path: String, pub kind: FsEntryKind, pub size: u64, pub mtime: String, pub readonly: bool }
pub enum FsEntryKind { File, Dir, Symlink }                 // 小写蛇形序列化
pub struct FsListResult { pub path: String, pub entries: Vec<FsEntry>, pub truncated: bool }

pub struct FsReadParams { pub path: String }
pub enum FsEncoding { Utf8, Utf8Bom, Utf16le, Utf16be, Unknown }   // "utf-8"/"utf-8-bom"/… 用 serde(rename)
pub enum FsLineEnding { Lf, Crlf, Mixed, None }
pub struct FsReadResult { pub path: String, pub content: String, pub encoding: FsEncoding, pub lossy: bool,
                          pub line_ending: FsLineEnding, pub hash: String, pub size: u64, pub mtime: String, pub readonly: bool }

pub struct FsWriteParams { pub path: String, pub content: String, pub expected_hash: String,
                           #[serde(default)] pub encoding: FsWriteEncoding }   // Default = Utf8；不含 Unknown
pub enum FsWriteEncoding { Utf8, Utf8Bom, Utf16le, Utf16be }
pub struct FsWriteResult { pub path: String, pub hash: String, pub size: u64, pub mtime: String }

pub struct FsCreateFileParams { pub path: String, #[serde(default)] pub content: String }
pub struct FsCreateDirParams { pub path: String }
pub struct FsRenameParams { pub from: String, pub to: String }
pub struct FsStatParams { pub path: String }
pub struct FsEntryResult { pub entry: FsEntry }

pub struct FsSearchParams { pub glob: Option<String>, pub query: Option<String>,
                            #[serde(default)] pub case_sensitive: bool,
                            #[serde(default = "default_max_results")] pub max_results: u32 }
pub struct FsSearchItem { pub path: String,
                          #[serde(skip_serializing_if = "Option::is_none")] pub line: Option<u32>,
                          #[serde(skip_serializing_if = "Option::is_none")] pub column: Option<u32>,
                          #[serde(skip_serializing_if = "Option::is_none")] pub preview: Option<String>,
                          #[serde(skip_serializing_if = "Option::is_none")] pub preview_truncated: Option<bool> }
pub struct FsSearchResult { pub items: Vec<FsSearchItem>, pub truncated: bool, pub scanned_files: u64 }
```

`error.rs` 的 `ErrorCode` 追加变体（serde 自动输出 SCREAMING_SNAKE_CASE）：
`FsPathOutsideWorkspace, FsTooLarge, FsBinary, FsConflict, FsNotFound, FsAlreadyExists, FsGitProtected`。
`protocol/v1/envelope.schema.json` 的错误码枚举同步追加；`halo-protocol/tests/contract.rs` 的错误码稳定性快照更新。

### 5.2 halo-sidecar：新增 `src/fs/` 模块

```
sidecar/crates/halo-sidecar/src/fs/mod.rs      — FsError、limits 常量、From<FsError> for SidecarError、handler 入口
sidecar/crates/halo-sidecar/src/fs/cage.rs     — 路径牢笼（纯函数，重点单测对象）
sidecar/crates/halo-sidecar/src/fs/ops.rs      — list/read/write/create/rename/stat
sidecar/crates/halo-sidecar/src/fs/search.rs   — 有界并行搜索
```

**常量**（`fs/mod.rs`）：

```rust
pub mod limits {
    pub const FS_READ_MAX_BYTES: u64 = 8 * 1024 * 1024;
    pub const FS_WRITE_MAX_BYTES: u64 = 8 * 1024 * 1024;
    pub const FS_BINARY_SNIFF_BYTES: usize = 8 * 1024;
    pub const FS_LIST_MAX_ENTRIES: usize = 10_000;
    pub const FS_LIST_MAX_DEPTH: u32 = 8;
    pub const FS_SEARCH_DEFAULT_RESULTS: u32 = 500;
    pub const FS_SEARCH_MAX_RESULTS: u32 = 20_000;
    pub const FS_SEARCH_FILE_MAX_MATCHES: usize = 100;
    pub const FS_SEARCH_FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;
    pub const FS_SEARCH_TIME_BUDGET_MS: u64 = 5_000;
    pub const FS_PREVIEW_MAX_BYTES: usize = 512;
}
```

**错误类型与映射**：

```rust
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("路径超出工作区范围：{0}")]            OutsideWorkspace(String),
    #[error("路径不存在：{0}")]                    NotFound(String),
    #[error("目标已存在：{0}")]                    AlreadyExists(String),
    #[error("文件大小 {size} 字节超过上限")]        TooLarge { size: u64 },
    #[error("二进制文件不支持读入编辑器")]          Binary { size: u64 },
    #[error("文件内容已被外部修改")]                Conflict { current_hash: String, mtime: String },
    #[error(".git 目录受只读保护：{0}")]           GitProtected(String),
    #[error("非法文件名：{0}")]                    InvalidName(String),   // → INVALID_PARAMS
    #[error("文件系统操作失败：{0}")]              Io(String),            // → INTERNAL
}
// dispatch.rs: impl From<FsError> for SidecarError（含 details 组装，与 §4.4 表一致）
```

**牢笼校验（`cage.rs`，纯函数）**：

```rust
/// 校验并解析读类目标（list/read/stat）：目标必须存在。
pub fn resolve_existing(root: &Path, rel: &str) -> Result<PathBuf, FsError>;

/// 校验并解析写类目标（write/create/rename 的 to）：
/// 父目录必须存在并通过牢笼校验；返回绝对路径与"目标当前是否存在"。
pub struct ResolvedTarget { pub abs: PathBuf, pub exists: bool }
pub fn resolve_target(root: &Path, rel: &str) -> Result<ResolvedTarget, FsError>;

/// 语法预检（两个 resolve 的第一步）：拒绝绝对路径/盘符/UNC/verbatim、任何 ".." 组件、
/// NUL；创建/重命名场景额外拒绝 Windows 保留设备名与结尾空格/点（InvalidName）。
fn precheck_syntax(rel: &str, creating: bool) -> Result<(), FsError>;

/// canonicalize 前缀校验：canonicalize(existing_path) 必须以 canonicalize(root) 为前缀
/// （大小写不敏感比较用 Path 组件级比较，先经 strip_verbatim；封死符号链接/junction 逃逸）。
fn ensure_within_root(root_canonical: &Path, candidate_canonical: &Path) -> Result<(), FsError>;

/// .git 保护：canonical 相对路径首组件 == ".git"（不区分大小写）→ GitProtected。
pub fn ensure_not_git_protected(root_canonical: &Path, abs: &Path) -> Result<(), FsError>;

/// 输出归一化：绝对路径 → 工作区相对 + `/` 分隔（协议线格式）。
pub fn to_wire_rel(root_canonical: &Path, abs: &Path) -> String;
```

要点：写类操作对**父目录** canonicalize（目标可能尚不存在）；读类操作对目标本身 canonicalize。`\\?\` 前缀复用 `git.rs::strip_verbatim` 的思路（抽为共享 helper 或复制到 cage.rs，不引入模块耦合）。

**操作（`ops.rs`）**：

```rust
pub fn list(root: &Path, rel: &str, depth: u32) -> Result<FsListResult, FsError>;
pub fn read(root: &Path, rel: &str) -> Result<FsReadResult, FsError>;
pub fn write(root: &Path, rel: &str, content: &str, expected_hash: &str, enc: FsWriteEncoding) -> Result<FsWriteResult, FsError>;
pub fn create_file(root: &Path, rel: &str, content: &str) -> Result<FsEntry, FsError>;
pub fn create_dir(root: &Path, rel: &str) -> Result<FsEntry, FsError>;
pub fn rename(root: &Path, from: &str, to: &str) -> Result<FsEntry, FsError>;
pub fn stat(root: &Path, rel: &str) -> Result<FsEntry, FsError>;
```

- `read`：先 `metadata` 判大小（> 8 MiB 即拒，不读内容）；读入后前 8 KiB NUL 嗅探；编码探测按 §4.2 顺序；哈希 = `sha2::Sha256` 对**原始字节**（复用 workspace 已有 `sha2` 依赖——配置事务已用）。
- `write`：读当前文件字节 → 哈希对比（不一致 → `Conflict`）→ 按 `enc` 编码 content（含 BOM 重建）→ 同目录 `halo-fs-tmp-<uuid>` 临时文件写入 → `rename` 原子替换（与 `ConfigTransaction` 同手法，但**不做备份**——乐观锁已防覆盖他人修改，编辑器自身有撤销）。
- `list`：`read_dir` 逐层；`depth` 递归用显式栈（防深目录爆栈）；根层跳过 `.git`；条目计数达 `FS_LIST_MAX_ENTRIES` 即停止并 `truncated: true`；symlink 不下钻。
- mtime 统一转 UTC RFC3339（`time::OffsetDateTime`，与协议一致）。

**搜索（`search.rs`）**：

```rust
pub fn search(root: &Path, git: &GitClient, p: &FsSearchParams) -> Result<FsSearchResult, FsError>;
```

- 候选枚举：`GitClient::ls_candidate_files()`（见 §5.3）；`glob` 过滤用 `globset` crate（workspace 新增依赖，MIT；ripgrep 家族，成熟稳定）。
- 内容匹配：`regex` crate（workspace 已有）编译 `query`（`case_sensitive: false` 时加 `(?i)`）；**不用 `git grep`**——理由：契约需要稳定的正则方言（git grep 的 POSIX/PCRE 取决于构建配置，行为不可锁定），且自实现才能精确执行大小上限、二进制跳过、单文件命中上限与时间预算。git grep 留作远期优化选项。
- 并行：`std::thread::scope` + `available_parallelism()` 个工作线程分片扫描，`AtomicBool` 预算/上限触发即协同退出；结果按候选清单原始顺序稳定排序后返回。
- paths-only 模式（`query: None`）：只走枚举 + glob 过滤，不打开文件——这就是 13 号快速打开的候选清单来源（R2 §5.3）。

**dispatch 接线（`dispatch.rs` 修改）**：

```rust
// CAPABILITIES 追加 "fs"
const CAPABILITIES: &[&str] = &["workspace","config","pi","opencode","task","review","handoff","history","fs"];

// handle() 的 match 追加：
"fs.list" => self.fs_list(params),
"fs.read" => self.fs_read(params),
"fs.write" => self.fs_write(params),
"fs.create_file" => self.fs_create_file(params),
"fs.create_dir" => self.fs_create_dir(params),
"fs.rename" => self.fs_rename(params),
"fs.stat" => self.fs_stat(params),
"fs.search" => self.fs_search(params),

// 共用前置：取根 + 信任门禁（全部 fs handler 第一行调用）
fn require_trusted_root(&self) -> Result<PathBuf, SidecarError> {
    let app = lock(&self.ctx.app);
    let ws = app.workspace.as_ref()
        .ok_or_else(|| SidecarError::new(ErrorCode::WorkspaceNotActive, "没有活动工作区，无法访问文件"))?;
    if !ws.is_trusted() {
        return Err(SidecarError::new(ErrorCode::WorkspaceNotTrusted, "工作区未确认信任，文件访问已拒绝"));
    }
    Ok(PathBuf::from(&ws.real_path))
}
```

**牢笼根 = `WorkspaceStatus.real_path`**（用户显式打开并信任的目录），不是 `git_root`。二者通常相同；当用户打开仓库子目录时，基线 diff 的路径（相对 `git_root`）与 fs 路径（相对 `real_path`）存在前缀差，换算责任在消费方（15 号徽章数据源做剥前缀，见 §6）。

**归因钩子（触发点定义，语义归 15 号）**：

```rust
/// fs.write / fs.create_file / fs.create_dir / fs.rename 成功后调用。
/// 存在非终态任务时：task.attribution.with_manual_edit(...) + store.put_task
/// + bus.emit(task.manual_edit, {"note": …, "source": "fs_write", "path": rel})。
/// note 默认文案："经内嵌编辑器写入：<path>"；去抖/汇总策略（同一文件连续保存
/// 是否合并 reason）由 15 号设计文档裁决，本处仅保证每次成功写入都到达该函数。
fn maybe_mark_manual_edit(&self, rel_path: &str)
```

实现上与既有 `task_mark_manual_edit` 共享内部路径：把 dispatch.rs 中"归因转 mixed + put_task + emit"三步抽为私有 fn `mark_manual_edit_internal(&self, note: &str, source: &str, path: Option<&str>)`，两个入口共用，避免语义漂移。

### 5.3 GitClient 增量（`git.rs`）

```rust
/// 搜索/快速打开候选清单：git ls-files --cached --others --exclude-standard -z。
/// 只读命令，符合"绝不执行修改性 git 命令"红线；输出为仓库相对路径（/ 分隔）。
/// 注意：输出相对 git_root；调用方（fs::search）在 real_path != git_root 时
/// 负责换算为工作区相对路径并过滤出 real_path 子树内的条目。
pub fn ls_candidate_files(&self) -> Result<Vec<String>, GitError>;
```

### 5.4 Python：`app/halo_studio/ipc/fs_client.py`（新增）

薄封装，无业务逻辑；供 `EditorService`（11 号）、`ExplorerViewModel`（本文）、快速打开（13 号）三方共用。遵循 ipc 层既有分工：请求经 `client.py` 的 Qt 包装发出，结果回调已在主线程。

```python
@dataclass(frozen=True)
class FsEntry:
    name: str; path: str; kind: str; size: int; mtime: str; readonly: bool

@dataclass(frozen=True)
class FsReadResult:
    path: str; content: str; encoding: str; lossy: bool
    line_ending: str; hash: str; size: int; mtime: str; readonly: bool

@dataclass(frozen=True)
class FsWriteResult:
    path: str; hash: str; size: int; mtime: str

@dataclass(frozen=True)
class FsSearchItem:
    path: str; line: int | None = None; column: int | None = None
    preview: str | None = None; preview_truncated: bool | None = None

class FsError(Exception):
    """携带契约错误码；调用方按 code 分支（如 FS_CONFLICT → 冲突对话框）。"""
    def __init__(self, code: str, message: str, details: dict): ...

class FsClient(QObject):
    """fs.* 契约的 1:1 薄封装。每个方法返回 concurrent Future（沿用 connection.request
    的 Future 语义）；另提供 *_async 变体接收 (on_ok, on_error) 主线程回调，
    QML 侧一律经 ViewModel 使用，不直接触达本类。"""
    def __init__(self, client: IpcClient): ...
    def list(self, path: str = "", depth: int = 1) -> Future: ...          # -> (list[FsEntry], truncated)
    def read(self, path: str) -> Future: ...                                # -> FsReadResult
    def write(self, path: str, content: str, expected_hash: str,
              encoding: str = "utf-8") -> Future: ...                       # -> FsWriteResult
    def create_file(self, path: str, content: str = "") -> Future: ...      # -> FsEntry
    def create_dir(self, path: str) -> Future: ...                          # -> FsEntry
    def rename(self, from_path: str, to_path: str) -> Future: ...           # -> FsEntry
    def stat(self, path: str) -> Future: ...                                # -> FsEntry
    def search(self, glob: str | None = None, query: str | None = None,
               case_sensitive: bool = False, max_results: int = 500) -> Future: ...  # -> (items, truncated, scanned_files)
```

约束：`FsClient` 不缓存、不重试、不做路径拼接以外的任何加工；错误一律以 `FsError(code, …)` 透传契约错误码。`lossy: True` 的读结果由 EditorService 负责强制只读（11 号契约义务，本文在 docstring 与测试中固化）。

### 5.5 Python：`app/halo_studio/viewmodels/explorer_viewmodel.py`（新增）

采用**扁平列表 + 层级**方案（而非 QAbstractItemModel 树）：QML `ListView` 渲染扁平行、`level` 属性控制缩进——比 `TreeView`+QAbstractItemModel 的实现/测试成本低一个量级，且天然契合"惰性展开插入行"的模式（R2 §5.1 懒扫描思想）。

```python
class FsNode:
    """内部树节点缓存（非 Qt 类型）。"""
    path: str; name: str; kind: str
    expanded: bool = False
    children_loaded: bool = False     # 未加载 ≠ 无子项（R2：'未加载'入模型）
    loading: bool = False
    children: list["FsNode"]

class FsTreeModel(QAbstractListModel):
    """把 FsNode 树投影为可见行的扁平列表。roles:
    name, relPath, kind, level, expanded, loading, isEditing,
    badgeLetter, badgeColorToken, badgeTooltip   # 装饰通道（15 号数据源）
    """
    def visible_rows(self) -> list[FsNode]: ...
    def apply_listing(self, dir_path: str, entries: list[FsEntry], truncated: bool): ...
        # 与现有子节点按 name 做增量 diff：begin/endInsertRows / begin/endRemoveRows /
        # dataChanged 最小更新，保持展开状态与选中稳定（刷新不塌树）
    def set_decorations(self, decorations: dict[str, Decoration]): ...   # path → Decoration
        # Decoration = dataclass(letter: str, color_token: str, tooltip: str, bubble: bool)
        # bubble=True 的装饰自动上浮聚合到未展开的祖先目录行（R1 §6.2）

class ExplorerViewModel(QObject):
    """QML 门面。依赖注入：FsClient、EditorService（11 号）、IpcClient（订阅事件）。"""
    model: FsTreeModel                      # Property(QObject, constant=True)
    workspaceActive: bool                   # Property + notify
    workspaceTrusted: bool                  # Property + notify（未信任 → 面板显示门禁占位，不发任何 fs 请求）
    autoRefreshEnabled: bool                # Property，默认 False
    autoRefreshIntervalMs: int              # Property，默认 30000
    errorOccurred = Signal(str, str)        # (code, 中文 message) → QML toast

    @Slot(str)            def expand(self, rel_path): ...        # 未加载 → fs.list(path, 1)，行置 loading
    @Slot(str)            def collapse(self, rel_path): ...
    @Slot()               def refresh(self): ...                 # 重列根 + 全部已展开目录（并发 fs.list，结果 apply_listing）
    @Slot(str, str)       def createFile(self, parent_dir, name): ...   # 就地新建行提交 → fs.create_file → 插入并 openInEditor
    @Slot(str, str)       def createDir(self, parent_dir, name): ...
    @Slot(str, str)       def rename(self, rel_path, new_name): ...     # 就地重命名提交 → fs.rename
    @Slot(str)            def openPreview(self, rel_path): ...   # 单击 → EditorService.openFile(path)（preview 语义归 11 号）
    @Slot(str)            def openPinned(self, rel_path): ...    # 双击/回车 → 固定打开
    @Slot(str)            def revealInSystem(self, rel_path): ...# QDesktopServices.openUrl(父目录)；UI 级只读动作，不经 fs.*
    @Slot(str, result=str) def validateName(self, name): ...     # 就地编辑实时校验：空/含 \/ 保留名/结尾空格点 → 中文错误文案；合法返回 ""
```

**刷新策略（三通道，无文件监听的替代方案）**：

1. **手动**：面板头部刷新按钮 / 命令 `explorer.refresh`；
2. **任务终态自动**：订阅 `task.state` 事件，任务进入终态（`review_ready`/`cancelled`/`failed`/`interrupted`）时自动 `refresh()`——Agent 改完文件后视图立即跟上，这是最重要的一致性时机；同时订阅 `workspace.changed`（切换/信任变化 → 整树重建或清空）；
3. **可选定时**：`QTimer`，默认关闭；开启后仅在窗口激活时触发（`QGuiApplication.applicationState` 判断），避免后台空转。

**命令注册**（经 13 号 `CommandRegistry`，10 号负责挂载时机）：`explorer.refresh`（刷新资源管理器）、`explorer.newFile`、`explorer.newFolder`、`explorer.rename`、`explorer.revealInSystem`、`explorer.collapseAll`。

### 5.6 QML：`app/halo_studio/qml/panels/ExplorerPanel.qml` + `ExplorerRow.qml`（新增）

组件树（颜色/字体/间距一律引用 10 号 `Theme` 单例 token，不出现裸值）：

```
ExplorerPanel.qml                        // required property var explorer (ExplorerViewModel)
├── ColumnLayout
│   ├── RowLayout  头部
│   │   ├── Label        标题（工作区目录名）
│   │   ├── Item         弹性占位
│   │   ├── ToolButton   新建文件   → explorer 进入"就地新建"态（选中目录下插入编辑行）
│   │   ├── ToolButton   新建文件夹
│   │   ├── ToolButton   刷新       → explorer.refresh()
│   │   └── ToolButton   全部折叠
│   ├── ListView  树主体（clip: true; ScrollBar.vertical）
│   │   ├── model: explorer.model
│   │   └── delegate: ExplorerRow.qml
│   └── 占位态（三选一，anchors.fill 树区域）
│       ├── 未打开工作区：引导文案
│       ├── 未信任：门禁说明 +"前往信任"入口（跳工作区视图）
│       └── 空目录：提示文案
└── Menu  上下文菜单（右键行时弹出）
    ├── MenuItem 在编辑器中打开        → explorer.openPinned(path)
    ├── MenuItem 新建文件 / 新建文件夹  （仅目录行）
    ├── MenuItem 重命名                → 行进入就地编辑态
    ├── MenuItem 复制相对路径          → 剪贴板
    ├── MenuItem 加入任务上下文        → 15 号命令挂点（command id: task.addFileToContext）
    └── MenuItem 在系统资源管理器中显示 → explorer.revealInSystem(path)（删除等破坏性操作走系统）

ExplorerRow.qml                          // ListView delegate
├── MouseArea（单击 → openPreview；双击 → openPinned；右键 → 菜单；悬停 → Theme.listHoverBackground）
├── RowLayout
│   ├── Item          缩进（width: level * Theme.treeIndentWidth）
│   ├── Image/文本箭头 展开箭头（kind==="dir"；loading 时换 BusyIndicator 小态）
│   ├── Image         类型图标（dir / file / symlink）
│   ├── Label         名称（isEditing 时替换为 TextField）
│   │   └── TextField 就地编辑（新建/重命名共用；onTextChanged → explorer.validateName 实时校验，
│   │                  非法时行下浮出错误 ToolTip；回车提交 / Esc 取消 —— R1 §6.1 就地编辑）
│   ├── Item          弹性占位
│   └── Label         徽章（badgeLetter；color: Theme[badgeColorToken]；ToolTip=badgeTooltip）
│                     ← 基线变更徽章挂点：本文件只渲染 role，数据源 15 号
```

选中/焦点/键盘（上下移动、左右折叠展开、回车打开、F2 重命名）走 `ListView.currentIndex` + `Keys.onPressed`，焦点上下文键 `explorerFocus` 上报给 13 号上下文服务。

面板描述对象（供 10 号壳层注册，最终归位以 10 号为准）：

```javascript
{ id: "explorer", title: "资源管理器", icon: "explorer", order: 0,
  source: "panels/ExplorerPanel.qml", position: "sidebar" }
```

### 5.7 线程与数据流

```
QML ExplorerPanel ──(Slot 调用)──▶ ExplorerViewModel ──▶ FsClient ──▶ IpcClient(Qt 信号桥) ──▶ connection(写线程)
        ▲                              │                                        │
        └──(model roles / 信号)────────┘◀──(主线程回调 apply_listing 等)◀────────┘◀── Sidecar 响应
Sidecar 事件流：task.state 终态 / workspace.changed ──▶ ExplorerViewModel 刷新策略
              task.manual_edit(source=fs_write) ──▶ 15 号消费（本文不处理）
```

- Explorer 全部逻辑在主线程（IO 都在 Sidecar 进程内）；单次 `fs.list` ≤ 10000 条的 `apply_listing` 为 O(n) diff，主线程可承受；
- Sidecar 侧 dispatch 为单线程串行处理请求：`fs.search` 内部并行扫描 + 5 s 预算上限，确保不长时间独占请求通道（风险与远期方案见 §9）；
- `fs.read/write` 的调用方是 EditorService（11 号），本文只保证 `FsClient` 语义；`FS_CONFLICT` 的冲突对话框、`lossy` 只读、保存失败拦截关闭（R1 §2.3 veto 流程）均为 11 号职责。

---

## 6. 差异化点（挂点，裁决权在 15 号）

本文只提供机制与挂点，不实现差异化语义：

1. **人工介入自动归因**：触发点已在契约锁定（§4.2 fs.write 等写类方法成功 + 非终态任务 → `mark_manual_edit_internal`，事件带 `source: "fs_write"`、`path`）。去抖/汇总/文案细则由 15 号裁决；本文保证每次成功写入必达钩子函数。
2. **基线感知徽章**：`FsTreeModel.set_decorations(dict[path, Decoration])` 是唯一挂点；数据源（基于 `review.get` 文件清单或基线 diff 事件计算"任务基线以来已变更"集合）由 15 号实现。注意路径换算：证据文件路径相对 `git_root`，装饰键须换算为相对 `real_path`（15 号剥前缀；`real_path == git_root` 时为恒等）。`bubble: true` 上浮聚合由 `FsTreeModel` 实现（本文交付）。
3. **任务上下文选择器**：上下文菜单预留"加入任务上下文"项，绑定命令 id `task.addFileToContext`（携带 relPath 参数）；命令实现归 15 号。
4. **审查→编辑器跳转 / 快速打开**：均为 `FsClient` 消费方（11/13 号），本文交付共用客户端即可。

---

## 7. 实施计划

### 7.1 文件清单

**新建：**

| 路径 | 内容 |
| --- | --- |
| `sidecar/crates/halo-protocol/src/methods/fs.rs` | §5.1 全部 DTO |
| `sidecar/crates/halo-sidecar/src/fs/mod.rs` | FsError、limits、handler 组装 |
| `sidecar/crates/halo-sidecar/src/fs/cage.rs` | 牢笼纯函数 + 单测 |
| `sidecar/crates/halo-sidecar/src/fs/ops.rs` | 文件操作 + 单测 |
| `sidecar/crates/halo-sidecar/src/fs/search.rs` | 有界搜索 + 单测 |
| `sidecar/crates/halo-integration-tests/tests/fs_boundary.rs` | 信任门禁/牢笼/`.git` 保护集成测试 |
| `sidecar/crates/halo-integration-tests/tests/fs_manual_edit.rs` | 运行中任务 + fs.write → 归因/事件集成测试 |
| `app/halo_studio/ipc/fs_client.py` | §5.4 |
| `app/halo_studio/viewmodels/explorer_viewmodel.py` | §5.5（FsNode/FsTreeModel/ExplorerViewModel/Decoration） |
| `app/halo_studio/qml/panels/ExplorerPanel.qml`、`app/halo_studio/qml/panels/ExplorerRow.qml` | §5.6 |
| `app/tests/test_fs_client.py`、`app/tests/test_explorer_viewmodel.py` | 见 §8 |

**修改：**

| 路径 | 改动 |
| --- | --- |
| `sidecar/crates/halo-protocol/src/methods/mod.rs` | `pub mod fs;` |
| `sidecar/crates/halo-protocol/src/error.rs` | +7 个 `ErrorCode` 变体 |
| `sidecar/crates/halo-protocol/tests/contract.rs` | 错误码快照 + fs DTO round-trip |
| `protocol/v1/envelope.schema.json` | 错误码枚举追加 |
| `sidecar/crates/halo-sidecar/src/main.rs` | `mod fs;` |
| `sidecar/crates/halo-sidecar/src/dispatch.rs` | fs 路由、CAPABILITIES+"fs"、`From<FsError>`、`require_trusted_root`、`mark_manual_edit_internal` 抽取 |
| `sidecar/crates/halo-sidecar/src/git.rs` | `ls_candidate_files()` |
| `sidecar/Cargo.toml`（workspace） | `[workspace.dependencies]` 追加 `globset`（`sha2`、`regex` 已有） |
| `app/tests/fake_sidecar.py` | fs.* 脚本化响应（含错误码注入），供 FsClient/EditorService/快速打开测试共用 |
| `docs/ipc-protocol.md` | 评审后并入 §4 契约增量全文 |
| `docs/module-contracts.md` | 第 1/6/8 节按 §3 增量更新 |
| `docs/design/README.md` | 本文档状态改"已完成" |

### 7.2 依赖顺序

1. **契约先行**：halo-protocol DTO + 错误码 + schema + 契约测试（其余工作的共同前提）；
2. **Rust fs 模块**：cage → ops → search → dispatch 接线 → 集成测试（此步完成即可交付 Sidecar，`cargo test --workspace` 全绿）；
3. **fake_sidecar 扩展**（Python 各方测试的接缝，先于 4/5）；
4. **FsClient**（11 号 EditorService 与 13 号快速打开自此可并行开工）；
5. **ExplorerViewModel + QML**（依赖 4；`openPreview/openPinned` 依赖 11 号 EditorService 存在——若 11 号未就绪，先以接口协议桩联调，集成期换实体）；
6. 15 号徽章/归因消费（依赖 1、2 的事件扩展与 5 的 `set_decorations` 挂点）。

---

## 8. 测试计划

### 8.1 Rust 单元测试（`fs/` 各文件 `#[cfg(test)]`）

- **cage**：`..`（含 `a/../..` 混合）、绝对路径（`C:\`、`\\server\share`、`\\?\C:\`、`/foo`）、NUL、保留设备名（`CON`、`com1.txt`）、结尾空格/点 → 各自错误；junction 逃逸：`cmd /c mklink /J`（无需管理员权限）建根外指向 junction，read/list 经其访问 → `OutsideWorkspace`；根内 junction 放行；`.git` 大小写变体（`.GIT\config`）写保护；空格/中文路径全通过（沿用 git.rs 测试的仓库命名习惯）。
- **ops-read**：8 MiB+1 → TooLarge；NUL 嗅探 → Binary；UTF-8/UTF-8 BOM/UTF-16LE/UTF-16BE 各解码正确且 BOM 剥离、encoding 如实；GBK 字节序列 → `unknown`+`lossy: true` 不误报二进制；line_ending 四态；hash 与手算 sha256 一致。
- **ops-write**：expected_hash 不匹配 → Conflict（details 含 current_hash）；匹配 → 原子替换且返回新 hash；BOM/UTF-16 回写字节级还原；目标不存在 → NotFound；`.git` 内 → GitProtected；临时文件不残留。
- **ops-list**：depth=1 仅一层；depth 递归先序；条目上限截断标记；根层无 `.git`；目录前文件后大小写不敏感排序；symlink 标记且不下钻。
- **ops-create/rename**：AlreadyExists / 父目录 NotFound / 跨目录 rename（移动）/ `.git` 保护。
- **search**：paths-only 全清单（含未跟踪、不含被忽略与 `.git`）；glob 过滤；正则命中行列与 preview；大小写开关；单文件命中上限；max_results 截断；二进制/超大文件跳过；非法正则 → InvalidName 映射 INVALID_PARAMS。

### 8.2 Rust 集成测试（`halo-integration-tests`）

- **fs_boundary.rs**：hello 后未打开工作区 → `WORKSPACE_NOT_ACTIVE`；打开未信任 → 全部 8 个方法 `WORKSPACE_NOT_TRUSTED`；信任后 list/read/write 全链路通；`capabilities` 含 `"fs"`；错误码线格式（SCREAMING_SNAKE_CASE 字符串）快照。
- **fs_manual_edit.rs**：fake-pi happy 任务运行中 fs.write → 响应 ok、随后事件流出现 `task.manual_edit{source:"fs_write", path}`、`task.status` 归因 mixed、终态证据 `attribution_reasons` 含该记录；无任务时 fs.write 不产生事件。
- **credential_canary**（既有）补充：fs.read 一个内容含假密钥样式的文件——fs.read 返回原文（编辑器如实显示，**不**脱敏，脱敏只发生在证据/摘要出口），但该内容不得进入任何事件/日志路径。

### 8.3 Python 测试（pytest + pytest-qt，经 fake_sidecar）

- **test_fs_client.py**：8 个方法 params/result 序列化正确；错误码 → `FsError.code` 透传；`lossy` 结果标记保留。
- **test_explorer_viewmodel.py**：expand 惰性加载与 loading 态；collapse/再展开用缓存不重复请求；refresh 增量 diff（新增/删除/改名行，展开状态保持）；truncated 提示；task.state 终态事件触发自动刷新；workspace.changed 重建/清空；未信任态不发请求；validateName 全表；set_decorations 渲染 role 与 bubble 上浮；createFile 成功后行插入。
- **QML smoke**：`--smoke` 加载含 ExplorerPanel 的场景不报错（10 号壳层就绪前用独立测试载入组件）。

### 8.4 验收口径

`scripts/test-all.ps1`（cargo test --workspace + pytest）全绿；现有 248+57 测试不回归。

---

## 9. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| `fs.search` 在 Sidecar 单线程 dispatch 上串行阻塞其他请求（如编辑器保存） | 大仓库搜索期间 UI 请求延迟最多 5 s | 内部并行扫描（万级文件实测应 < 1 s）+ 5 s 硬预算 + 候选/命中上限；13 号 UI 侧对连续键入做防抖与结果复用；**远期**：dispatch 改按请求线程池或对 fs.search 单独 worker 化（协议不变） |
| 无文件监听 → 资源管理器视图陈旧（Agent 运行中改文件不实时可见） | 用户可能基于过期树操作 | 任务终态自动刷新覆盖最重要时机；写类冲突由 expected_hash 乐观锁兜底（陈旧视图导致的覆盖不可能发生）；可选定时刷新；远期 `notify` + `fs.changed` 追加式事件（§1.2） |
| Windows 路径细节（保留名、大小写不敏感、junction、`\\?\` 前缀） | 牢笼被绕过或误拒 | cage 收敛为纯函数 + §8.1 穷举测试；canonicalize 前缀比较用组件级路径比较而非字符串前缀；junction 测试用 `mklink /J` 免管理员权限 |
| 非 UTF-8（GBK 等）文件被误判二进制或被 lossy 回写破坏 | 中文用户旧代码文件损坏 | 探测顺序保证无 NUL 文本走 `unknown`+lossy；契约锁定 lossy 只读、`fs.write` 的 encoding 枚举不含 unknown（想写必须先转码，首期不提供转码） |
| 大目录（node_modules 级）fs.list 卡顿 | 展开即万级条目 | 单层惰性展开为主 + 10000 条目上限截断标记；截断行在 UI 显示"目录过大已截断"提示 |
| 归因钩子重复触发（编辑器频繁保存） | Mixed reasons 刷屏 | 触发点如实必达（本文责任），去抖/汇总在 15 号统一裁决（该处持有归因语义全貌） |
| `real_path != git_root`（打开仓库子目录）时徽章路径错位 | 徽章不显示或错行 | 契约明示牢笼根=real_path、证据路径相对 git_root；换算责任与算法（剥前缀+过滤子树外条目）已写入 §5.3/§6，15 号照做并配测试 |

---

## 修订记录

- 2026-07-27：首版（03 号对齐记录触发；fs.* 契约 + 资源管理器 + FsClient 完整设计）。
