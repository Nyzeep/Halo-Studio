# DeepSeek Harness 上游现状研究（2026-09-05）

> 状态：研究输入，补充 `deepseek-harness-assessment-20260818.md`，不重复其结论框架
> 主源：本地 checkout `D:\DeepSeek Harness\deepseek-harness`（HEAD `d347e70390`，2026-09-04）+ 上游 README/官方文档站
> 用途：为「去 BitFun 化 + 从 DSH/pi 提取改进」的 wayfinder 决策票提供事实

## 1. 上游身份与成熟度

- 上游仓库 `github.com/deepseek-ai/deepseek-harness`，npm 包 `@deepseek-ai/dsh`，文档站 deepseek-harness.github.io；本地 remote 已核对为该地址。
- 许可证 **MIT**（根 package.json `license` 字段），符合 ADR-0052 的 MIT 归属要求。
- 技术栈：TypeScript monorepo（pnpm，Node ^22.19 || >=24），~50 个包按 `packages/<域>/<名>` 两级组织；另有 Python SDK（wheel 内打包 `dsh --profile sdk`）。
- README 的 developer preview 警告**原文仍在**："in _developer preview_ and iterating rapidly. THERE WILL BE COMPATIBILITY-BREAKING CHANGES."（README.md）
- 版本漂移（对照 8/18 评估时点）：
  - 2026-08-18 时点 ≈ `dsh-v0.1.2-alpha.5`（git tag 包含关系核实）
  - 本地 HEAD = `0.1.3-alpha.1`（release/dsh-0.1.3-alpha.1 分支，提交日期 2026-09-04）
  - 期间 **2561 个提交**；期间合入过一条 `session format migration` 分支（git log 可见），说明 SessionEvent 持久化格式本身仍在动——对候选 B 是直接风险信号。
- Cordis 框架来自 cordiverse/cordis，设计论文 arXiv:2608.25512（README 引用）。

## 2. 架构事实核对（对 8/18 评估的证实与更新）

评估文档中的架构描述全部得到证实，且更精确：

- **everything-is-a-plugin**：模型适配器、工具注册表、session log、agent loop 都是 Cordis 插件，注册即可逆卸载（docs/architecture.md）。
- **组合机制**：profile（web/headless/sdk/sdk-minimal/acp 模板）+ bundle（`dsh-base` 公共底层、`dsh-web-app`、`dsh-headless`、`dsh-sdk-app`、`dsh-acp-app`、`dsh-sdk-minimal` 特例）+ `cordis.patch.yml` 有序补丁层；web profile 支持补丁热重载。
- **核心包表**（architecture.md）：

| 包 | 职责 | ctx 键 |
|---|---|---|
| `core/session` | 追加式 `SessionEvent` 日志 + 内存存储 | `ctx.sessions` |
| `core/system-prompt` | 提示段与工具 schema 装配 | `ctx.systemPrompt` |
| `core/tools` | 作用域工具注册表 + 受守卫的执行管线 | `ctx.tools` |
| `core/agent` / `core/agent-loop` | Agent 接口、注册表与默认驱动 | `ctx.agents` / `ctx.agentLoop` |
| `llm/llm` | 消息与流词汇、适配器 seam | `ctx.llm` |
| `core/scope` | per-agent 作用域注册原语 | 无键（库） |

- **三层事件域**（这是 DSH 扩展模型的核心设计，值得整段吸收为词汇）：
  1. **Session events**——追加到日志的持久事实，经 `session/event` 广播，"必须活过 reload 的事实才用它"；
  2. **Agent events**（`agent/*`）——携带活 Agent 的进行中事件（inbox、step、status、request、validation、continuation）；
  3. **Capability events**（`fs/*`、`tools/*`、`telemetry/*`）——向 seam 挂策略与适配器，不接触 loop。
- **Turn/step 流水**（architecture.md 原文流程图）：durable 事件为 `turn/*`、`step/*`、`user/message`、`assistant/message`、`assistant/attempt`、`tool/*`；`agent/pre-step`、`agent/request`、`llm/stream`、`tools/pre-execute|execute|post-execute` 是**瀑布**（监听者必须 `next()` 委托）；`agent/turn-stopping` 串行无 `next()`。模型历史**从日志派生**、从不单独存储，replay 即重新派生。

## 3. 候选 B 相关：SessionEvent 词汇（Halo 已开工方向）

来源 `packages/core/session/src/types.ts`（docs/subsystems/session.md 引述）：

- `SessionEventMap`（merge-extensible，插件可声明合并新事件类型，如 `compaction/start|summary|end`、hook 桥的 log-only `hook/invoked|result`）核心成员：
  - `turn/start {turn}` / `turn/end {turn, reason: TurnEndReason}`（空 turn 无 step 事件）
  - `step/start|end {turn, step}`——一步 = 一次模型请求 + 它调用的工具
  - `user/message`——模型可见输入的持久表示，`source` 区分真人/注入上下文/goal 续跑轮
  - `assistant/message`——内嵌**精确压缩模型流** `AssistantStreamRecord[]` + 可选 `usage` + `interrupted: true` 标记（取消时落地已交付前缀，无需从 turn 边界重推中断）
  - `assistant/attempt`——未产生表面消息的模型尝试（失败/重试/流错误），保留不虚构历史
  - `tool/call {callId, name, arguments 原样未解析}` / `tool/result {message, error?, meta?}`——`meta` 工具私有展示载荷，`Session.append` 用 `isJsonValue` 运行时校验
  - `request/header`、`request/context`——log-only 请求头重建
