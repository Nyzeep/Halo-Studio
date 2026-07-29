# 15 - 差异化功能（可验证编码交付 × IDE）

**状态**：设计完成，待评审
**日期**：2026-07-27
**依据**：`requirements-alignment/03-ide-editor-and-reference-alignment.md`（范围内 5）、`docs/design/references/R5-bitfun-analysis.md`、`docs/ipc-protocol.md`（任务/证据/审查节）、10/11/12/13 号设计文档
**裁决权**：03 号对齐记录与 10-13 号文档均把差异化功能的语义裁决权交给本文档。本文对五个功能给出最终裁决；10-13 号预留的挂点在此接线，任何与本文冲突的差异化描述以本文为准。

---

## 1. 目标与范围

### 1.1 定位

这五个功能是"可验证编码交付"与"IDE 形态开发工作台"的**正交组合**，为本产品独有：通用 IDE 没有任务基线/归因/证据概念，通用 Agent 工具没有内嵌人工编辑面。每个功能都只是把既有的交付事实（任务基线、归因、交付证据版本、交付审查）投影到 IDE 表面（编辑器、资源管理器、审查视图），**不产生新的事实类别，不改变既有红线**。

### 1.2 功能清单与分级

| # | 功能 | 分级 | 一句话 |
| --- | --- | --- | --- |
| F1 | 人工介入自动归因 | **MVP（核心）** | 活跃任务期间经 Sidecar `fs.*` 写类方法落盘 → 自动归因转 Mixed + `task.manual_edit` 事件 + 编辑器标签徽章；诚实标记，绝不阻止保存 |
| F2 | 任务上下文选择器 | **MVP** | 资源管理器右键 / 编辑器命令把文件或选区一键加入任务创建表单的 files / notes；清单可见可移除 |
| F3 | 审查→编辑器跳转 | **MVP** | 审查视图每个文件"在编辑器中打开"→ `EditorService.openFile(path, line)`；审查本身保持只读 |
| F4 | 基线感知徽章 | **次优先** | "任务基线以来已变更"的文件集合在资源管理器与编辑器标签显示徽章；证据为权威源，运行中轻量 fs.stat 提示为可裁剪增强 |
| F5 | 归因边栏 gutter | **次优先** | 最新证据 per-file diff 解析出任务关联变更行区间 → 编辑器 gutter 着色；哈希不匹配即降级为文件级徽章，绝不断言行级归因 |

### 1.3 范围外（明确不做）

- **不阻止、不延迟、不确认任何保存/写入**：归因是诚实标记而非门禁（F1 铁律）。
- **不做行级真实归因**：不引入编辑日志/按键流水去区分"这一行到底谁写的"；行级着色只是文件级归因事实在证据变更区间上的投影（F5 §4.5.3）。
- **不自动附带上下文**：F2 只提供"用户主动选取"的快捷入口，绝不自动把打开的文件、整个工作区或历史塞进任务说明（词汇表：任务说明）。
- **不把审查变为编辑器**：F3 是单向跳转，审查视图零编辑能力不变（03 号边界）。
- **不引入文件监听**：F4 刷新沿用 12 号"手动 + 任务终态 + 可选定时"三通道；`fs.changed` 事件属远期（12 号 §1.2）。
- **不在运行中任务上追加上下文**：F2 只作用于任务创建草稿；运行中任务无对应 IPC，不伪造该能力。
- 差异化功能均不触碰：凭据红线、无 Mock 生产回退、接受/拒绝不动 Git、单工作区单任务。

---

## 2. 参考结论引用

| 来源 | 借鉴 | 落法 | 不借鉴 |
| --- | --- | --- | --- |
| R5 §3.2 审查多维事实不折叠、**freshness 事实** | "证据相对当前工作树已过期"是独立事实，用哈希比对派生，不新增状态机 | F5 的 `end_hash` 失配降级、F4 徽章 tooltip 注明证据版本；行级装饰过期即撤，文件级事实保留 | freshness 独立状态机与 coverage 维度全集 |
| R5 §3.3 发现连续性：系统观察与用户处置分离；"模型沉默永不 resolve" | 归因是系统观察到的**事实**，只能由事实产生（fs 写入必达钩子），不能由 UI 旁路制造或撤销 | F1 归因判定单点在 Sidecar；徽章清除只随任务生命周期，不提供"撤销归因"按钮 | group key / occurrence fingerprint 双键体系 |
| R5 §3.4 只读审查身份 + "修复归因不明时如实退化并诚实标注范围" | 不能精确断言就明说不断言 | F5 混合归因文件的行级 tooltip 明写"行级归因不作断言"；漂移/截断/无哈希一律降级为文件级 | ReviewFixer 可写修复身份（审查保持只读） |
| R5 §5.3 / §7-8 诚实降级："无真实隔离不宣称已拦截" | 能力缺失/数据缺失 = 功能隐藏或降级并给原因，不伪造 | 旧证据无 `end_hash` → gutter 不显示；运行中 stat 提示的 tooltip 如实标"运行中提示"与其局限 | — |
| R5 §6.1 内部执行细节不出现在一级 UI | 一级 UI 只说用户语言 | 徽章/gutter 的 tooltip 不暴露树对象哈希、seq、内部 id | 状态词汇全集 |
| R1 §6.2 装饰通道（经 10/11/12 号转述采纳） | 脏点、人工编辑徽章、基线徽章、gutter 是**正交通道**，互不复用 UI 元素 | F1/F4/F5 各走 11/12 号已建的独立角色与装饰列 | 多来源 weight 合并框架 |
| R2 §1.2(c) 锚定而非行号（经 11 号采纳） | gutter 装饰以 `QTextCursor` 锚点存储，缓冲区内编辑自动漂移 | F5 直接消费 11 号 `setGutterDecorations` 的锚点机制 | CRDT 锚点实现 |

---

## 3. 与现有契约的关系（增量逐条）

### 3.1 对 `docs/ipc-protocol.md`

**本文档不新增任何 IPC 方法**。全部增量为既有消息形状的**追加式字段**与语义补充：

