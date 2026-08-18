# R5 - BitFun 参考分析：运行时分层、适配器边界、审查生命周期与治理思想

**参考项目**：`D:\用于参考的开源项目的代码\BitFun-main`（只读，不复制任何源码）
**借鉴边界**（01/03 号对齐记录锁定）：借鉴运行时与界面分层、能力/适配器边界、审查生命周期、扩展权限治理、故障状态透明化；**不借鉴** Tauri/WebView 界面承载、通用 AI 平台定位、办公与 Mini App、远程多端——本文对不借鉴区域仅一句话带过。
**分析日期**：2026-07-27

---

## 1. 运行时分层

### 1.1 Cargo workspace 的六类 crate 分层

BitFun 后端是一个约 37 成员的 Cargo workspace（根 `Cargo.toml`），`src/crates/` 下按**接口归属**而非按调用方分为六类：

| 层 | crate（相对路径 `src/crates/`） | 职责 |
| --- | --- | --- |
| contracts（稳定数据接口） | `contracts/core-types`、`contracts/events`、`contracts/runtime-ports`、`contracts/product-domains` | 纯 DTO、事件 schema、端口 trait、权限/审计契约；不依赖任何上层 |
| execution（内核与执行层） | `execution/agent-runtime`、`execution/agent-stream`、`execution/harness`、`execution/tool-contracts`、`execution/tool-execution`、`execution/runtime-services`、`execution/plugin-runtime-client` 等 | 会话/轮次/调度/取消/权限协调/事件产生；工具契约与执行；工作流（SDD/DeepReview 等）编排 |
| adapters（生态与提供方适配） | `adapters/ai-adapters`、`adapters/opencode-adapter`、`adapters/claude-code-adapter`、`adapters/codex-adapter`、`adapters/static-hook-support`、`adapters/transport` | AI Provider 协议差异吸收；外部生态来源发现与映射；传输适配 |
| services（平台服务） | `services/services-core`、`services/terminal`、`services/services-integrations` 等 | OS/终端/进程等边界外 I/O 的具体实现 |
| assembly（产品组装） | `assembly/core`、`assembly/product-capabilities`、`assembly/external-sources` | 组装根：按 DeliveryProfile 选择能力、服务实现与降级策略 |
| apps / interfaces（入口） | `src/apps/{desktop,cli,server,sdk-host}`、`interfaces/acp` | 各入口协议的薄适配器 |

**锁定的依赖方向**（`docs/architecture/product-architecture.md` §3.3）：
产品入口 / interfaces → assembly → adapters / services / execution → contracts；assembly 不得依赖 app crate；`contracts/*` 与 `runtime-ports` 不得依赖任何上层。这与我们 module-contracts.md 第 0 节的"六 crate 零依赖 + 只有 halo-sidecar 依赖全部业务 crate"是同一哲学，BitFun 用"接口归属决定 crate 边界"表述得更显式：**一个 crate 只能拥有一类稳定边界**。

### 1.2 "一个 Agent Runtime，多种交付形态"

核心组织原则（product-architecture.md §1 设计原则 13）：GUI、TUI、Headless CLI、SDK、ACP、Server 都只是**同一个 Agent Runtime 的 adapter**；Query、Session、Tool、Permission、Hook、Event 每项能力只有一个行为归属模块（owner），入口只能消费窄用例接口和只读视图，不能访问内部状态、复制业务规则或持有第二份权威状态。

运行时内核（`execution/agent-runtime/src/`）的会话模型是四级层次：
`SessionManager -> Session -> DialogTurn -> ModelRound`（一次用户轮次可含多个模型轮）。装配采用全类型化 builder：`AgentRuntimeBuilder` 只接收已组装的类型化部件（submission / session management / dialog turn / lifecycle delivery / cancellation 五类端口 + `RuntimeServices` + 各注册表），运行时自身**不创建**文件系统、终端、AI 客户端等具体实现——创建只发生在产品组装（assembly）。