- 生成的 [persistence-catalog](../../deepseek-harness/docs/persistence-catalog.md) 枚举全部成员（含合并项）及其载荷、surface badge、声明点。
- **对 Halo 的直接对照**：Halo 正在落地的 `managed_event_facts`（受管事实持久化，见 2026-09 的两个 feat 提交）与该模型的同构点在于 append-only 事实 + 派生投影；DSH 的增量价值主要在 `interrupted`/`attempt` 语义（避免虚构历史）、事件域三分法、以及"瀑布拦截点"词汇。

## 4. 候选 C 相关：Approval 与 Sandbox 契约

**User approval**（`packages/interaction/user-approval`，docs/subsystems/approval.md）：

- 封闭结局枚举 `ApprovalOutcome = 'allowed-once' | 'rejected' | 'cancelled' | 'unavailable'`，**fail-closed**：缺失/非属主/抛错的 answerer 一律落 `unavailable`；调用方在 `rejected/cancelled/unavailable` 上拒绝执行。
- 会话级策略 `ApprovalPolicy = 'ask' | 'never'`：生效值取日志中最后一条 `approval/policy` 事件（可重放重建），`setApprovalPolicy` 是唯一写路径；`never` 在服务内部先于瀑布强制，后注册的 answerer 无法绕过。
- 审计对 `approval/asked` + `approval/decided`：log-only，不进模型转录。
- `ApprovalRequest` 故意**不含工具参数**：answerer 通过 `callId` 把提问挂到已流式呈现的 tool call 上，避免第二份漂移副本——这与 Halo「Agent 操作请求」（一次性决议、无持久放行）的语义高度同构，且提供了更干净的防漂移设计。
- 无会话级/永久放行；`allowed-once` 只覆盖被问的那个动作。

**Process sandbox**（`packages/sandbox/sandbox` + `sandbox-local`，docs/subsystems/sandbox.md）：

- `SandboxMode = 'read-only' | 'workspace-write' | 'danger-full-access'`（仅前两者可下发；第三者直接绕过、不调 `ctx.sandbox`）。
- `SandboxEnforcement = 'full' | 'partial'`——**执行完整度是如实上报的事实**：旧 Landlock ABI 与 Windows ACL runner 属于当前 `partial` 案例（Everyone/hard-link 边界），要求绝对边界的消费方必须拒绝或显式呈现该差别。
- 后端：Linux bwrap/Landlock、macOS Seatbelt、**Windows ACL 受限令牌**；`bash-sandbox`/`pwsh-sandbox` 消费该 seam。
- per-call `SandboxExecutionPolicy {mode, workspaceRoot(规范化), sessionId…}`——策略按能力调用解析并携带。
- **对 Halo 的风险事实**：Halo 是 Windows 优先产品（ADR-0040），而 DSH 的 Windows 沙箱自报 `partial` enforcement——提取威胁模型与契约测试可行，直接依赖其 Windows 执行边界不可行。

## 5. 提取面排序（2026-09-05 时点，按 8/18 评估的 A–E 框架）

| 候选 | 时点评估 | 主要新证据 |
|---|---|---|
| **B 事件事实投影**（Halo 已开工） | **Strong，维持** | 事件域三分法、`interrupted`/`assistant/attempt` 不虚构历史语义、persistence-catalog 生成机制；但 **session format migration 刚合入**，跟随上游格式有漂移成本——建议借鉴语义而非跟随格式 |
| **C 审批/沙箱映射** | **Strong，维持但修正范围** | approval 契约完整、fail-closed、审计对；**Windows 沙箱仅 partial enforcement** → 提取威胁模型 + 契约测试，不提取 Windows 执行边界 |
| **D Skills/Workflows/Goals** | Worth exploring，不变 | `packages/skill|workflow|todo|goal|plan` 均在；未见成熟度变化证据（本轮未深查） |
| **A DSH Agent Loop 作第二 Adapter** | Worth exploring，风险上升 | developer preview 警告未撤 + 2561 提交/18 天的漂移速度；SDK/ACP profile 机器应答桥（one-shot machine decisions）是新的参考点 |
| **E 迁移 TS/Cordis 基座** | Speculative，证据未改善 | 无稳定性信号；Rust→TS 重写面未变 |

## 6. 给 wayfinder 决策票的事实清单

1. 「借鉴 SessionEvent 语义、不跟随其持久化格式」应成为候选 B 的约束（migration 事件是直接证据）。
2. 候选 C 的可提取物：`ApprovalOutcome` 封闭枚举 + fail-closed 规则、`approval/asked|decided` 审计对、`callId` 防漂移设计、`SandboxEnforcement` 如实上报模式；不可提取物：Windows 沙箱执行边界（partial）。
3. DSH 的能力事件（`fs/*`、`tools/*`）三层分离可作为 Halo「运行事实 / 活事件 / 策略挂载」的词汇校准器。
4. 若走候选 A（第二 Adapter），DSH 的 `sdk`/`acp` profile 证明其有面向程序化驱动的稳定入口，但版本锚定策略必须先定（当前无 LTS 承诺）。

## 参考

- 本地 checkout：`D:\DeepSeek Harness\deepseek-harness`（HEAD d347e70390, 2026-09-04）
- `docs/architecture.md`、`docs/subsystems/session.md`、`docs/subsystems/approval.md`、`docs/subsystems/sandbox.md`、根 `README.md`、根 `package.json`
- 上游：https://github.com/deepseek-ai/deepseek-harness · 文档站 https://deepseek-harness.github.io/deepseek-harness/
- 既有评估：`docs/architecture/deepseek-harness-assessment-20260818.md`