1. **`ReviewBundle.files[]` 追加可选字段 `end_hash`**（F5）。并入文本（3.5 节 ReviewBundle 示例的 files 条目）：

   ```jsonc
   "files": [{"path": "src/auth.rs", "change": "modified", "diff": "…", "truncated": false,
              "end_hash": "sha256:<64位hex>|null"}]
   ```

   说明：该文件在**结束树**中的内容哈希（对文件字节计算 sha256，与 `fs.read`/`fs.stat` 的 `hash` 同口径），供行级归因装饰做过期校验。`change: "deleted"`、文件 > 8 MiB、或本字段引入前入库的旧证据版本 → `null`。既有消费者不受影响。

2. **`ReviewBundle` 追加可选字段 `manual_edit_paths`**（F1/F5）：

   ```jsonc
   "manual_edit_paths": ["src/auth.rs"]    // 任务活跃期内发生过 fs 写类人工介入的工作区相对路径去重清单；
                                            // attribution 为 agent_only 时恒为空数组；旧记录缺省为 []
   ```

3. **`task.manual_edit` 事件语义补充**（对 12 号 §4.3 已定稿的事件行追加说明句，字段形状不变）：

   > 事件**逐次推送**（每次成功写入一条，UI 幂等消费）；归因 `attribution_reasons` 按（任务, 路径）**去重只记一次**；自动触发窗口为任务**活跃态**（`created`/`running`/`awaiting_action`/`finishing`），`review_ready` 及之后的写入不再改归因（见本文 §4.1.2 裁决）。

4. **对 12 号待并入文本的一处修订**（12 号 §4.2 fs.write 行与 §5.2 钩子注释中的"存在非终态任务"）：统一改为"当前任务处于**活跃态**（created/running/awaiting_action/finishing）"。理由见 §4.1.2；12 号文本尚未并入 ipc-protocol.md，属评审期协同修订而非破坏性变更。

5. 错误码：**零新增**。

### 3.2 对 `docs/module-contracts.md`

| 节 | 增量 |
| --- | --- |
| §2 halo-core | `FileEvidence` 追加 `pub end_hash: Option<String>`（EvidenceDraft 同步）；新增纯函数 `pub fn manual_edit_note(op: ManualEditOp, path: &str, to_path: Option<&str>, local_hhmm: &str) -> String`（`ManualEditOp = Write | CreateFile | CreateDir | Rename`）；`limits` 追加 `MANUAL_EDIT_REASONS_MAX: usize = 64` |
| §4 halo-store | `tasks` 表追加列 `manual_edit_paths TEXT NOT NULL DEFAULT '[]'`（JSON 数组）；`evidence` 文件记录追加 `end_hash` 存储；schema_version 递增，迁移向后兼容（旧行读出为默认值） |
| §6 halo-sidecar | `GitClient` 追加只读方法 `show_tree_file(tree: &str, path: &str) -> Result<Vec<u8>, GitError>`（`git cat-file` 读结束树文件内容，只读红线内）；`mark_manual_edit_internal` 按 §4.1 语义实现（去重、note 文案、上限、不阻断）；`task_flow.rs` 证据落库时计算 `end_hash` 并带出 `manual_edit_paths` |
| §8 app | 新增包 `halo_studio/differentiation/`（文件清单见 §6.1）；root context 新增属性 `taskContextVM`（F2）、`reviewJumpVM`（F3）；F1/F4/F5 控制器为无 QML API 的 headless 对象，由 `AppContext` 持有强引用 |
| §10 所有权 | 新增一行：`py-differentiation` → `app/halo_studio/differentiation/**`、`app/tests/test_differentiation_*.py`；集成期允许按本文 §6.2 修改 10/11/12 号所有的挂点文件 |

### 3.3 对 10/11/12/13 号设计文档的增量与消费

| 文档 | 消费（既有挂点，零改动） | 增量（追加式，评审后并入对方文档） |
| --- | --- | --- |
| 10 号 | `statusBarDifferentiationSlot`（F1 状态栏归因位）、`taskContextSelectorSlot`（F2 芯片区）、`ReviewSurface.openInEditorRequested(path, line)`（F3）、token `gutterAgentChangeBackground` / `baselineChangedBadgeForeground` / `decoration*Foreground` | Theme 追加 1 个 token：`gutterMixedChangeBackground = #8cd29922`（F5 混合归因行，warn 同族半透明）；`ShellViewModel` 追加 `@Slot() showEditor()`（等价 `centerMode="editor"`，F3 跳转后切回编辑器，避免复用 `activate("review")` 的再点翻转语义） |
| 11 号 | `openFile(path, line)`、`setGutterDecorations`（锚点机制）、`setBaselineChangedPaths`、`currentSelection`、`task.manual_edit` 徽章消费（§4.9） | ① `currentSelection` 追加字段 `text`（选区原文，≤ 8 KiB；超限置 `""` 并追加 `textTruncated: true`）；② `EditorService` 追加信号 `documentSaved(documentId, path, sha256)`（保存成功后发射，携带写入返回的新哈希）；③ `EditorDocument` 追加只读属性 `diskSha256`；④ **裁决**：`task.manual_edit` payload 以 12 号并入契约的**单数 `path`** 为准，11 号 §4.9 的 `files` 复数形态按单路径消费（命中打开文档置徽章，`path: null` 时走全局提示分支） |
| 12 号 | `mark_manual_edit_internal` 钩子必达（F1）、`FsTreeModel.set_decorations`（F4）、ExplorerPanel 上下文菜单"加入任务上下文"项（F2）、`real_path != git_root` 剥前缀换算责任（F4/F3 照做） | ① 钩子任务状态窗口收窄为活跃态（§3.1-4）；② ExplorerPanel 菜单项接线方式：**不经 `registry.execute`**，直接 `taskContextVM.addFile(model.relPath)`（13 号 execute 无参数透传，带参命令按 13 号 §4.3 说明 3 的既定豁免直调服务 API） |
| 13 号 | 保留命令 id `task.addFileToContext` / `task.addSelectionToContext` / `review.openInEditor`（§5），按 §4.1 命名规范注册进同一 registry | 三条命令的注册参数（title/category/when 见 §4.2.4、§4.3.4）；无参形态语义 = 作用于"当前活动对象"（活动编辑器文件/选区、审查当前选中文件） |

---

## 4. 详细设计

### 4.1 F1 人工介入自动归因（MVP 核心）

#### 4.1.1 用户故事

