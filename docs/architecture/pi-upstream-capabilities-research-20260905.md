# pi 上游能力面研究（2026-09-05）

> 状态：研究输入，服务「去 BitFun 化 + 从 DSH/pi 提取改进」的 wayfinder 决策票
> 主源：`badlogic/pi-mono`（GitHub, MIT）RPC 文档与 README（main @ v0.85.0, 2026-09-04）+ 本地 adapter 源码

## 1. 上游身份与版本漂移（关键事实）

- pi 是 Mario Zechner（badlogic）的 monorepo `github.com/badlogic/pi-mono`："AI agent toolkit: unified LLM API, agent loop, TUI, coding agent CLI"，MIT。
- **npm scope 已迁移**：旧包 `@mariozechner/pi-coding-agent` 停在 **0.73.1**（2026-05-07 后未更新）；现行包是 **`@earendil-works/pi-coding-agent` = 0.85.0**（2026-09-04，与 GitHub release v0.85.0 同日）。README 的 See Also 已全部指向 `@earendil-works/pi-*`。
- monorepo 包结构：`agent`（agent 框架 `pi-agent-core`）、`ai`（LLM 工具包 `pi-ai`）、`coding-agent`（CLI/harness）、`tui`、`protocol`、`server`、`client`、`chord`、`session-backends`、`telemetry`、`evals`。
- **Halo 锚点**：`halo-pi-rpc-adapter/src/lib.rs:62-63` 只支持 profile `0.81.1` / `0.83.0`；上游已到 **0.85.0**——落后约两个 minor。安装来源必须钉死（旧 scope 的 npm 包已死），否则能力探测/版本档案会漂。

## 2. RPC 协议全貌（v0.85，`packages/coding-agent/docs/rpc.md`，1618 行）

framing：stdin/stdout 严格 LF JSONL；官方明确警告 Node `readline` 不合规（会把 U+2028/U+2029 当换行）——与 Halo `framing.rs` 的做法一致。

**命令面（25+，按文档分组）**：

| 组 | 命令 | Halo 是否已消费 |
|---|---|---|
| 提示 | `prompt`、**`steer`**、`follow_up`、`abort`、`clear_queue` | prompt/follow_up/abort 已用；**steer、clear_queue 未用** |
| 会话 | `new_session`（含 `parentSession` 追踪）、`switch_session`、**`fork`**（从分支上任一 user message 分叉，可被扩展 `session_before_fork` 取消）、`get_entries`（`since` 游标）、`get_messages`、`get_session_stats`、`export_html` | get_entries、get_state 已用；**fork/switch/new_session/export/stats 未用** |
| 模型 | `set_model`、`cycle_model`、`get_available_models` | **未用**（Halo 在启动时投影模型配置，ADR-0064） |
| 思考 | `set_thinking_level`、`cycle_thinking_level`、`get_available_thinking_levels` | **未用**（配置里存 thinking level，但运行中不可调） |
| 队列 | `set_steering_mode`、`set_follow_up_mode` | **未用** |
| 压缩 | `compact`、`set_auto_compaction` | **未用** |
| 重试 | `set_auto_retry`、`abort_retry` | **未用** |
| Bash | `bash`、`abort_bash`（宿主侧 shell 逃生门） | **未用** |

**事件面**：`agent_start` / `agent_end` / **`agent_settled`**、`turn_start` / `turn_end`、`message_start` / `message_end`、`message_update`（流式增量）、`bash_execution_update`、`tool_execution_start` / `tool_execution_update` / `tool_execution_end`、`queue_update`、`compaction_start` / `compaction_end`、`auto_retry_start` / `auto_retry_end`、`summarization_retry_*`、`extension_error`。全部命令支持可选 `id` 关联。

**Extension UI 协议**（Halo 已消费的部分）：请求 `select` / `confirm` / `input` / `editor` / `notify` / `setStatus` / `setWidget` / `setTitle` / `set_editor_text`（stdout），应答 value / confirm / cancel（stdin）。Halo 的 `halo_permission_gate.ts`（adapter 内，`HALO_PI_EXTENSION_VERSION=1.0.0`）与 `extension_ui_response`（lib.rs:1425）走的就是这条路。

## 3. 上游扩展模型与信任语义（对 Halo 直接相关）