`RuntimeServices`（`execution/runtime-services`）是一组 `Arc<dyn Port>` 的显式依赖集合（filesystem / workspace / session_store / permission / events / clock 必选，terminal / git / remote 等 Option），设计约束明确写为：不提供 `get<T>() -> Any` 服务定位器；能力缺失返回类型化 `unsupported`；它是运行时依赖集合而非全局可变 app 状态。

### 1.3 与 Halo Studio 的对照

- 我们的 Sidecar 是同步线程模型（std 线程 + crossbeam），BitFun 全栈 tokio async——不借鉴其异步栈，只借鉴分层与归属纪律。
- Halo 的"应用控制层（Python ipc/viewmodels）→ Sidecar → 受管应用"三段进程模型对应 BitFun 的"入口 adapter → Runtime API → Runtime"，两者都禁止入口层旁路业务；我们已通过"ViewModel 只经 client 说契约语言"落实。
- BitFun 的 `RuntimeServices` 显式端口集合，对照我们 `AppState { workspace, store, configs, pi, opencode, task }`——首期规模下无需端口化改造，但新增 `fs.*` 能力时应保持"能力独立、失败类型化、无服务定位器"的同款纪律。
- 不借鉴：BitFun 的 SDK Host / Server / Relay / 多实例 Local Agent Host 属通用 AI 平台定位，与我们单工作区单任务定位无关。

---

## 2. 能力 / 适配器边界

### 2.1 AI Provider 适配：统一语义与原生语义双轨保留

`adapters/ai-adapters` 是最成熟的适配器样板，内部结构：

- `providers/{anthropic,gemini,openai}/`：每个 Provider 独立的 request 构造与 message_converter；
- `stream/stream_handler/{anthropic,gemini,openai,responses}.rs`：每个 Provider 的流式解析器，全部归一到 `UnifiedResponse`；
- `client/quirks.rs`：Provider 个性差异单独隔离成文件，不散落在主流程。

`agent-stream/src/unified.rs` 的 `UnifiedResponse` 是"统一但不抹平"的范本：

- 规范化字段（text / reasoning_content / tool_call / usage）供内核消费；
- **同时保留** `finish_reason`（原始值，注释明确写"raw finish reason retained for diagnostics and replay"）与规范化的 `tool_call_completion` 终态事实——两者并存，不用规范化覆盖原生语义；
- `UnifiedTokenUsage` 对缓存读/写 token 分别建模，并在注释里逐个列出四家 Provider 的原生字段名与命中率分母口径——差异被文档化而非被平均化。

流超时用两阶段模型（`stream_handler/mod.rs` 的 `StreamTimeoutController`）：TTFT（首个有效输出）超时与 idle（相邻输出间隔）超时分离，避免单一超时把"模型思考慢"与"连接死了"混为一谈。

### 2.2 外部生态适配：一个生态一个 crate，外部类型不进内核

`adapters/opencode-adapter`、`claude-code-adapter`、`codex-adapter` 三个生态各一个 crate，内部按能力类别切文件（agent_source / command_source / hook_source / mcp_source / tool_source）。锁定规则（product-architecture.md）：

1. **"OpenCode 是兼容目标，不是内部模型"**：适配层保持外部生态的可观察行为（配置格式、加载顺序、冲突语义），但外部类型不能反向成为内核数据模型；
2. 适配器不依赖兄弟适配器；通用目录与能力归属模块只依赖开放的生态 ID 与能力专属 Provider 契约，**不按生态分支行为**；
3. 提交链固定为 `生态 adapter → 能力 Provider → 能力归属模块`，adapter 永远不直接写内核权威状态；
4. `opencode-adapter/src/lib.rs` 开头即声明边界："不执行 JavaScript、不安装 npm 包、不依赖用户本地 opencode CLI"——适配器只做发现、解析（用 oxc 做 TS 静态解析）与映射。

### 2.3 与 Halo Studio 的对照

我们 halo-runtime 的 Pi（stdio RPC）与 OpenCode（回环 HTTP）两个适配器正对应这个模式：协议差异留在各自适配器，向上统一为 `RuntimeEvent`。协议对齐 v2（设计文档 14 号）应吸收两点：