> 我在任务运行期间忍不住打开编辑器改了 `src/auth.rs` 并按下 Ctrl+S。保存立即成功，没有任何弹窗拦我；随后该文件的标签出现 ⚑ 徽章，状态栏归因位变为"归因 Mixed"，运行轨迹里多了一行人工介入记录。任务结束后，交付审查的归因原因里如实写着"用户于 14:32 经工作台保存 src/auth.rs"。我确信交付结论不会把我的手改冒算成 Agent 的产出——这正是"任务基线"承诺的：任务期间发生人工编辑时，不断言关联变更全部由 Agent 编写。

#### 4.1.2 裁决

| 争点 | 裁决 | 理由 |
| --- | --- | --- |
| **幂等性：多次保存记一次还是逐次累计** | **归因原因按（任务, 路径）去重只记一次**（首次写入时间入文案）；**`task.manual_edit` 事件逐次推送**（UI 幂等消费） | 归因是"该文件是否发生过人工介入"的**事实位**，不是编辑流水账：逐次累计会让 `attribution_reasons` 随每次 Ctrl+S 膨胀、撑向 SUMMARY 限长并把审查界面变成噪音（R5 §3.3：观察去重聚合，同一事实不重复告警）。事件则是**过程通知**——徽章/轨迹需要实时性，逐次推送成本为人手保存频率，且不入库不产生持久噪音 |
| **触发窗口** | 任务**活跃态**（`created`/`running`/`awaiting_action`/`finishing`）；`review_ready` 及之后不触发 | `review_ready` 时交付证据已定稿：其 diff（基线树→结束树）**不包含**此后的编辑，此时追改归因等于对一份不含该改动的证据断言人工介入，违背诚实原则。审查期的后续编辑由 F5 哈希失配降级与 F4 徽章如实呈现（R5 §3.2 freshness 是独立事实，不折叠进归因） |
| **触发操作集** | `fs.write` / `fs.create_file` / `fs.create_dir` / `fs.rename`（12 号已锁定的全部写类方法）；rename 去重键取 `to` 路径 | 四者都是原生工作区改动之外的人工介入面；漏掉任何一个都会造成归因盲区 |
| **note 文案** | `用户于 {HH:MM} 经工作台{操作}{path}`；操作 ∈ 保存 / 新建文件 / 新建目录 / 重命名（rename 文案含 `{from} → {to}`）；时间为本地时区 HH:MM，取本地时区失败回退 UTC 并加 " UTC" 后缀 | 与既有示例 `"用户于 08:12 标记人工编辑"` 同风格；统一"经工作台"——Sidecar 只知道请求来自 UI 进程的 fs 通道，不猜测是编辑器还是资源管理器（不断言未知事实） |
| **原因条数上限** | `MANUAL_EDIT_REASONS_MAX = 64`；达到上限后追加**一条**汇总 `"此后仍有更多文件发生人工编辑（逐条记录已省略）"`，之后不再追加原因（去重集与事件照常） | 防御性上限；`manual_edit_paths` 字段仍完整记录路径集合 |
| **不阻止保存** | 归因三步（更新 attribution → `put_task` → `emit`）任一失败**不影响** `fs.write` 的成功响应；失败仅记 stderr 诊断，且**不**将该路径记入去重集（下次写入自然重试） | 铁律：诚实标记而非阻拦；文件写入的成败只取决于文件系统本身 |

#### 4.1.3 契约增量

复用 12 号已定义的全部形状（`task.manual_edit` 的 `source: "fs_write"` + `path`；`maybe_mark_manual_edit` 必达钩子）。本文新增：`ReviewBundle.manual_edit_paths`（§3.1-2）、事件语义补充句（§3.1-3）、触发窗口修订（§3.1-4）。**无新方法、无新事件。**

#### 4.1.4 三层改动点

| 层 | 改动 |
| --- | --- |
| Rust | `halo-core`：`manual_edit_note()` 纯函数 + `MANUAL_EDIT_REASONS_MAX`。`halo-store`：`tasks.manual_edit_paths` 列。`halo-sidecar`：`mark_manual_edit_internal(note, source, path)` 实现去重（任务运行态持 `BTreeSet<String>`，随 `put_task` 持久化）、活跃态窗口判定、上限与汇总条、失败不阻断；`maybe_mark_manual_edit` 扩为携带 `op: ManualEditOp`；`review.get` 组装时带出 `manual_edit_paths` |
| Python | `differentiation/manual_edit_notifier.py`：订阅 `task.manual_edit` / `task.state`，维护会话内人工介入路径集与计数（供状态栏 tooltip 与 F5 消费）；`TaskViewModel` 若尚无 `attribution` 属性则追加（notify，自 `task.state` 事件的 TaskStatus 取值） |
| QML | `differentiation/AttributionStatusItem.qml` 注入 10 号 `statusBarDifferentiationSlot`：`taskVM.attribution === "mixed"` 时显示"归因 Mixed"（`Theme.warn` 前景），tooltip"本任务期间发生人工介入：N 个文件"，点击 → 打开底部面板轨迹。编辑器标签 ⚑ 徽章由 11 号既有实现消费事件，本文零改动 |

#### 4.1.5 验收标准（集成测试场景）

1. **自动归因主链路**（`fs_manual_edit.rs` 扩展）：fake-pi `action_request` 模式使任务停在 `awaiting_action` → `fs.write` 同一文件两次 → 两次响应均 `ok: true`；事件流出现 **2 条** `task.manual_edit{source:"fs_write", path}`；`task.status` 的 `attribution == "mixed"`；任务终态后 `review.get` 的 `attribution_reasons` 恰含 **1 条**该文件原因（含路径与 HH:MM），`manual_edit_paths == [该路径]`。
2. **多文件**：写入第二个文件 → 原因增至 2 条、`manual_edit_paths` 含两路径。
3. **窗口边界**：任务进入 `review_ready` 后 `fs.write` → 写入成功、**无** `task.manual_edit` 事件、归因不变。
4. **无任务**：无活动任务时 `fs.write` → 成功且无事件（12 号既有断言保留）。
5. **不阻断**（Rust 单元）：注入 store 写失败 → `fs.write` 仍返回成功；去重集未记录该路径。
6. **UI**（pytest-qt）：注入 `task.manual_edit` 事件 → 打开文档徽章置位、状态栏归因位可见；任务终态 → 徽章清除（11 号既有测试引用）。

#### 4.1.6 术语一致性检查