- 扩展 API：`pi.registerTool()`（**可整体替换内建工具**）、`pi.registerCommand()`、`pi.on("tool_call", …)` 事件拦截、`pi.registerProvider()`、异步工厂（启动等待）。官方列出的用例包括 permission gates、git checkpointing、sandbox/SSH 执行、MCP 集成——Halo 的权限门是其中一种合法形态。
- **Project Trust 语义**（README → Settings → Project Trust）：非交互模式（`-p`、`--mode json`、`--mode rpc`）不弹信任提示，回落 `defaultProjectTrust`；`ask`/`never` **不加载项目资源**，`always` 才信任。这与 Halo 工作区信任模型（ADR-0021、0031）同向：受管会话应显式钉住该配置，而不是依赖用户全局设置。
- Session 存储：JSONL **树结构**（每条 `id`/`parentId`），支持原地分叉（`/tree`）；压缩在上下文里有损，但完整历史保留在 JSONL。会话按工作目录存放于 `~/.pi/agent/sessions/`。
- Provider 广度：30+ API key 供应商（Anthropic/OpenAI/Google/DeepSeek/ZAI/MiniMax/Kimi 等）+ 订阅登录（Claude Pro/Max、ChatGPT Codex、Copilot）+ llama.cpp 本地路由 + `models.json` 自定义；thinking level off→max。

## 4. 对 Halo Studio 的提取面（按受管交付链 leverage 排序）

1. **`steer`（运行中转向）**：在主执行器运行中插入纠正，语义是"当前 assistant 回合的工具调用跑完后、下次 LLM 调用前"投递——比 `follow_up`（回合后排队）更适合「等待开发者」态的任务纠正 UX；`set_steering_mode` 控制处理方式。
2. **`agent_settled` / `queue_update` 事件**：比从 turn 边界推断「主执行器已结束/队列空」更可靠，直接改善任务终态与「等待开发者」判定。
3. **运行中模型/思考级控制**（`get_available_models`、`set_model`、`set_thinking_level`）：把 ADR-0064 的共享模型配置服务从"启动时投影"升级为"运行时可切换"，同时不破坏归因（事件里带模型信息）。
4. **`fork` / `new_session`（parentSession）**：任务重试/续办的归因链——从指定 user message 分叉，父会话可追踪，配合交付证据版本（ADR-0075 语义）可表达"重试证据版本"而不丢基线。
5. **`compact` / `set_auto_compaction`**：长受管任务（多轮追问）的上下文管理；注意 Halo 的「活动会话记录」脱敏/限长规则要叠加在 compaction 事件投影上。
6. **`get_session_stats` / `export_html`**：交付证据包的候选输入，但**必须过脱敏层**（ADR-0042/0043/0048），原文导出不可直接入库。
7. **`prompt`/`steer` 的 `images` 附件**：「任务说明」可携带图片素材（需要 Halo 侧大小与内容策略）。
8. **版本档案维护**：上游 0.85.0 已出而 Halo 档案停在 0.83.0；npm scope 迁移（@mariozechner → @earendil-works）意味着安装源探测逻辑（`build_pi_command` + `--version`）要同时识别两种来源，发布物应钉版本。

## 5. 风险

- 上游无 LTS/稳定承诺，RPC 文档随 main 演进；Halo 的 profile 档案机制（每版本独立档案 + fail-closed）是正确防线，成本是每个上游 minor 都要做一次档案验收。
- `bash` 命令是宿主侧 shell 逃生门，受管模式下应保持不消费（避免绕过 Halo 的高风险操作决议）。
- 扩展可整体替换内建工具——受管会话必须只加载 Halo 自己的 `halo_permission_gate.ts`，并依赖 Project Trust 语义挡住工作区扩展（ADR-0011/0031-0033）。

## 参考

- RPC 协议：https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/rpc.md（v0.85.0）
- 上游 README：https://github.com/badlogic/pi-mono（packages/coding-agent/README.md）
- npm：`@earendil-works/pi-coding-agent` 0.85.0（2026-09-04）；旧包 `@mariozechner/pi-coding-agent` 0.73.1（停更）
- 本地消费面证据：`product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/lib.rs`（:62-63 版本档案，:755/:764/:778/:793/:1126/:1425 命令用法）、`docs/development/pi-rpc-adapter.md`、`docs/architecture/pi-first-party-extension-inventory.json`