- 规范化事件里**并存**规范化字段与原生原始字段（如 `RuntimeTraceItem.detail` 保留原生 payload），不要让规范化成为信息漏斗；
- Pi/OpenCode 差异（就绪检查方式、取消语义、事件游标）按 quirks 思路集中在各适配器一处，`task_flow.rs` 编排层不按 agent 分支。

---

## 3. 审查生命周期

BitFun 的审查体系由两份文档锁定：`docs/architecture/deep-review.md`（当前执行基线）与 `docs/architecture/review-lifecycle.md`(目标产品架构），是本项目最有对照价值的部分。

### 3.1 记录 / 修订版本双层身份

- **Review record**：一条显式审查脉络（lineage）的稳定用户可见身份；
- **Review revision**：每次初始运行或用户显式 re-review 产生一个**不可变**修订版本，指向前驱 revision；
- 执行子会话（read-only child session）只是一个 revision 的可替换实现机制，不是产品身份；
- 最新 revision 主导展示，旧 revision 保持可查；目标陈旧不改写旧 revision 的执行阶段；
- record 变更经单一元数据服务、按 record 串行写入、以单调 record version 拒绝陈旧更新。

这与我们 halo-core 的 `EvidenceLog`（追加式证据版本，只有最新版可 accept/reject，`EVIDENCE_NOT_LATEST` 拒绝旧版操作）结构同构——BitFun 验证了我们的选择，并补充了"record 稳定身份 + revision 前驱链"的表述：重试/交接产生的多个证据版本共享同一任务身份，这正是我们 task → evidence versions 的关系。

### 3.2 多维事实不折叠为单一状态

review-lifecycle.md 最核心的设计决定：执行阶段、结果可用性、发现结论、证据覆盖度、目标新鲜度是**五个独立事实，禁止折叠成一个状态枚举**：

- 只有执行阶段（Phase）是状态机：`Preparing → Running → Completed / Failed / Cancelled`（Preparing 自环仅限同一幂等请求的修复，不得提交第二个逻辑审查轮次）；
- coverage（complete / limited / failed / unknown）描述实际审过的目标覆盖度；
- freshness（current / stale / unknown）专指目标版本不匹配；两者独立推导，禁止按优先级互相覆盖；
- UI 组合展示（如"需要关注 · 目标已过期"），而不是挑一个赢家；
- **"无发现"必须连同 coverage 与 freshness 一起展示，绝不渲染为 passed / approved / safe to merge；模型建议是建议，不是仓库门禁结果。**

对照 Halo：我们的 ReviewBundle 已把 outcome / attribution / verification 三态分离，方向一致。值得吸收的增量是 **freshness 事实**：交付进入 `review_ready` 后若工作区继续变化（人工编辑或再次运行），审查视图应能呈现"该证据版本相对当前工作树已过期"，与基线树指纹比对即可派生，不需要新状态机。

### 3.3 发现连续性：系统观察与用户处置分离

- **Observation**（new / repeated / changed / not observed）由系统对比前后结构化报告得出；
- **Disposition**（open / resolved / dismissed）只能来自用户显式动作，**模型沉默永不 resolve 一个发现**；后续报告未再出现只标 `not observed`，不自动 `resolved`；
- 双键设计：group key（规范化路径+类别+标题，跨 revision 聚合）与 occurrence fingerprint（位置/严重度/描述等证据指纹）；处置只有两键完全匹配才继承，证据变了标 `changed` 并回到待关注；
- 相似文本不足以断言语义同一；模糊/模型匹配可辅助建议但不得关闭发现。

### 3.4 只读执行与读写分离

- `CodeReview` / `DeepReview` 是只读对抗性审查身份，**没有编辑、命令、Git 修改工具**；`ReviewFixer` 是分离的可写修复身份，仅在用户批准后由前端动作触发，修复后的复查再开一个全新的只读审查子会话；
- 复查范围：能精确归因修复改动时用"原范围 ∪ 修复改动文件"，不能归因时**如实退回全工作区 diff 并诚实标注范围**——不宣称做不到的窄范围；
- 目标证据（target evidence）带不可变 base/head、逐文件状态、completeness/limitation 事实与工作区绑定（当前 HEAD 是否匹配、工作树是否脏污染）；证据无效时失败关闭。

这与 03 号对齐记录"交付审查保持只读、编辑器是独立人工编辑面"完全同向；"修复归因不明时如实退化并标注"与我们的 Mixed 归因原则同源。

另有一个工具级证据机制值得记录：`execution/agent-runtime/src/evidence_ledger.rs` 为每次工具执行记追加式台账（session/turn/tool、目标类型 file/command/subagent/artifact/checkpoint、状态含 partial_timeout、touched_files、含 diff_hash 的检查点、partial_output 上限 8000 字节）——"证据都有界、可截断、带哈希"的做法与我们 halo-core `limits` + `cap` 截断标记一致。

---

## 4. 事件规范化

### 4.1 管线全貌

```
Provider SSE 流
  → per-provider stream handler（adapters/ai-adapters/src/stream/stream_handler/*）
  → UnifiedResponse（execution/agent-stream/src/unified.rs）
  → 内核轮次编排（execution/agent-runtime/src/{dialog_turn,event_bus,event_router}.rs）
  → AgenticEvent（contracts/events/src/agentic.rs，Provider 无关的 tagged enum）
  → 前端投影（contracts/events/src/frontend_projection.rs）
  → 传输 adapter（Tauri emit / peer host，只转发不重定义）
```

关键设计事实：

1. **`AgenticEvent` 是唯一的产品事件词汇**：会话级（SessionCreated/StateChanged/Deleted）、轮次级（DialogTurnStarted/Completed/Cancelled/Failed）、模型轮级（ModelRoundStarted/Completed/AttemptSuperseded）、内容流（TextChunk/ThinkingChunk）、工具（ToolEvent）、系统错误。每个事件携带 session/turn/round 层级身份，且定义了 `AgenticEventPriority`（Critical=错误与取消立即发送 / High / Normal / Low）用于投递调度。
2. **工具调用有显式子状态机**（`ToolEventData`）：`EarlyDetected → ParamsPartial → Queued → Waiting → Started → Progress / Streaming / StreamChunk → ConfirmationNeeded → Confirmed / Rejected → Completed / Failed / Cancelled`。终态事件携带分段耗时（queue_wait_ms / preflight_ms / confirmation_wait_ms / execution_ms）——权限等待与真实执行的耗时被区分，故障与慢速可归因。
3. **身份的双名保留**：`ToolEventIdentity { tool_id, tool_name, effective_tool_name }`——Provider 面向名与运行时实际目标名不同时两者都保留，规范化不吃掉原生语义。
4. **重试透明**：`ModelRoundAttemptSuperseded` 事件 + `ModelRoundAttemptDiagnostic` 把被自动重试取代的尝试的**原始错误文本**保留下来（注释明确：故意保留 raw provider/transport text 供桌面按需展示，不改变重试策略）——用户可以看到"到底发生了什么"。
5. **投影层职责克制**：`frontend_projection.rs` 只做 `AgenticEvent → {event_name: "agentic://...", payload}` 的字段改名（snake→camel），文档明确它"不定义跨协议的事件类型、版本、回放或保留语义"；跨协议版本化事件清单必须随真实消费方单独设计。
6. 配套工程：`agent-stream/src/tool_call_accumulator.rs` 累积流式工具调用分片；`execution/tool-call-jsonrepair` 专门修复模型输出的残缺 JSON 参数。

### 4.2 与 Halo Studio 的对照

我们的管线（受管应用原生输出 → halo-runtime `RuntimeEvent` → sidecar 规范化为契约事件 `trace.item`/`task.phase`/… → 全局 seq → UI）与之同构。可吸收的增量：

- `task.action_request` 的解决目前只有事件出现/消失，BitFun 的 `ConfirmationNeeded → Confirmed / Rejected` 显式成对事件让权限流转在轨迹里首尾可对账——协议 v2 可考虑为 action_request 增加 resolved 事件（payload 带 request_id 与结果）；
- 终态携带分段耗时的思路可放进 TraceItem.detail（如任务等待用户操作的时长 vs 运行时长），零协议破坏；
- "重试如实入轨迹并保留原始原因"与我们"中断如实标记、不自动恢复重放"的纪律一致，Pi/OpenCode 适配器遇 EOF/坏帧转 Failed 时应在 detail 保留原始片段（脱敏后）。

---

## 5. 扩展权限治理（备将来，首期无扩展）

### 5.1 静态策略：分层规则栈与"后匹配者胜"

`contracts/product-domains/src/tool_permissions.rs` 是纯领域契约（无运行时/持久化/交互职责）：

- 规则 = `(action, resource, effect)`，effect ∈ Allow / Ask / Deny；规则列表**顺序显著，后匹配者胜**；
- 预设 `Ask`（默认）展开为一组规则：兜底 `*→ask`，`read→allow` 但 `.env`/`.env.*→ask`（`.env.example→allow`），git 只读命令逐条 allow——敏感例外排在宽松规则之后自然生效；
- **五层解析顺序**：`product_defaults → global(preset+rules) → project → agent → enforced`；enforced（产品/组织强制）永远最后，用户级 FullAccess 无法放宽它。

### 5.2 委派收紧：类型上不可能放宽的 ceiling

子代理（Subagent）委派用 `PermissionRuntimeCeiling`：父代理传给子代理的运行时限制**在构造函数处拒绝任何 Allow 规则**（`try_new` 遇 Allow 即返回校验错误）——委派只能保持或收紧，不可能放宽，这是类型系统层面的保证而非运行时约定。子代理策略解析 = 子代理自己的五层 + 父 ceiling 插在 enforced 之前。

### 5.3 决策、记忆与审计三分

`contracts/runtime-ports/src/permission.rs` 把权限持久化拆成三个端口：

- `PermissionGrantStorePort`：按项目记忆的授权（pending 请求明确不属于这里）；
- `PermissionAuditStorePort`：**append-only** 审计记录；
- `PermissionReplyStorePort`：一次权限答复的 grants + audit **原子提交**。

配套约束（agent-runtime-services-design.md §2.3）：决策请求必须携带 session/turn/agent/source/effect/**执行域**；UI 只展示决策与选项，不成为授权来源；模型输出只能辅助解释，不能直接写权限、审计或策略状态；`allow_in_sandbox` 只能在真实沙箱路径存在时返回——**没有 OS/容器隔离时不得宣称"已被沙箱拦截"，只能停用插件或明确报告 policy-limited**（诚实降级，与我们"无 Mock 生产回退"同一气质）。

### 5.4 来源发现与执行许可分离

- 生态来源后台发现，不阻塞产品入口；**发现与加载顺序只决定候选输入，不自动授予执行权限**；
- 可执行来源（Command/Tool/Subagent/MCP）在首次激活、内容版本变化、能力扩大、执行域/凭据可见范围变化时重新确认；**确认前不得 import 模块、启动 worker、读取凭据**；
- `contracts/product-domains/src/external_source_control.rs` 定义版本化控制面：状态（Discovering/Disabled/ReviewRequired/Conflict/Active/Degraded/Unsupported/Available/Removed）+ 枚举化恢复动作（Refresh/Retry/Review/ResolveConflict/InstallRuntime/ReconnectHost/ExitSafeMode）；
- 第三方 JS/TS 永远在受监督子进程（Plugin Host）中运行；进程崩溃时同组插件一起失效、按一次进程级预算恢复，不宣称 Host 内插件互相隔离；
- 静态发现工具（`adapters/static-hook-support`）连文件读取都有界：`read_bounded_file` 只读 `max_bytes+1` 字节，防止文件在 stat 与 read 之间变大导致无界分配（TOCTOU 防御），目录遍历有 depth/entries/files 四重上限；静态发现的 Hook 只脱敏展示、不加载 handler、不授予权限。

### 5.5 对 Halo 的意义

首期我们无扩展（03 号边界不动摇），本节纯粹备将来。与现状的同源点：halo-config 的 `ENV_WHITELIST` + 凭据失败关闭即"enforced 层"哲学；受信任 Git 工作区的 identity_changed 降级即"内容版本变化重新确认"。将来若引入 MCP/插件，五层规则栈、ceiling 只收紧、grants/audit 原子提交、发现≠许可四条原则应整体采纳。

---

## 6. UI 信息架构（仅概念层）

BitFun 桌面 UI 以 Tauri/WebView + React 承载（`src/web-ui/`）——承载方式明确不借鉴，以下只取信息组织概念。

### 6.1 Agent 中心工作台的组织

`src/web-ui/src/` 的领域切分：`app/`（壳层：layout / NavBar / NavPanel / SceneBar / panels / TitleBar）、`flow_chat/`（会话流主视图：state-machine / reducers / store / **tool-cards** / deep-review 子域）、`app/pages/`（session / git / file-viewer / terminal / settings / agents / skills）、`infrastructure/`（api / event-bus / runtime / theme / i18n）。

概念要点：

1. **会话流是主视图，工具调用渲染为结构化卡片（tool-cards）而非原始文本流**——每类工具有专属卡片组件，消费 §4 的 ToolEvent 状态机（含确认中/已拒绝/失败态）。这与我们"运行轨迹是结构化过程视图，原始终端输出永不作为主内容"一致。
2. **审查有独立子域**（`flow_chat/deep-review/`：commandParser / targetResolver / launchPrompt / Service），审查启动**不强制打开执行详情**，先留在父任务，结果以卡片沉淀且重启后仍可用。
3. **空状态三态强制**：metadata-only 或未加载的子会话必须显示 preparing / loading / load-failed，禁止"标题 + 空白面板"；详情用平实语言展示真实状态，**不暴露内部 Skill 名、agent id、packet id 或预算数字**。
4. **状态透明化的统一词汇**：对外一级状态固定为"已发现、已应用、可用、需确认、更新中、沿用上一版本、部分受限、暂时过期、已移除/已停用、不可用"，**并附原因与恢复建议**；内部实现状态（Host 重启、暂停等）只能作为详情映射，不得形成第二套并列状态；"静态预览、未执行"不得误报为可用。
5. **Capability Availability 单一事实源**：能力可用状态由产品计划+服务健康+策略计算，所有入口读同一状态；"入口隐藏 ≠ 能力已禁用"。

这些与我们已有决策（RuntimeStateInfo 的 `reason`/`recovery_hint`、每个受管应用独立健康状态、绝不合并全局在线）同构；增量在于三态空状态与"内部执行细节不出现在一级 UI"的明文纪律。

### 6.2 不借鉴项（一句话）

Tauri/WebView 与 React 承载、MiniApp 与办公场景（`MiniApp/`、`services/page-function-runtime`）、远程多端与中继（`apps/relay-server`、`src/mobile-web`、Remote/Peer 体系）、公开 Agent SDK 与通用 AI 平台定位（`apps/sdk-host`、`interfaces/acp` 的平台化目标）均与 Halo Studio 的单机单工作区定位无关，全部不借鉴。

---

## 7. 对 Halo Studio 的借鉴清单（≤8 条）

| # | 借鉴点 | BitFun 出处 | 映射到 Halo Studio |
| --- | --- | --- | --- |
| 1 | **接口归属决定 crate 边界**："一个 crate 只拥有一类稳定边界"、contracts 层零上行依赖、错误在适配器边界转成类型化错误 | `docs/architecture/product-architecture.md` §3.3、`agent-runtime-services-design.md` §1.4 | 验证并强化现有六 crate 零依赖纪律（module-contracts §0）；新增 `fs.*` 能力沿用：DTO 进 `halo-protocol`，实现进 `halo-sidecar`，不新建横向依赖 |
| 2 | **规范化与原生语义双轨保留**：统一响应并存原始 finish_reason；工具身份并存 provider 名与 effective 名；Provider quirks 集中隔离 | `execution/agent-stream/src/unified.rs`、`contracts/events/src/agentic.rs`（ToolEventIdentity）、`adapters/ai-adapters/src/client/quirks.rs` | 设计文档 14（协议对齐 v2）：`halo-runtime` 的 `RuntimeTraceItem.detail` 保留 Pi/OpenCode 原生 payload（脱敏后）；Pi/OpenCode 差异集中在各自适配器，`task_flow.rs` 不按 agent 分支 |
| 3 | **权限/确认流转显式成对事件 + 分段耗时**：ConfirmationNeeded→Confirmed/Rejected 闭环；终态带 queue/confirmation/execution 分段耗时 | `contracts/events/src/agentic.rs`（ToolEventData） | IPC v1 追加式增量：`task.action_request` 增加对应 resolved 事件（带 request_id 与结果）；TraceItem.detail 携带等待/执行分段时长——`halo-protocol` 新增事件 + `halo-sidecar` 事件规范化 |
| 4 | **审查多维事实不折叠**：phase/availability/findings/coverage/freshness 五维独立，"无发现"绝不渲染为 passed/safe | `docs/architecture/review-lifecycle.md` | `halo-core` ReviewBundle 已分离 outcome/attribution/verification；增量：以基线树/结束树指纹派生 **freshness 事实**（证据版本相对当前工作树是否过期），审查视图组合展示，不新增状态机 |
| 5 | **record/revision 双层身份 + 追加式修订 + 单调版本拒绝陈旧写** | `review-lifecycle.md` 域模型 | 验证 `halo-core` EvidenceLog append-only 与 `EVIDENCE_NOT_LATEST` 设计；重试/交接共享同一任务身份产生新证据版本的语义保持不变，历史视图保留旧版本可查 |
| 6 | **重试与故障透明化**：被取代的尝试保留原始错误文本入轨迹；空/未加载视图强制 preparing/loading/load-failed 三态，禁止空白面板 | `agentic.rs`（ModelRoundAttemptSuperseded/Diagnostic）、`review-lifecycle.md` 产品投影节 | `halo-runtime` 适配器 EOF/坏帧转 Failed 时在 detail 保留脱敏原始片段；`app/halo_studio` 各 QML 视图（审查/轨迹/历史）落实三态空状态，禁止裸空白 |
| 7 | **扩展治理四原则（备将来，首期不实施）**：五层规则栈后匹配者胜且 enforced 兜底、委派 ceiling 类型上只能收紧、grants+audit 原子提交且审计 append-only、来源发现≠执行许可（内容版本变化需重新确认） | `contracts/product-domains/src/tool_permissions.rs`、`contracts/runtime-ports/src/permission.rs` | 记录为将来 MCP/插件的治理蓝本；现有同源点：`halo-config` ENV_WHITELIST/凭据失败关闭 = enforced 层；工作区 identity_changed 降级 = 内容版本变化重新确认 |
| 8 | **状态透明化统一词汇 + 枚举化恢复动作**：一级状态附原因与恢复建议，内部状态只作详情映射不成第二套状态；"诚实降级"——无真实隔离不宣称已拦截 | `contracts/product-domains/src/external_source_control.rs`、product-architecture.md §5 | 验证 `RuntimeStateInfo{reason, recovery_hint}` 设计；IDE 壳层状态栏/资源管理器沿用"一级状态 + 原因 + 恢复建议"三件套；与"无 Mock 生产回退"红线互证 |

---

## 8. 结论

BitFun 在四个方面为本轮设计提供了高置信度的外部验证：接口归属式分层（对应我们 crate 解耦纪律）、追加式审查修订与多维事实分离（对应我们证据版本与三态验证结论）、事件规范化双轨保留（对应我们 TraceItem 规范化）、诚实降级与状态透明（对应我们失败关闭与 recovery_hint）。真正的新增输入是三条：freshness 事实、action_request 的显式闭环事件、空状态三态纪律——均可以追加方式并入 IPC v1 与 10/14/15 号设计文档，不触及任何既有消息形状。其平台化部分（Tauri/WebView、SDK/Server/Remote、插件运行时）与本产品定位无关，仅治理思想按第 7 条留档备将来。