| 术语 | 检查 |
| --- | --- |
| 任务基线 | ✔ 完整落实"任务期间发生人工编辑时，不断言关联变更全部由 Agent 编写"；基线前已有修改（baseline_dirty_files）不经此通道，继续"永不归因给 Agent" |
| 原生工作区改动 | ✔ 主 Agent 的改动不经 fs.*（受管应用按原生权限模型直写），不会误触本钩子；Halo 只观察与归因 |
| 交付证据版本 | ✔ 追加式不破坏；归因原因随任务记录演进，证据版本落库时固化快照 |
| 避免使用"自动交付/完成消息" | ✔ 归因转 Mixed 只是标记，不改变任务结论产生方式 |

---

### 4.2 F2 任务上下文选择器（MVP）

#### 4.2.1 用户故事

> 我准备让 Pi 修一个登录超时问题。在资源管理器右键 `src/auth.rs` 选"加入任务上下文"，又在编辑器里选中出问题的 15 行按 `task.addSelectionToContext` 命令。任务面板的表单里出现了文件芯片 `src/auth.rs ×`，补充说明末尾多了一个带行号的选区文本块。我删掉一个误加的芯片，填好目标点"创建任务"——发给 Agent 的 `files` 与 `notes` 就是我看到的这份清单，一个字不多。

#### 4.2.2 裁决与设计

- **作用对象 = 任务创建草稿**：芯片与选区块只进入创建表单；运行中任务不可追加上下文（无 IPC，不伪造）。任务创建成功后草稿清空。
- **文件加入** → `TaskSpec.files`：路径归一化（`/` 分隔、工作区相对），按归一化键去重；芯片区（10 号 `taskContextSelectorSlot`）逐项显示并带 × 移除。
- **选区加入的表达**（附入 `notes`，锁定格式）：

  ```
  --- 选区 src/auth.rs 第 120-134 行 ---
  <选区原文>
  --- 选区结束 ---
  ```

  同时把该文件加入 files 芯片（去重）。选区原文上限 **8 KiB 且 ≤ 200 行**；超限则不附原文，改为单行 `--- 选区 src/auth.rs 第 120-500 行（内容过长未附原文，请按行号查阅）---` 并提示用户。无选区时命令退化为把活动文件加入 files。
- **可见可移除**：files 芯片按钮移除；选区块位于 notes 文本框内，用户直接编辑删除（notes 本就是用户可编辑的补充说明，不做第二套结构化存储——单一事实源）。
- **命令接线**：`task.addFileToContext`（category 任务，when `hasWorkspace`；面板调用时作用于活动编辑器文件）、`task.addSelectionToContext`（when `hasWorkspace && hasActiveEditor`）。资源管理器菜单带行参数，按 §3.3 直调 `taskContextVM.addFile(relPath)`，不经 `registry.execute`。

#### 4.2.3 契约增量

**零 IPC 增量**：完全复用 `TaskSpec.files`（"用户主动选取，可空"）与 `notes`。11 号 `currentSelection` 追加 `text` 字段（§3.3）。

#### 4.2.4 三层改动点

| 层 | 改动 |
| --- | --- |
| Rust | 无 |
| Python | `differentiation/task_context.py`：`TaskContextViewModel(QObject)` —— `files` 列表模型（role: `relPath`）、`addFile(path)` / `removeFile(path)` / `addActiveEditorSelection()`（读 `editorService.currentSelection`，产出选区块经 `notesBlockAppended(str)` 信号交 QML 追加）、`clear()`、`hint` 属性（超限提示）；13 号 registry 注册两条命令（回调闭包捕获本 VM） |
| QML | `differentiation/TaskContextChips.qml` 注入 `taskContextSelectorSlot`（Flow 布局芯片 + ×）；集成期改 `TaskPanel.qml`：创建任务时 `files = taskContextVM 清单`，`notesBlockAppended` → notes TextArea 追加，创建成功 → `taskContextVM.clear()`；集成期改 `ExplorerPanel.qml` 菜单项 onTriggered 接线 |

#### 4.2.5 验收标准（集成测试场景）

1. `addFile` 去重与归一化：`src\a.rs` 与 `src/a.rs` 只产生一个芯片；`removeFile` 后创建任务 → fake_sidecar 收到的 `TaskSpec.files` 与芯片一致。
2. 选区加入：注入 `currentSelection{path, 120, 134, text}` → notes 追加块格式逐字匹配、files 含该路径；`text` 超 8 KiB / 超 200 行 → 无原文降级行 + `hint` 非空。
3. 无选区 → 退化为 addFile(activeFilePath)；无活动编辑器 → 命令 when 拦截（`executeFailed`）。
4. 创建成功后草稿清空；运行中任务（taskRunning）不影响草稿编辑，但创建门禁由既有 `TASK_ALREADY_RUNNING` 呈现，无旁路判断。
5. 红线回归：任何路径下 `TaskSpec` 不包含用户未显式加入的文件（对照词汇表"任务说明"反例）。

#### 4.2.6 术语一致性检查

| 术语 | 检查 |
| --- | --- |
| 任务说明 | ✔ "显式提供的任务目标 + 主动选取的文件、已有 Diff 或补充说明"——选择器只是"主动选取"的加速器；避免项"隐式完整工作区上下文/自动附带完整历史"经 §4.2.5-5 固化为测试 |
| Agent 任务 | ✔ 上下文只随 `task.create` 一次性提交，不引入运行中注入通道 |

---

### 4.3 F3 审查→编辑器跳转（MVP）

#### 4.3.1 用户故事

> 审查 Pi 的交付时，我在只读 Diff 里看到 `src/auth.rs` 有一处改法可疑。点击该文件行的"在编辑器中打开"，中心区切回编辑器并在第 120 行（该文件第一处变更）落下光标。我改完按 Ctrl+S（此刻任务已 review_ready，保存不再改归因），再切回审查页继续看下一个文件——审查页还是那个只读的审查页。

#### 4.3.2 设计

- **入口**：ReviewSurface 文件列表每行追加"在编辑器中打开"按钮（ghost 族图标按钮）→ 发 10 号预留信号 `openInEditorRequested(path, line)`；命令 `review.openInEditor`（category 审查，when `hasWorkspace`）作用于当前选中文件。
- **行号**：`line = first_target_line(diff)` —— 解析该文件 diff **第一个 hunk 头** `@@ -a,b +c,d @@` 的新侧起始行 `c`；diff 为空或 `truncated: true` 或解析失败 → `-1`（仅打开不定位）。按钮 tooltip 注明"定位基于证据版本 vN，文件此后再编辑可能已漂移"（诚实，不宣称精确）。
- **路径换算**：ReviewBundle 路径相对 `git_root`，`openFile` 需要相对 `real_path` 的工作区路径 → 剥前缀换算（12 号 §5.2 锁定的责任分配），换算后落在 `real_path` 子树外的文件按钮禁用（tooltip"该文件位于当前打开的子目录之外"）。
- **不可打开态**：`change == "deleted"` → 按钮禁用（tooltip"文件已在交付中删除"）；`renamed` → 打开新路径。跳转恒打开**当前工作树文件**（编辑器不打开历史版本——编辑面只有一个现实）。
- **接线**（集成期，Shell/Main 层）：`openInEditorRequested(path, line)` → `editorService.openFile(path, line)` + `shellVM.showEditor()`（§3.3 对 10 号的增量 slot）。
- **只读不变**：ReviewSurface 的 DiffViewer `readOnly` 恒真断言与"仅最新证据可决定"逻辑零改动。

#### 4.3.3 契约增量

**零 IPC 增量**。`openFile(path, line)` 即 11 号既有 API；10 号信号既有；新增仅 `showEditor()`（§3.3）。

#### 4.3.4 三层改动点

| 层 | 改动 |
| --- | --- |
| Rust | 无 |
| Python | `differentiation/diffparse.py`：纯函数 `first_target_line(diff) -> int` 与（F5 共用的）`added_line_ranges(diff)`；`differentiation/review_jump.py`：`ReviewJumpViewModel(QObject)` —— 输入 reviewVM 当前文件列表 + workspaceVM 的 `real_path`/`git_root`，输出每文件 `{editorPath, editorLine, canOpen, reason}`；注册 `review.openInEditor` 命令 |
| QML | 集成期改 `ReviewSurface.qml`：文件行追加按钮（enabled/tooltip 绑定 `reviewJumpVM`），onClicked 发既有信号；集成期在 Shell 层接线信号 → `openFile` + `showEditor()` |

#### 4.3.5 验收标准（集成测试场景）

1. `first_target_line` 参数化表：单 hunk / 多 hunk（取第一个）/ 纯新增文件（`+1` 起）/ 空 diff / truncated / 坏 hunk 头 → 各得预期行或 -1。
2. pytest-qt：触发按钮（或直接 emit 信号）→ `editorService.openFile` 收到换算后的路径与解析行；`shellVM.centerMode` 变为 `"editor"`。
3. deleted 文件按钮禁用；`real_path != git_root` 时前缀剥离正确、子树外文件禁用。
4. 红线回归：跳转前后 ReviewSurface 只读断言全绿；审查视图无任何写入动作新增。

#### 4.3.6 术语一致性检查

| 术语 | 检查 |
| --- | --- |
| 交付审查 | ✔ "以只读文件变更与 Diff…判断"——跳转不给审查加编辑能力，避免项"Halo 内置编辑（于审查内）"未被违反 |
| 03 号边界 | ✔ "审查与编辑器互跳但不混合"——本功能就是那个"互跳"的实现 |

---

### 4.4 F4 基线感知徽章（次优先）

#### 4.4.1 用户故事

> 任务结束进入待审查，我在资源管理器里一眼看到 `src/` 上浮着一个变更聚合点，展开后 `auth.rs` 行尾有橙色 `M`、新文件 `token.rs` 是绿色 `A`；编辑器里已打开的 `auth.rs` 标签也带上了 `M` 徽章。悬停提示"任务基线以来已变更（证据 v2）"。我据此优先审这些文件，而不是凭记忆找 Agent 动过哪里。

#### 4.4.2 设计

**数据源分两级，权威与提示分明：**

1. **权威源（必做）= 最新交付证据**：任务进入 `review_ready`（`task.finished` 事件或启动恢复时 `task.status`）→ `review.get`（最新版）→ `files[]` 集合即"任务基线以来已变更"（基线树→结束树 diff；`baseline_dirty_files` 天然不在其中，符合"基线前修改不归因"）。映射：

   | change | letter | colorToken | bubble |
   | --- | --- | --- | --- |
   | modified | M | `decorationModifiedForeground` | true |
   | added | A | `decorationAddedForeground` | true |
   | deleted | D | `decorationDeletedForeground` | true（自身行不存在，仅上浮祖先） |
   | renamed | R | `decorationModifiedForeground` | true |

   tooltip：`任务基线以来已变更（证据 v{N}）`。同时 `editorService.setBaselineChangedPaths(paths)` 点亮编辑器标签 `baselineChanged` 角色（11 号既有）。

2. **运行中轻量提示（可裁剪增强）**：任务活跃期，`trace.item` 中 `kind == "file_hint"` 且 `detail` 含 `path` 字段的条目进入候选集（detail 无 path 则忽略——不从自由文本猜路径）；对候选做 `fs.stat`，`mtime > baseline.captured_at`（RFC3339 字典序比较）→ 临时徽章，letter 同上但 tooltip 如实降级：`运行中提示：来自运行轨迹的文件线索，以交付证据为准`。证据到达后整体替换。

**刷新时机与成本上限（锁定）：**

| 项 | 值 |
| --- | --- |
| 证据徽章刷新 | `task.finished` / 启动恢复发现 `review_ready` 及之后含证据的任务 → 一次 `review.get`；不轮询 |
| 运行中 stat | file_hint 到达后 **2 s 去抖**合并批次；每任务累计 stat ≤ **200 次**（超出仅记候选不再 stat，状态栏不提示——提示属噪音）；串行发出，不并发轰炸 dispatch |
| 徽章生命周期 | 从数据到达 → **下一次 `task.create`（新基线）或 `workspace.changed`（切换/撤信任）清空**；accept/reject 不清（改动仍在工作树，事实未消失） |
| 路径换算 | 证据路径相对 `git_root` → 剥前缀转 `real_path` 相对路径；子树外条目丢弃（12 号 §6 锁定算法） |
| 门禁 | 工作区未信任或 fs capability 缺失 → 运行中 stat 通道整体停用（证据徽章不依赖 fs.*，照常） |

#### 4.4.3 契约增量

**零 IPC 增量**：复用 `review.get`、`fs.stat`、`trace.item(file_hint)`、`task.state`/`task.finished`。（file_hint 的 `detail.path` 字段形状由 14 号协议对齐落定；落定前无该字段即自然只有证据徽章——诚实降级，无需协调。）

#### 4.4.4 三层改动点

| 层 | 改动 |
| --- | --- |
| Rust | 无 |
| Python | `differentiation/baseline_badges.py`：`BaselineBadgeController(QObject)`（headless）—— 订阅上述事件，产出 `dict[relPath, Decoration]` → `explorerVM.model.set_decorations(...)`（12 号挂点）与 `editorService.setBaselineChangedPaths(...)`（11 号挂点）；内置去抖定时器、stat 计数器、路径换算 |
| QML | **零新增**：资源管理器徽章渲染是 12 号 `ExplorerRow` 既有 role；编辑器标签 `baselineChanged`（M 徽章、`baselineChangedBadgeForeground`）是 11 号既有 role |

#### 4.4.5 验收标准（集成测试场景）

1. 证据映射：构造含 modified/added/deleted/renamed 的 ReviewBundle → 装饰字典 letter/token/tooltip 逐项正确；deleted 路径入字典（供 bubble）；`setBaselineChangedPaths` 收到全集。
2. 生命周期：`task.create` → 全部清空；`workspace.changed` → 清空；accept 决定后不清。
3. 运行中通道：注入 3 条带 `detail.path` 的 file_hint + fake_sidecar 脚本 stat（两新一旧 mtime）→ 仅 mtime 新者得临时徽章且 tooltip 为"运行中提示"文案；证据到达 → 整体替换为证据徽章。
4. 成本上限：注入 201 条 file_hint → fake_sidecar 收到的 `fs.stat` 恰 200 次；2 s 去抖窗口内的多条 hint 合并为一批。
5. 路径换算：`real_path` 为 `git_root` 子目录时剥前缀正确、子树外证据文件不产生装饰。
6. 未信任工作区：无任何 `fs.stat` 请求发出。

#### 4.4.6 术语一致性检查

| 术语 | 检查 |
| --- | --- |
| 任务基线 | ✔ 徽章语义 = "识别任务开始后的关联变更"；`baseline_dirty_files` 不入徽章集（保留用户已有修改的界定） |
| 交付证据版本 | ✔ 权威源恒为最新证据版本，tooltip 标注版本号；不消费旧版本 |
| 验证结果 / 运行轨迹 | ✔ file_hint 是运行轨迹的规范化条目，提示级消费并如实标注来源，不冒充证据 |

---

### 4.5 F5 归因边栏 gutter（次优先）

#### 4.5.1 用户故事

> 审查后我把 `src/auth.rs` 跳进编辑器。行号侧第 120-134 行是一段蓝色条——这是本次交付的任务关联变更区间，悬停显示"任务关联变更（证据 v2 · 归因：仅 Agent）"。另一个我任务期间手改过的文件里，变更区间是橙色条，悬停如实写着"该任务归因 Mixed：此文件曾发生人工介入，行级归因不作断言"。我随手改了两行再保存，蓝条消失只剩标签上的 M 徽章——文件已偏离证据版本，工作台不假装还能对上行号。

#### 4.5.2 设计

**数据源恒为最新证据版本**（`is_latest`；与审查页当前浏览的历史版本无关）。对每个**打开的**文档（不预解析未打开文件）：

1. **门禁链（全部通过才显示行级）**：
   - 文档路径（换算回 `git_root` 相对）∈ 最新证据 `files[]`；
   - 该文件 `end_hash` 非 null 且 `diff` 非 `truncated`；
   - `end_hash == 文档当前磁盘哈希`（打开时取 `EditorDocument.diskSha256`，保存后取 `documentSaved` 信号携带的新哈希）。
   任一不满足 → **降级为仅 F4 文件级徽章**，不渲染任何行级装饰（R5 §3.2 freshness / §3.4 如实退化）。
2. **行区间**：`added_line_ranges(diff) -> [(start, end)]` —— 解析 unified diff 各 hunk 的新侧新增/修改行区间（`+` 行在新文件中的连续段）；纯删除 hunk 在其位置行给单行标记。
3. **着色（attribution 投影，文件粒度）**：

   | 条件 | colorToken | tooltip |
   | --- | --- | --- |
   | 任务 `attribution == agent_only` | `gutterAgentChangeBackground` | `任务关联变更（证据 v{N} · 归因：仅 Agent）` |
   | `attribution == mixed` 且 path ∈ `manual_edit_paths` | `gutterMixedChangeBackground` | `任务关联变更（证据 v{N} · 归因 Mixed：此文件曾发生人工介入，行级归因不作断言）` |
   | `attribution == mixed` 且 path ∉ `manual_edit_paths` | `gutterAgentChangeBackground` | `任务关联变更（证据 v{N} · 归因：Agent；本任务整体为 Mixed）` |

   **明确不做**行内谁写了哪个字符的断言——颜色区分的是**文件级归因事实**在变更区间上的投影（§1.3 范围外；词汇表"任务基线"避免项）。
4. **写入通道**：`editorService.setGutterDecorations(documentId, [{line, kind: "attribution", colorToken, tooltip}])`（11 号既有 API；锚点存储使缓冲区内编辑自动漂移，无需本文处理未保存编辑）。
5. **失效与重算时机**：文档打开 / `task.finished`（新证据）→ 计算并写入；`documentSaved` 且新哈希 ≠ `end_hash` → 清空该文档装饰；`task.create` / `workspace.changed` → 全清。解析结果按 `(task_id, evidence_version, path)` 记忆化；单文件 diff ≤ 256 KiB（既有限长），主线程解析为毫秒级，不开线程。

#### 4.5.3 契约增量

`ReviewBundle.files[].end_hash` 与 `manual_edit_paths`（§3.1，本文唯一的 IPC 契约增量，均为追加式可选字段）。Sidecar 生成证据时经 `GitClient.show_tree_file(end_tree, path)` 取结束树文件字节算 sha256（> 8 MiB 或 deleted → null；只读 git 命令，红线内）。

#### 4.5.4 三层改动点

| 层 | 改动 |
| --- | --- |
| Rust | `halo-core`：`FileEvidence.end_hash`；`halo-store`：evidence 记录追加存储 + 迁移；`halo-sidecar`：`git.rs::show_tree_file`、`task_flow.rs` 证据组装计算 end_hash、`review.get` 带出两个新字段 |
| Python | `differentiation/attribution_gutter.py`：`AttributionGutterController(QObject)`（headless）—— 订阅 `task.finished` / 文档模型 / `documentSaved`，执行门禁链与区间计算，调 `setGutterDecorations`；复用 `diffparse.added_line_ranges`；`manual_edit_paths` 优先取 ReviewBundle 字段，缺省回退 `manual_edit_notifier` 会话集 |
| QML | **零新增**：渲染即 11 号 `EditorGutter` 装饰列（6px 色条 + tooltip，只认 colorToken）；`gutterMixedChangeBackground` token 由 10 号 Theme 追加（§3.3） |

#### 4.5.5 验收标准（集成测试场景）

1. `added_line_ranges` 参数化表：单 hunk 连续新增 / 一 hunk 多段 / 多 hunk / 纯删除 hunk（单行标记）/ added 整文件 / 空与坏输入 → None。
2. 门禁链（pytest-qt + 构造 ReviewBundle）：哈希匹配 + agent_only → 蓝色区间齐全；`end_hash: null` → 无行级仅徽章；`truncated: true` → 同上；哈希不匹配 → 同上。
3. 归因着色：mixed 且 path ∈ manual_edit_paths → `gutterMixedChangeBackground` 与"不作断言"tooltip；mixed 但 path 不在集合 → agent 色 + "本任务整体为 Mixed"。
4. 漂移降级：显示后触发 `documentSaved`（新哈希 ≠ end_hash）→ 该文档 `setGutterDecorations(id, [])` 被调用，F4 徽章保留。
5. Rust 集成（happy 任务写 `hello_from_agent.txt`）：`review.get` 的该文件 `end_hash` 等于对磁盘该文件字节手算的 sha256；deleted 文件为 null。
6. 生命周期：`task.create` / `workspace.changed` → 全部文档装饰清空。

#### 4.5.6 术语一致性检查

| 术语 | 检查 |
| --- | --- |
| 任务基线 | ✔ "不断言关联变更全部由 Agent 编写"被落成 UI 事实：mixed 文件 tooltip 明写不作断言 |
| 可审查交付 / 交付证据版本 | ✔ 只消费最新证据版本；旧版本仅可查不驱动装饰（与 `EVIDENCE_NOT_LATEST` 同一哲学） |
| 交付审查 | ✔ gutter 在编辑器（人工编辑面），不改审查视图；两面经 F3 互跳不混合 |

---

## 5. 裁决汇总（差异化点登记）

| # | 裁决 | 影响方 |
| --- | --- | --- |
| 1 | 归因原因按（任务, 路径）去重一次；事件逐次推送 | Sidecar、11 号徽章、历史/审查展示 |
| 2 | 自动归因窗口 = 任务活跃态，`review_ready` 后写入不改归因 | 12 号钩子文本修订、Sidecar |
| 3 | `task.manual_edit` payload 以单数 `path` 为准（11 号 `files` 复数形态废止） | 11 号 §4.9 |
| 4 | note 统一"经工作台{操作}"，含路径与本地 HH:MM；上限 64 条 + 1 条汇总 | halo-core、Sidecar |
| 5 | 归因持久化失败不影响 fs 写入成功（不阻止保存铁律） | Sidecar |
| 6 | 上下文选择器只作用于任务创建草稿；选区以锁定文本格式入 notes（8 KiB/200 行上限） | F2 全链 |
| 7 | 带参差异化命令直调 VM，不经 `registry.execute`（沿 13 号 §4.3 说明 3） | 12/13 号接线 |
| 8 | 跳转行 = diff 第一个 hunk 新侧起始行；不可解析 → 仅打开；恒打开当前工作树文件 | F3 |
| 9 | 徽章权威源 = 最新证据；运行中 stat 仅提示级且 tooltip 如实标注；stat ≤ 200 次/任务、2 s 去抖 | F4 |
| 10 | 徽章生命周期到下一次 `task.create` 或工作区变更；accept/reject 不清 | F4 |
| 11 | 行级 gutter 门禁链（在证据集 ∧ end_hash 存在 ∧ diff 未截断 ∧ 哈希匹配），任一失败降级文件级 | F5 |
| 12 | 行级颜色是文件级归因的投影，不做行内归因断言；mixed 命中人工介入文件用 `gutterMixedChangeBackground` | F5、10 号 token |
| 13 | `ReviewBundle` 追加 `files[].end_hash` 与 `manual_edit_paths` 是本文唯一 IPC 契约增量（追加式） | ipc-protocol.md |

---

## 6. 实施计划

### 6.1 文件清单

**新建：**

| 路径 | 内容 | 功能 |
| --- | --- | --- |
| `app/halo_studio/differentiation/__init__.py` | 包导出 + `create_differentiation(...)` 装配工厂 | 全部 |
| `app/halo_studio/differentiation/diffparse.py` | `first_target_line` / `added_line_ranges` 纯函数（无 Qt） | F3/F5 |
| `app/halo_studio/differentiation/manual_edit_notifier.py` | 会话内人工介入集与计数 | F1 |
| `app/halo_studio/differentiation/task_context.py` | `TaskContextViewModel` | F2 |
| `app/halo_studio/differentiation/review_jump.py` | `ReviewJumpViewModel` | F3 |
| `app/halo_studio/differentiation/baseline_badges.py` | `BaselineBadgeController` | F4 |
| `app/halo_studio/differentiation/attribution_gutter.py` | `AttributionGutterController` | F5 |
| `app/halo_studio/qml/differentiation/AttributionStatusItem.qml`、`TaskContextChips.qml`（+ qmldir） | 状态栏归因位、上下文芯片 | F1/F2 |
| `app/tests/test_differentiation_diffparse.py` 等 6 个测试文件 | §7 | 全部 |
| `sidecar/crates/halo-integration-tests/tests/differentiation_attribution.rs` | F1 场景 1-4 + F5 场景 5 | F1/F5 |

**修改：**

| 路径 | 改动 | 时机 |
| --- | --- | --- |
| `sidecar/crates/halo-core/src/…` | `FileEvidence.end_hash`、`manual_edit_note`、`MANUAL_EDIT_REASONS_MAX` | R1 阶段 |
| `sidecar/crates/halo-store/src/…` | 两处追加列 + 迁移 | R1 |
| `sidecar/crates/halo-sidecar/src/{dispatch,task_flow,git}.rs` | 钩子语义、end_hash、manual_edit_paths 带出 | R2（依赖 12 号 fs 模块落地） |
| `app/tests/fake_sidecar.py` | `task.manual_edit` 事件剧本、含 end_hash/manual_edit_paths 的 ReviewBundle 剧本 | P1 |
| `app/halo_studio/app.py` | 装配 differentiation 包、context 属性、命令注册 | P2 |
| 10 号所有：`Theme.qml`（+1 token）、`shell.py`（showEditor）、`StatusBar/TaskPanel/ReviewSurface` 挂点接线 | §3.3 增量 | 集成期 |
| 11 号所有：`service.py`（currentSelection.text、documentSaved、diskSha256） | §3.3 增量 | 集成期 |
| 12 号所有：`ExplorerPanel.qml` 菜单接线 | §3.3 增量 | 集成期 |
| `docs/ipc-protocol.md` / `docs/module-contracts.md` / `docs/design/README.md` | §3.1/§3.2 并入、状态更新 | 评审后 |

### 6.2 依赖顺序

1. **R1（Rust 契约）**：halo-core / halo-store 增量 + 契约测试——独立可先行；
2. **R2（Sidecar）**：依赖 12 号 fs 模块（钩子必达点）落地后实现 F1 语义与 F5 end_hash；`cargo test --workspace` 全绿即可交付；
3. **P1**：`diffparse.py`（零依赖，随时可开）+ fake_sidecar 剧本扩展；
4. **P2（MVP 批）**：F1 notifier/状态栏 → F2（依赖 11 号 currentSelection.text 与 10 号槽位）→ F3（依赖 11 号 openFile 与 10 号 showEditor）；
5. **P3（次优先批）**：F4（依赖 12 号 Explorer 与 11 号标签角色）→ F5（依赖 R2 的 end_hash 与 11 号 documentSaved/diskSha256）。

MVP 验收线 = F1/F2/F3 全部验收标准通过；F4/F5 可独立后置，不阻塞 MVP 发布。

---

## 7. 测试计划

| 层 | 载体 | 断言要点 |
| --- | --- | --- |
| Rust 单元 | halo-core | `manual_edit_note` 四操作文案（含 rename 双路径、UTC 回退后缀）；`FileEvidence.end_hash` serde round-trip（含 null）；reasons 上限与汇总条 |
| Rust 单元 | halo-store | 迁移幂等；旧行读出 `manual_edit_paths=[]` / `end_hash=None`；append-only 不回归 |
| Rust 集成 | `differentiation_attribution.rs` | §4.1.5 场景 1-4（去重/逐次事件/活跃态窗口/无任务）；§4.5.5 场景 5（end_hash 与磁盘手算一致）；归因失败注入不阻断写入 |
| Python 单元（无 Qt） | `test_differentiation_diffparse.py` | §4.3.5-1 与 §4.5.5-1 参数化全表；坏输入不抛异常 |
| Python 单元（pytest-qt + fake_sidecar） | 各控制器测试 | F1：事件→集合/计数/属性；F2：§4.2.5 全部；F3：§4.3.5-2/3；F4：§4.4.5 全部（含 stat 计数上限与去抖合并）；F5：§4.5.5-2/3/4/6 |
| QML/冒烟 | `--smoke` 扩展 | 装配含 differentiation 上下文属性后根对象加载成功；芯片/状态栏组件独立实例化不报错 |
| 红线回归 | 既有测试全量 | `scripts/test-all.ps1` 全绿（cargo 248 + pytest 57 基线只增不减）；审查只读断言、凭据 canary、`qml/**` 无裸色值扫描（新 QML 文件纳入）均不回归 |

---

## 8. 风险与缓解

| # | 风险 | 缓解 |
| --- | --- | --- |
| 1 | `saveAll` 在活跃任务期一次写 N 个脏文件 → N 条事件/原因瞬时涌出 | 原因按文件去重 + 64 条上限；事件量 = 脏文件数（人手规模）；轨迹按事件正常滚动，无额外抑制必要——如实呈现本就是产品语义 |
| 2 | 本地时区获取失败或跨时区困惑（note 时间） | 失败回退 UTC 并加显式后缀；协议时间戳仍全 UTC，note 只是展示文案 |
| 3 | 用户误读行级颜色为"精确到行的归因" | tooltip 文案明写"行级归因不作断言"；mixed 色只在确知人工介入过的文件出现；文档与验收测试锁定文案 |
| 4 | diff 解析器遇到非常规 diff（无 hunk 头、\ No newline 等）出错 | `diffparse` 纯函数 + 坏输入返回 None/-1 的防御式契约 + 参数化坏输入用例；解析失败 = 降级而非异常 |
| 5 | `real_path != git_root` 前缀剥离错位导致徽章/跳转错文件 | 换算集中在 `baseline_badges` / `review_jump` 各一处 + 专项测试（12 号 §6 已锁定算法）；子树外条目丢弃而非猜测 |
| 6 | 运行中 stat 提示在大任务下形成请求风暴 | 2 s 去抖 + 200 次/任务硬上限 + 串行发送；detail 无 path 即忽略，绝不全树扫描 |
| 7 | end_hash 计算增加证据生成耗时（大交付多文件） | 单文件 ≤ 8 MiB 上限（超限置 null）；`git cat-file` 逐文件读取与既有 diff 生成同量级；文件数已受证据限长约束 |
| 8 | store 迁移触碰既有数据 | 追加列 + DEFAULT 值，迁移幂等测试；旧证据 end_hash 为 null 走 F5 降级路径（诚实，无需回填） |
| 9 | 会话重启后运行中人工介入集丢失（F5 mixed 判定依赖） | 权威源是 `ReviewBundle.manual_edit_paths`（持久化）；会话集仅活跃期兜底——重启后任务本会转 `interrupted`，无行级装饰需求 |
| 10 | 与 14 号协议对齐的 file_hint.detail.path 字段时序 | F4 运行中通道对字段缺失自然停用（零耦合降级）；字段落定后无需改本设计 |

---

## 修订记录

- 2026-07-27：首版（03 号对齐记录触发；R5 分析与 10/11/12/13 号设计为输入；五功能裁决与唯一 IPC 追加增量 `end_hash`/`manual_edit_paths` 在此锁定）。
