# 14 — Pi/OpenCode 真实协议对齐（适配器协议 v2）

**状态：** 设计完成，待落地
**依据：** `requirements-alignment/03-ide-editor-and-reference-alignment.md`（范围内第 4 条）、`docs/design/references/R3-opencode-analysis.md`、`docs/design/references/R4-pi-analysis.md`
**对照现状：** `docs/module-contracts.md` 第 5、7 节；`sidecar/crates/halo-runtime`（pi.rs / opencode.rs / process.rs / lib.rs）；`sidecar/crates/halo-testkit`（fake-pi / fake-opencode）；`sidecar/crates/halo-sidecar`（dispatch.rs / state.rs / task_flow.rs）

---

## 1. 目标与范围

### 1.1 目标

把 halo-runtime 的两个适配器从**内部假设协议 v1** 修订为**与真实开源实现一致的协议 v2**：

1. 对 R3/R4 差异表的每一处差异给出明确裁决（采纳真实协议 / 保留简化并说明理由），产出可直接替换 `docs/module-contracts.md` 第 5 节的适配器协议规范全文（附录 A）与第 7 节全文（附录 B）。
2. `RuntimeEvent` 对上层的形状**保持不变**：`State / Trace / ActionRequest / Verification / TaskDone` 五个变体的字段一字不改，`halo-sidecar` 的 task_flow 主循环不动，只做本设计明确列出的最小增量。
3. halo-testkit 两个假进程改讲真实协议 v2，全部现有集成测试场景在 v2 话术下保持覆盖（允许按 v2 语义修订断言，覆盖面不缩水）。
4. 凭据注入方案按 R4 §5.2 的真实环境变量名修订：`LaunchConfig` 可显式指定注入变量名，缺省按 provider 默认表推导。

### 1.2 受管边界（硬约束，逐条确认不变）

| 边界 | v2 落实方式 |
| --- | --- |
| 仅回环 | OpenCode 仍 `serve --hostname 127.0.0.1 --port <Sidecar 选定端口>`；fake-opencode 拒绝非 127.0.0.1 绑定；Pi 纯 stdio 无网络面。 |
| 每次启动新认证 | 32 字节随机 hex 的生成逻辑保留，注入变量由 `HALO_OC_TOKEN` 改为 `OPENCODE_SERVER_PASSWORD`（真实变量名），请求头由 Bearer 改为 `Basic base64("opencode:<hex>")`。 |
| 精确版本握手 | 真实协议无独立 `/version` → 替代握手：`GET /global/health` 同一响应中的 `version` 字段与 `OPENCODE_LOCKED_VERSION` **全等**比较，纪律不变（见 4.3.2）。 |
| 优雅停止 + 强杀兜底 | Pi：关 stdin = 官方优雅退出（exit 0），宽限后强杀。OpenCode：真实协议**无 shutdown 端点**，官方停止方式即杀进程 → 语义重定义见裁决 OC-9，Graceful/Forced 二值语义保留。 |
| 端口/认证信息不进公开状态 | `OcShared` 的 `port`/`password` 仍为私有字段；Debug/错误 message/事件 payload 一律不出现；现有 canary 测试（`debug_and_errors_never_leak_port_or_token`、`credential_canary`、`contains_lower_hex_run` 探测）全部保留。 |
| 凭据红线 | 绝不使用 Pi `--api-key`（命令行对其他进程可见）、绝不写 `auth.json`（明文落盘）；OpenCode 绝不以无密码模式启动。 |

### 1.3 范围外

- 不改 IPC v1 既有消息形状（只做追加：新方法 `task.resolve_action`、`LaunchConfigInput` 新可选字段，见第 3 章）。
- 不接入两个真实应用的高级能力：Pi 的 steer/follow_up、会话树（fork/clone/switch）、compaction 手动触发、bash 注入、`get_messages/get_entries/get_tree` 全量命令、模型循环；OpenCode 的多会话并发、fork/revert/share/summarize/todo、subtask part、`permission reply=always`、pty/mcp/vcs/worktree、mDNS、多目录实例（我们仍一工作区一进程）。
- 不做 UI 改版：`task.resolve_action` 的界面按钮挂点归 10 号（壳层归位）与 15 号（差异化裁决），本文档只交付 Sidecar 侧通道。
- 不引入 async runtime：仍为线程 + crossbeam-channel。

---

## 2. 参考结论引用

| 引用 | 借鉴什么 | 不借鉴什么 |
| --- | --- | --- |
| R3 §1（服务形态）、§2（会话/消息/SSE）、§3（轮次生命周期）、§5（权限/提问）、§6（差异表）、§7（适配器建议） | 启动参数、Basic 认证与 `OPENCODE_SERVER_PASSWORD`、`/global/health` 握手、session+prompt_async 任务映射、SSE 消费与 idle 防抖（`stream.transport.ts` 同款复核策略）、abort/dispose/杀进程停止语义、permission/question 事件与 reply 端点 | 多实例路由的多目录用法（只取「显式带 directory」纪律）；`session.idle` 弃用事件不作结束权威；OpenCode 源码一行不抄 |
| R4 §1（进程形态）、§2（封包/命令/事件/扩展 UI）、§3（get_state/版本）、§4（取消停止）、§5（配置注入）、§6（差异表）、§7（适配器建议） | `--mode rpc` 平铺命令话术、`get_state` 空闲判定字段、prompt 接受语义与响应乱序容忍、`agent_settled` 终态信号、`get_last_assistant_text` 取摘要、关 stdin 优雅退出、凭据 env 名映射表、`PI_CODING_AGENT_DIR` 隔离、16MB 入站行上限 | Pi 会话持久化（`--no-session` 关闭）、扩展生态（`--no-extensions` 关闭，仅保留 extension_ui_request 帧的映射能力）、`--api-key`/auth.json 注入方式；Pi 源码一行不抄 |
| R1/R2/R5 | 本文档无直接引用（协议对齐与 UI/分层无涉） | — |

---

## 3. 与现有契约的关系（契约增量，逐条）

### 3.1 对 `docs/ipc-protocol.md` 的增量（全部为**追加式**，v1 兼容）

| # | 增量 | 内容 |
| --- | --- | --- |
| I-1 | 新方法 `task.resolve_action`（3.4 节追加） | params `{"task_id":"task-…","request_id":"…","decision":"approve_once"\|"reject"\|"answer","message":"…\|null","answers":[["选中label"]]\|null}` → result `{"accepted": true}`。约束：任务须处于 `awaiting_action` 且 `request_id` 等于当前未决请求，否则新错误码 `ACTION_REQUEST_NOT_FOUND`；`decision` 与请求 kind 不匹配（如对 permission 用 answer）→ `INVALID_PARAMS`。**不暴露 `always`**（Halo 不替用户固化放行规则）。语义说明：serve 模式下 OpenCode 的 HTTP reply 端点就是其原生通道（官方 TUI/CLI 同一 API），`task.action_request` 事件的 `channel:"native"` 字段与文案保持不变。 |
| I-2 | `LaunchConfigInput` 追加可选字段 | `"credential_env_var": "ANTHROPIC_API_KEY" \| null`。null = 按 provider 默认表推导（见 4.7）；显式值须匹配 `^[A-Z][A-Z0-9_]{2,63}$`、不得与环境白名单冲突、不得为保留名 `OPENCODE_SERVER_PASSWORD`/`OPENCODE_SERVER_USERNAME`/`PI_CODING_AGENT_DIR`/`PI_SKIP_VERSION_CHECK`，违规返回 `INVALID_PARAMS`。 |
| I-3 | 错误码追加 | `ACTION_REQUEST_NOT_FOUND`（第 5 节清单追加一项）。 |
| I-4 | `task.action_request` 事件语义补注 | payload 形状不变；补充一句：「请求可经 `task.resolve_action` 决议；Agent 恢复输出或适配器收到对端 replied 信号时自动回到 running」。 |
| I-5 | `thinking_level` 枚举 | **不变**（`off/low/medium/high` 四档，见裁决 PI-14）。 |

`protocol/v1/envelope.schema.json` 只约束封包形状，无需改动。

### 3.2 对 `docs/module-contracts.md` 的增量

| # | 节 | 增量 |
| --- | --- | --- |
| M-1 | 第 3 节（halo-config） | `LaunchConfig` 增字段 `credential_env_var: Option<String>`；新增 `DEFAULT_CREDENTIAL_ENV` 常量表与 `resolve_credential_env_var(cfg) -> Result<String, ConfigError>`；`validate_launch_config` 增加该字段校验；新增错误变体 `ConfigError::CredentialEnvUnresolved`。详见 4.7。 |
| M-2 | 第 5 节（halo-runtime） | **整节替换**为附录 A 全文。 |
| M-3 | 第 6 节（halo-sidecar） | dispatch 增 `task.resolve_action` 路由；`CREDENTIAL_ENV_VAR` 常量删除，改用 `halo_config::resolve_credential_env_var`；`AgentHandle` trait 增 `resolve_action`；`ActiveTask` 增 `pending_action` 字段。详见 4.10。 |
| M-4 | 第 7 节（halo-testkit） | **整节替换**为附录 B 全文。 |
| M-5 | `docs/architecture.md` 决策 8 | 文字微调：`HALO_OC_TOKEN`/Bearer → `OPENCODE_SERVER_PASSWORD`/Basic；TOCTOU 结论不变（端口被抢占最多握手失败，冒充进程拿不到密码）。 |
| M-6 | `docs/traceability.md` | 追加 v2 对齐条目（实施时更新）。 |

---

## 4. 详细设计

### 4.1 差异裁决表 — Pi（对照 R4 §6，15 项）

| # | 维度 | 裁决 | 说明 |
| --- | --- | --- | --- |
| PI-1 | 启动参数 | **采纳真实协议** | `<exe> --mode rpc --no-session --no-approve --no-extensions [--thinking <level>] [--model <provider>/<model_id>]` + `extra_args`。`--no-session`：任务级隔离，不写 `~/.pi/agent/sessions`；`--no-approve`：显式拒绝项目级 `.pi` 资源信任（防工作区内恶意扩展）；`--no-extensions`：v2 协议面最小化。 |
| PI-2 | 封包 | **采纳真实协议** | 平铺命令 `{"id?":"<string>","type":"<命令>",…字段}`；`id` 由 u64 改为 String（`"halo-<用途>-<uuid4>"`）。 |
| PI-3 | 响应形状 | **采纳真实协议** | `{"id?","type":"response","command","success",data?/error?}`；按 `type=="response"` 识别响应，按 `id` 路由，未知 id/乱序容忍（现纪律保留）。 |
| PI-4 | 就绪探测 | **采纳真实协议**（机制保留，字段改判） | 真实 Pi 启动无握手输出 → 「发 `get_state` 等响应」的机制本来就成立。判定改为：`command=="get_state" && success==true && data.isStreaming==false && data.isCompacting==false`；不再匹配 `state:"idle"` 字符串。超时/非空闲/失败路径语义不变。 |
| PI-5 | 任务提交 | **采纳真实协议** | `run_task` 方法不存在 → 单条 `{"type":"prompt","id":"halo-task-<uuid>","message":<模板拼装>}`。模板 = instructions + 关注文件清单 + base_diff（围栏包裹）+ notes（见 4.5.2）。不使用 steer/follow_up。 |
| PI-6 | 任务完成 | **采纳真实协议** | prompt 响应 `success:true` 仅表示已接受（不产生 TaskDone）；`success:false` → 立即 `TaskDone{outcome:"failed"}`。终态信号 = `agent_settled` 事件；随后发 `get_last_assistant_text` 取 summary，按最近 assistant `stopReason` 判 outcome（`stop`/`toolUse`→finished、`aborted`→cancelled、`error` 或 `auto_retry_end{success:false}`→failed）。 |
| PI-7 | 事件流 | **采纳真实协议** | 顶层 `type` 平铺的 `AgentSessionEvent`；映射表见 4.5.3；未知 `type` 一律忽略（前向兼容红线）。 |
| PI-8 | 权限请求 | **采纳真实协议 + 生产静默** | 适配器完整实现 `extension_ui_request`（select/confirm/input/editor）→ `ActionRequest` 映射与 `extension_ui_response` 回写（`resolve_action`），保证协议完备与测试场景保留；但生产启动带 `--no-extensions`，真实 Pi v2 路径不会产生该帧（Pi 核心无权限系统，工具直接执行，事后由基线归因+审查把关）。通知类（notify/setStatus/…）→ 忽略。 |
| PI-9 | 取消 | **采纳真实协议** | `{"type":"abort","id":…}` → 流以 `error(reason:"aborted")` 收尾并必出 `agent_settled` → `TaskDone{outcome:"cancelled"}`。task_flow 的取消宽限/强杀语义不变。 |
| PI-10 | 退出 | **采纳真实协议** | 无 exit 命令；`stop(grace)` = （任务运行中先发 abort）+ **关 stdin**（Pi 收 EOF 优雅退出 exit 0）→ 宽限内退出 = Graceful；超时 kill = Forced。SIGTERM 在 Windows 不可靠，不用。 |
| PI-11 | 版本探测 | **一致，保留** | `--version` → 裸 semver 首行；`probe_version`/`parse_semver_token` 不动。Pi 不做精确版本锁定（现状保留，锁定纪律仅 OpenCode）。 |
| PI-12 | 帧纪律 | **采纳真实协议（上限分层）** | 读取器仅按 LF 切分、剥尾部 `\r`、不把 U+2028/2029 当换行（serde_json 天然满足，读取器不得用按码点断行的实现）；Pi 适配器入站行上限独立设 `PI_MAX_LINE_BYTES = 16 * 1024 * 1024`；超限 → `Failed{PI 输出行超过上限}`。`halo_protocol::MAX_LINE_BYTES(1MiB)` 只管 UI↔Sidecar IPC，互不相干。规避大帧：v2 只用 `get_state`/`get_last_assistant_text`，禁用 `get_messages/get_entries/get_tree`。 |
| PI-13 | 凭据注入 | **采纳真实变量名** | `credential_ref` 的明文注入到 `resolve_credential_env_var(cfg)` 解析出的变量（如 `ANTHROPIC_API_KEY`）；另注入非凭据常量 `PI_CODING_AGENT_DIR=<HALO_DATA_DIR>\pi-agent`（隔离用户全局 auth.json/extensions/settings）与 `PI_SKIP_VERSION_CHECK=1`（削减启动出网）。见 4.7。 |
| PI-14 | thinking level | **保留简化** | IPC v1 的 `off/low/medium/high` 四档不破坏，同名直传 `--thinking <level>`；Pi 的 minimal/xhigh/max 三档不暴露。理由：追加枚举值属 IPC 追加式演进，可留待后续需求，本轮不必绑定。 |
| PI-15 | 会话模型 | **保留简化** | `--no-session` 纯内存会话，一个 Agent 任务 = 一次进程内单条 prompt；会话续跑/受管 `--session-dir` 留待后续版本。理由：与「单工作区单任务、交付证据为界」的产品模型一致。 |

### 4.2 差异裁决表 — OpenCode（对照 R3 §6，12 项）

| # | 维度 | 裁决 | 说明 |
| --- | --- | --- | --- |
| OC-1 | 启动命令 | **一致，保留** | `<exe> serve --hostname 127.0.0.1 --port <p>`，`p` 仍由 Sidecar 选空闲端口（不用 `--port 0`，端口权威保持在 Sidecar 侧）。 |
| OC-2 | 就绪确认 | **采纳真实协议（双重判定）** | ① 读子进程 stdout 就绪行 `opencode server listening on http://<host>:<port>`（正则 `on\s+(https?://\S+)`），**校验端口与 Sidecar 指定值一致**，不一致 → `Failed{端口不一致}`；② `GET /global/health` 返回 200 且 `healthy==true`。两者都在 `Timeouts.ready` 内完成才算就绪。就绪行同时是端口被抢占（TOCTOU）时的快速失败信号。 |
| OC-3 | 认证 | **采纳真实协议** | 每次启动生成 32 字节随机 hex（逻辑复用），经 `OPENCODE_SERVER_PASSWORD` 注入（凭据类，不进日志/IPC）；用户名固定默认值 `opencode`；所有请求带 `Authorization: Basic base64("opencode:<hex>")`。401 → `RuntimeError::Unauthorized` 失败关闭（现语义保留）。**无密码启动路径在 Halo 内不存在**。 |
| OC-4 | 健康检查 | **采纳真实协议** | `GET /health` 不存在 → `GET /global/health` → `{"healthy":true,"version":"…"}`。 |
| OC-5 | 版本握手 | **采纳真实协议（全等纪律保留）** | 无独立 `/version` → 版本取 `/global/health` 响应的 `version` 字段，与 `OPENCODE_LOCKED_VERSION` 全等比较，不匹配 → `Failed{RUNTIME_VERSION_MISMATCH}`。锁定常量由 `"0.4.2"`（占位）改为 `"1.18.4"`（参考源码值，**验收时按实际装机二进制重锁**）。启动前 CLI `--version` 探测 + 全等预检（dispatch 现逻辑）保留。 |
| OC-6 | 任务提交 | **采纳真实协议** | `POST /task` 不存在 → `POST /session`（body `{"title":<任务标题>}`）记 `sessionID` → `POST /session/{id}/prompt_async`（204 受理），parts = text part（模板同 Pi）+ files 映射为 file part；`base_diff` 并入 text part（OpenCode 无独立 diff 输入通道）。所有请求显式带 `?directory=<工作区真实路径>`（percent 编码），不依赖服务端 cwd。**不注入 permission 规则集**：沿用 OpenCode 原生默认规则（`ask` 命中才产生权限请求），符合「按自身原生权限模型」。 |
| OC-7 | 事件流 | **采纳真实协议** | 长轮询 `/events?after=n` 不存在 → SSE `GET /event?directory=…`：Ready 后立即建立专用线程长连接，阻塞读、空行分帧、取 `data:` 载荷 JSON；忽略 `server.heartbeat` 与未知 `type`；按 `sessionID` 过滤本会话事件。无序号无重放 → 断线重连一次 + 快照端点（`/session/status`、`/permission`）重建；重建失败 → `Failed`。映射表见 4.6.3。 |
| OC-8 | 取消 | **采纳真实协议** | `POST /cancel` 不存在 → `POST /session/{id}/abort`；随后流上出现 `MessageAbortedError` 与 `session.status(idle)` → 结束判定给出 `TaskDone{outcome:"cancelled"}`。task_flow 取消宽限/强杀语义不变。 |
| OC-9 | 优雅停止 | **采纳真实协议（语义重定义）** | `/shutdown` 不存在；官方停止方式 = 杀进程（Windows `taskkill /T /F` 同义）。v2 `stop(grace)` = ①任务未终局先 abort → ②`POST /global/dispose`（释放实例、服务端结束事件流）→ ③关闭 SSE 线程 → ④终止子进程（kill+wait，现 `RealChild::kill` 已含 wait）。**`StopOutcome::Graceful` = abort/dispose 阶段均成功送达（服务有机会收敛后按官方语义终止）；`Forced` = 任一请求失败/超时（服务失联，直接强杀）**。二值语义与「原生优先、超时兜底」精神保留。 |
| OC-10 | 权限/操作请求 | **采纳真实协议** | 自造 `action_request` 事件 → `permission.asked`（→`ActionRequest{kind:"permission"}`）与 `question.asked`（→`ActionRequest{kind:"clarification"}`）；决议经新 `resolve_action` 通道：permission → `POST /permission/{id}/reply`（只暴露 `once`/`reject`+message，**不暴露 `always`**）；clarification → `POST /question/{id}/reply`（answers）或 `/reject`。`permission.replied`/`question.replied` 事件 → `Trace{kind:"lifecycle"}`（触发编排层 ActionResolved，机制沿用「Agent 恢复输出即已决」）。 |
| OC-11 | 结果表达 | **采纳真实协议** | 长轮询 `done/outcome` 不存在 → 结束判定：收到本会话 `session.status(idle)` 后 **复核 `GET /session/status`**（防旧轮次迟到 idle）→ `GET /session/{id}/message?limit=1` 取末条 assistant message：`error` 空且 `time.completed` 有值 → `finished`（summary=末条 text parts 拼接，cap 后交上层）；`error.name=="MessageAbortedError"` → `cancelled`；其余 error / 已缓存 `session.error` → `failed`。`GET /session/{id}/diff` 仅可选诊断，**任务关联变更的权威仍是 Git 基线算法**（module-contracts §6），v2 不消费。 |
| OC-12 | 实例模型 | **保留简化** | 一进程多目录实例是真实能力，但我们仍每工作区一进程、每请求显式 `directory`；不接 `x-opencode-directory` 头（统一用查询参数，语义等价且 SDK 同款）。 |

### 4.3 就绪/失败语义映射（真实协议缺失能力的替代信号）

| 我们假设的能力 | 真实缺失情况 | v2 替代信号 |
| --- | --- | --- |
| Pi `get_state → state:"idle"` | 无 `state` 字段 | `success && !isStreaming && !isCompacting`（严格空闲另看 `pendingMessageCount==0`，就绪检查不要求——刚启动必为 0） |
| Pi 启动握手输出 | 无任何 banner/ready 帧 | 命令-响应即握手：启动后立刻发 `get_state`，`Timeouts.ready` 内收到合法响应即握手成功；进程早退（EOF）+ stderr 含凭据/模型指引 → `Failed{PI_NO_MODEL_OR_CREDENTIAL}` |
| Pi per-task 终局响应 | prompt 响应仅表示受理 | `agent_settled` 事件为唯一终态信号；响应与事件乱序容忍 |
| OpenCode `/health` | 不存在 | `GET /global/health` 200 且 `healthy==true`；同响应携带版本 |
| OpenCode `/version` 独立端点 | 不存在 | `/global/health.version` 全等握手（替代握手方案）；CLI `--version` 预检保留 |
| OpenCode `/shutdown` | 不存在 | abort + `/global/dispose` + 杀进程（裁决 OC-9） |
| OpenCode 事件序号/重放 | SSE 无序号无重放 | 断线重连一次 + `GET /session/status` / `GET /permission` 快照重建；idle 结束判定一律复核 `/session/status` |
| 任务失败的实时信号 | 两侧均无 outcome 字段 | Pi：`stopReason:"error"`、`auto_retry_end{success:false}`；OpenCode：`session.error` 事件缓存 + 末条 assistant `error` |
| Agent 验证结论（verification） | **两个真实协议均无验证结论语义** | 协议路径**不再产生** `RuntimeEvent::Verification`；变体保留（形状锁定），生产来源只剩用户 `task.mark_verification`（not_run）与 task_flow 缺省「Agent 未报告验证结果」。诚实呈现，不做工具输出推断（避免伪造验证结论）。测试影响见 7.3。 |

### 4.4 类型与签名（halo-runtime，公共面变更）

```rust
// lib.rs — RuntimeState / RuntimeTraceItem / RuntimeEvent / StopOutcome / Timeouts / RuntimeError 全部不变。
// map_trace_event 删除（v1 同构事件映射已无对象），映射逻辑下沉到各适配器模块。

/// 启动命令：新增三个由 dispatch 从 LaunchConfig/数据目录填充的可选字段（均非凭据）。
pub struct LaunchCmd {
    pub exe: String,
    pub args: Vec<String>,                 // = LaunchConfig.extra_args，附加在适配器固定参数之后
    pub env: HashMap<String, String>,      // halo-config 构好（白名单 + overrides + 凭据注入）
    pub cwd: String,                       // 受信任工作区真实路径
    pub model: Option<String>,             // "provider/model_id"；Pi → --model；OpenCode → prompt_async.model
    pub thinking_level: Option<String>,    // "off"|"low"|"medium"|"high"；Pi → --thinking；OpenCode 首期不用
    pub private_home: Option<String>,      // Pi 专用：PI_CODING_AGENT_DIR 指向的受管目录
}

/// 任务输入：新增 title（OpenCode 会话标题；Pi 忽略）。
pub struct RunTaskSpec {
    pub title: Option<String>,
    pub instructions: String,
    pub files: Vec<String>,
    pub base_diff: Option<String>,
    pub notes: Option<String>,
}

/// Agent 操作请求的决议（task.resolve_action 的 runtime 自有形状）。
pub enum ActionDecisionKind { ApproveOnce, Reject, Answer }
pub struct ActionDecision {
    pub kind: ActionDecisionKind,
    pub message: Option<String>,           // Reject 时给 Agent 的纠正反馈
    pub answers: Option<Vec<Vec<String>>>, // Answer 时与 questions 顺序对应的选中 label 数组
}

impl PiHandle {
    pub fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError>;
    /// v2：回写 extension_ui_response（--no-extensions 下生产不触达，协议完备 + 测试可达）
    pub fn resolve_action(&self, request_id: &str, decision: &ActionDecision) -> Result<(), RuntimeError>;
    pub fn cancel_native(&self);           // {"type":"abort"}
    pub fn stop(&self, grace: Duration) -> StopOutcome;   // abort? + 关 stdin + 宽限 + kill
    pub fn state(&self) -> RuntimeState;
}

impl OpenCodeHandle {
    pub fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError>;   // POST /session + prompt_async
    pub fn resolve_action(&self, request_id: &str, decision: &ActionDecision) -> Result<(), RuntimeError>;
    pub fn cancel_native(&self);           // POST /session/{id}/abort
    pub fn stop(&self, grace: Duration) -> StopOutcome;   // abort? + dispose + SSE 收尾 + kill
    pub fn state(&self) -> RuntimeState;
}

pub const OPENCODE_LOCKED_VERSION: &str = "1.18.4";   // 验收时按装机二进制重锁
pub(crate) const PI_MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
```

新增内部模块：

- `framing.rs`：`read_line_lf(reader, &mut buf, max_bytes) -> io::Result<Option<usize>>` — 仅按 `\n` 切分、剥尾部 `\r`、超限返回专用错误；Pi 读线程专用（16MB），单测覆盖长行/CRLF/含 U+2028 字符串。
- `encoding.rs`：`base64_encode(&[u8]) -> String`（标准字母表，Basic 头用）与 `percent_encode(&str) -> String`（RFC3986 unreserved 之外全部转 %XX，directory 参数用）；各约 25 行 + 单测。不新增外部依赖。

### 4.5 Pi 适配器 v2 数据流

#### 4.5.1 启动与就绪

```
PiRuntime::start(cmd, tx, opts)
 ├─ 组参：--mode rpc --no-session --no-approve --no-extensions
 │        [--thinking <cmd.thinking_level>] [--model <cmd.model>] + cmd.args
 ├─ env：cmd.env（白名单+凭据）；适配器追加 PI_SKIP_VERSION_CHECK=1、
 │        PI_CODING_AGENT_DIR=<cmd.private_home>（Some 时）
 ├─ stdio：stdin/stdout piped；stderr piped → 诊断线程（环形保留末 8KiB，仅入 Failed reason 摘录，
 │        截 200 字节 + 仅可打印字符；出口在 sidecar 层统一再过 sanitize）
 ├─ 读线程：framing::read_line_lf（16MB 上限）→ handle_frame
 └─ 就绪：发 {"type":"get_state","id":"halo-ready-<uuid>"}
          等 response(command=get_state, success) 且 !isStreaming && !isCompacting → Ready
          超时/EOF/坏帧/success=false → Failed（reason 话术沿用现有中文文案骨架）
```

#### 4.5.2 run_task 与 message 模板

```
{"type":"prompt","id":"halo-task-<uuid>","message":<TEMPLATE>}

TEMPLATE（固定拼装，模块内常量函数 build_task_message(spec) -> String）:
  【任务目标】\n{instructions}\n
  （files 非空）【关注文件】\n- {file}…\n
  （base_diff 有值）【已有变更（基线 diff）】\n```diff\n{base_diff}\n```\n
  （notes 有值）【补充说明】\n{notes}
```

- pending 路由表 `HashMap<String, Pending>`，`Pending` 变体改为：`Reply(Sender<Value>)`（就绪检查 / get_last_assistant_text）、`PromptAccept`、`Abort`。
- `PromptAccept` 响应：`success:false` → `TaskDone{outcome:"failed", summary:error 文本}` 并清理任务态；`success:true` → 仅标记已受理。**响应可能晚于首批事件**：任务态跟踪不依赖响应先行。

#### 4.5.3 事件映射（AgentSessionEvent → RuntimeEvent）

| Pi 事件 `type` | RuntimeEvent |
| --- | --- |
| `agent_start` | `Trace{kind:"phase", text:"started", detail:{"phase":"started"}}` |
| `turn_start` / `turn_end` | `turn_end` 记录 `message.stopReason` 入任务态；不发事件（turn 粒度对 UI 无额外信息） |
| `message_update`（`text_delta`/`thinking_delta`） | 按块聚合缓冲；`text_end`/`thinking_end`/`turn_end` 时冲刷为 `Trace{kind:"agent_note", text:<cap 4KiB>, detail:{"block":"text"\|"thinking"}}`（**不逐 delta 发事件**，防事件风暴） |
| `message_update`（`error`, reason aborted/error） | 记录终局线索（aborted → cancelled 候选；error → failed 候选） |
| `tool_execution_start` | `Trace{kind:"tool", text:<toolName>, detail:{"call_id","args 摘要(限长)"}}` |
| `tool_execution_end` | `Trace{kind:"tool", text:"<toolName> "+(isError?"失败":"完成")}`；`toolName ∈ {edit,write}` 时另发 `Trace{kind:"file_hint", text:<args.path>, detail:{"path","change":"touched"}}`（仅 UI 提示，归因权威仍是 Git 基线） |
| `tool_execution_update` | 忽略（partialResult 为累积值，逐条转发即风暴） |
| `compaction_start/end`、`auto_retry_start/end` | `Trace{kind:"phase", text:"压缩中"/"重试中"…}`；`auto_retry_end{success:false}` 记 failed 候选 |
| `extension_ui_request`（select/confirm/input/editor） | `ActionRequest{request_id:<id>, kind:"permission", prompt:<title+"\n"+message>}`；`method` 与 `options` 存入 handle 内 `pending_ui: HashMap<id, UiMeta>` 供 resolve 回写 |
| `extension_ui_request`（notify/setStatus/setWidget/setTitle/set_editor_text） | 忽略 |
| `extension_error` | `Trace{kind:"lifecycle", text:"扩展错误：…", detail:{…}}` |
| `agent_settled` | 终局收口：发 `{"type":"get_last_assistant_text","id":"halo-summary-<uuid>"}`（`Reply` pending，2s 超时容忍失败）→ `TaskDone{outcome, summary}`；outcome 判定：已见 aborted → `"cancelled"`；已见 error/auto_retry 失败 → `"failed"`；其余 → `"finished"`；summary 取 `data.text`（取不到时空串，由上层落缺省文案） |
| 其他/未知 `type` | 忽略（含 `queue_update`、`summarization_retry_*` 等） |

#### 4.5.4 resolve_action（Pi 路径）

- `ApproveOnce` → `{"type":"extension_ui_response","id":<request_id>,"confirmed":true}`（confirm）或 `{"value":<answers[0][0]>}`（select/input/editor）。
- `Reject` → `{"type":"extension_ui_response","id":<request_id>,"cancelled":true}`（message 无处安放，忽略）。
- request_id 不在 `pending_ui` → `Err(RuntimeError::InvalidState)`。
- Pi 侧带 timeout 的请求超时自动以默认值 resolve —— 适配器无需守时，超时后的迟到回写被 Pi 忽略。

#### 4.5.5 取消与停止

- `cancel_native()`：`{"type":"abort","id":"halo-abort-<uuid>"}`（`Abort` pending，响应忽略）。终局仍由 `agent_settled` → `TaskDone{"cancelled"}` 走通用路径，task_flow 取消分支收到 TaskDone 即判 native。
- `stop(grace)`：任务未终局先发 abort → 关 stdin（`writer.take()`，Pi 收 EOF 优雅退出）→ `wait_exit(grace)` → 退出 = Graceful；超时 `kill()` = Forced。状态迁移 Stopping→Stopped 不变。

### 4.6 OpenCode 适配器 v2 数据流

#### 4.6.1 启动与就绪

```
OpenCodeRuntime::start(cmd, tx, opts)
 ├─ pick_free_port()（不变）；password = random_hex_token()（不变，仅改注入名）
 ├─ spawn：serve --hostname 127.0.0.1 --port <p> + cmd.args；
 │        env 追加 OPENCODE_SERVER_PASSWORD=<password>；stdout piped、stderr null
 ├─ 就绪行线程：读 stdout 行，匹配 "listening on http://…:<port>"；
 │        端口≠p → set_failed(端口不一致) + kill；匹配后线程转为丢弃模式（防管道背压）
 ├─ connect()：等就绪行信号（共享 ready_line: Channel）→ 轮询 GET /global/health
 │        （100ms 间隔，请求超时 500ms）直到 healthy==true；401 → Unauthorized 失败关闭
 ├─ 版本：同一响应 version 字段与 OPENCODE_LOCKED_VERSION 全等，否则 VersionMismatch
 └─ Ready 后：spawn sse_loop 线程（GET /event?directory=<percent_encode(cwd)>）
```

- `OcShared` 字段增量：`password: String`（替代 token）、`directory: String`（cwd 真实路径）、`session_id: Mutex<Option<String>>`、`pending_actions: Mutex<HashMap<String, ActionKind /*Permission|Question*/>>`、`last_error: Mutex<Option<String>>`、`sse_shutdown: AtomicBool`；`events_cursor` 删除（无长轮询游标）。
- `request()` 改造：`Authorization: Basic {base64("opencode:"+password)}`；URL 统一追加 `directory` 查询参数；错误 message 仍不含 URL/端口/密码。

#### 4.6.2 run_task

1. `POST /session`（body `{"title": spec.title 或 instructions 首行 cap 80}`，10s）→ 记 `session_id`。
2. `POST /session/{id}/prompt_async`（10s）body：

```jsonc
{
  "model": {"providerID": "<provider>", "modelID": "<model_id>"},   // cmd.model 按 '/' 切分；无 '/' 时省略该字段（用服务端默认）
  "parts": [
    {"type": "text", "text": "<build_task_message(spec)，与 Pi 同一模板>"},
    {"type": "file", "mime": "text/plain", "url": "file://<cwd>/<f>", "filename": "<f>"}   // 每个 spec.files
  ]
}
```

3. 期待 204；非 204/错误 → `Err`（task_flow 现有失败路径接管）。任务过程与终局全部经 SSE。

#### 4.6.3 SSE 事件映射（本会话过滤后）

| OpenCode 事件 | RuntimeEvent |
| --- | --- |
| `server.connected` / `server.heartbeat` | 忽略（heartbeat 复位 30s 读超时计时） |
| `session.status`（busy/retry，本会话） | `Trace{kind:"phase", text:"running"/"retrying", detail:{"phase":…}}` |
| `message.part.updated`（ToolPart） | `Trace{kind:"tool", text:"<tool> <state>", detail:{限长 input/output 摘要}}` |
| `message.part.updated`（text/reasoning 落定） | `Trace{kind:"agent_note", text:<cap 4KiB>}` |
| `message.part.delta` | 忽略（首期无逐字流式） |
| `file.edited` | `Trace{kind:"file_hint", text:<file>, detail:{"path":…}}` |
| `session.diff` / `todo.updated` | 忽略（归因权威在 Git 基线；todo 首期不接） |
| `permission.asked` | `ActionRequest{request_id:<per id>, kind:"permission", prompt:<permission+patterns+metadata 摘要>}`；`pending_actions[id]=Permission` |
| `question.asked` | `ActionRequest{request_id:<que id>, kind:"clarification", prompt:<questions 摘要>}`；`pending_actions[id]=Question` |
| `permission.replied` / `question.replied` / `question.rejected` | `Trace{kind:"lifecycle", text:"操作请求已在原生通道决议"}`（触发编排层 ActionResolved）；清 `pending_actions` |
| `session.error`（本会话） | 缓存 `last_error`；`Trace{kind:"lifecycle", text:"运行错误：<error.name>"}` |
| `session.idle`（deprecated） | 忽略（只认 `session.status`） |
| `session.status`（idle，本会话） | 结束判定（下） |
| `server.instance.disposed` / 未知类型 | disposed → 流终止处理；其余忽略 |

**结束判定**（官方 stream.transport.ts 同款防抖）：收到 idle → `GET /session/status`（2s）复核本会话确为 idle（非 idle = 迟到事件，忽略）→ `GET /session/{id}/message?limit=1`（5s）取末条 assistant：

- `error` 空且 `time.completed` 有值 → `TaskDone{outcome:"finished", summary:<text parts 拼接 cap>}`
- `error.name == "MessageAbortedError"` → `TaskDone{outcome:"cancelled", summary:"任务已被中止"}`
- 其余 error 或 `last_error` 已缓存 → `TaskDone{outcome:"failed", summary:<error 摘要>}`

**断线处理**：读 EOF/IO 错误且非 shutdown → 重连一次（500ms 后）；重连成功 → `GET /session/status` 快照（若本会话已 idle 直接走结束判定）+ `GET /permission`（重发未决 ActionRequest）；重连失败 → `set_failed("OpenCode 事件流中断", …)`（现文案保留）。

#### 4.6.4 resolve_action（OpenCode 路径）

- `pending_actions[request_id] == Permission`：`ApproveOnce` → `POST /permission/{id}/reply` body `{"reply":"once"}`；`Reject` → `{"reply":"reject","message":<message>}`。
- `== Question`：`Answer` → `POST /question/{id}/reply` body `{"answers": answers}`；`Reject` → `POST /question/{id}/reject`。
- 未知 request_id → `Err(RuntimeError::InvalidState)`；HTTP 404（对端已超时/已决）→ 同上（错误 message 中文、无端口/密码）。

#### 4.6.5 取消与停止

- `cancel_native()`：有 `session_id` → `POST /session/{id}/abort`（5s，尽力而为）；终局由 SSE idle → `TaskDone{"cancelled"}`。
- `stop(grace)`：Stopping → ①任务未终局先 abort（5s）→ ②`POST /global/dispose`（2s）→ ③`sse_shutdown=true` + 等 SSE 线程退出（≤1s）→ ④杀子进程（kill+wait；真实 OpenCode 不自行退出，kill 即官方停止语义）→ Stopped。**Graceful = ①②均成功送达；Forced = 任一失败/超时**（测试注入无子进程场景同此判定）。

### 4.7 凭据注入与环境变量（halo-config 增量）

```rust
// halo-config 新增
pub const DEFAULT_CREDENTIAL_ENV: &[(&str, &str)] = &[
    ("anthropic",   "ANTHROPIC_API_KEY"),
    ("openai",      "OPENAI_API_KEY"),
    ("google",      "GEMINI_API_KEY"),
    ("gemini",      "GEMINI_API_KEY"),
    ("deepseek",    "DEEPSEEK_API_KEY"),
    ("groq",        "GROQ_API_KEY"),
    ("mistral",     "MISTRAL_API_KEY"),
    ("xai",         "XAI_API_KEY"),
    ("openrouter",  "OPENROUTER_API_KEY"),
    ("zai",         "ZAI_API_KEY"),
    ("kimi",        "KIMI_API_KEY"),
    ("minimax",     "MINIMAX_API_KEY"),
    ("huggingface", "HF_TOKEN"),
];

/// 显式 credential_env_var 优先；否则按 model 的 "provider/" 前缀查默认表；
/// 有 credential_ref 但两者都解析不出 → ConfigError::CredentialEnvUnresolved（失败关闭，不猜测）。
pub fn resolve_credential_env_var(cfg: &LaunchConfig) -> Result<String, ConfigError>;
```

- 校验（`validate_launch_config` 增量）：`credential_env_var` 匹配 `^[A-Z][A-Z0-9_]{2,63}$`；不得出现在 `ENV_WHITELIST`；不得为保留名（`OPENCODE_SERVER_PASSWORD`、`OPENCODE_SERVER_USERNAME`、`PI_CODING_AGENT_DIR`、`PI_SKIP_VERSION_CHECK`、`PATH` 类）。
- 注入通道不变：dispatch 在 `runtime.start` 取 `resolve_credential_env_var(&config)` 得变量名，`injected.push((name, secret))` 走 `build_child_env`（签名不变）。`CREDENTIAL_ENV_VAR = "HALO_PROVIDER_API_KEY"` 常量删除。
- 两个受管应用共用同一默认表（OpenCode 的 provider env 命名与 Pi 同族；不同名时用户经 `credential_env_var` 显式指定）。
- 非凭据的适配器常量（`OPENCODE_SERVER_PASSWORD`、`PI_CODING_AGENT_DIR`、`PI_SKIP_VERSION_CHECK`）由适配器在 spawn 前 `command.env(...)` 直接追加（OpenCode token 注入的既有先例），不经 `build_child_env`，也不受白名单校验（它们不来自用户输入）。
- `--api-key` 与 `auth.json` 两条注入路径**明令禁止**（凭据红线，写入附录 A 规范正文）。

### 4.8 halo-runtime 改造清单（逐函数）

**lib.rs**

| 项 | 改动 |
| --- | --- |
| `map_trace_event` | 删除（连同其单测）；两适配器各自实现专用映射 |
| `LaunchCmd` | 增 `model` / `thinking_level` / `private_home`（Debug 保持只隐藏 env 值） |
| `RunTaskSpec` | 增 `title: Option<String>` |
| 新增 | `ActionDecision` / `ActionDecisionKind`；`framing` / `encoding` 模块；`PI_MAX_LINE_BYTES` |
| `RuntimeState/RuntimeEvent/StopOutcome/Timeouts/RuntimeError/lock` | 不变 |

**process.rs**

| 函数 | 改动 |
| --- | --- |
| `probe_version` / `parse_semver_token` / `wait_exit` / `ChildProcess` / `RealChild` | 不变 |

**pi.rs**

| 函数/项 | 改动 |
| --- | --- |
| `PiRuntime::probe` | 不变 |
| `PiRuntime::start` | 参数组装改 `--mode rpc --no-session --no-approve --no-extensions [--thinking][--model]`；stderr 改 piped + 诊断环形缓冲线程；env 追加 `PI_SKIP_VERSION_CHECK`/`PI_CODING_AGENT_DIR` |
| `start_with_transport` | 就绪帧改 `{"type":"get_state","id":"halo-ready-…"}`；判定改 `success && !isStreaming && !isCompacting`；`next_id: AtomicU64` → 删除，改 uuid 字符串 id；早退时并入 stderr 摘录 |
| `Pending` | `Reply` 保留；`RunTask` → `PromptAccept`（success:false 才产生 TaskDone{failed}）；`Cancel` → `Abort`；键类型 `u64` → `String` |
| `reader_loop` | 改用 `framing::read_line_lf`（16MB）；超限 → `set_failed`；EOF/坏帧文案不变 |
| `handle_frame` | 识别顺序：`type=="response"` → 按 id 路由；`type=="extension_ui_request"` → ActionRequest/忽略分流；其余 → `map_agent_event` |
| 新增 `map_agent_event` / `PiTaskTracker` | 4.5.3 映射表；tracker 记 stopReason/失败候选/文本聚合缓冲，`agent_settled` 触发 summary 拉取与 TaskDone |
| 新增 `build_task_message(spec)` | 4.5.2 模板（Pi/OpenCode 共用，放 lib.rs 或 pi.rs 导出给 opencode 复用） |
| `PiHandle::run_task` | 发 `{"type":"prompt","id":…,"message":build_task_message(spec)}`，登记 `PromptAccept` |
| 新增 `PiHandle::resolve_action` | 4.5.4 |
| `PiHandle::cancel_native` | 帧改 `{"type":"abort","id":…}` |
| `PiHandle::stop` | 逻辑保持（先 abort 再关 stdin 等宽限）；仅帧话术更新 |
| 单测 | FakePi 内存对端全部改讲 v2 话术；场景一一对应保留（就绪成功/乱序容忍/EOF/坏帧/就绪超时/非空闲/abort 送达/stop 优雅/强杀/未就绪拒绝），新增：prompt success:false → TaskDone failed、agent_settled→get_last_assistant_text→TaskDone、16MB 长行、extension_ui_request→ActionRequest→resolve 回写 |

**opencode.rs**

| 函数/项 | 改动 |
| --- | --- |
| `OPENCODE_LOCKED_VERSION` | `"0.4.2"` → `"1.18.4"`（验收时按装机二进制重锁；`locked_version_constant_is_exact` 单测同步） |
| `OpenCodeRuntime::probe` | 不变 |
| `OpenCodeRuntime::start` | env 注入名改 `OPENCODE_SERVER_PASSWORD`；stdout 改 piped + 就绪行线程（端口校验）；把 `cmd.cwd` 作为 directory 传入 connect |
| `pick_free_port` / `random_hex_token` | 不变 |
| `OcShared` | 字段增删见 4.6.1；`request()` 改 Basic 头 + directory 查询参数 |
| `connect` | 就绪行等待 + `GET /global/health`（healthy+version 同响应）；`/version` 调用删除；Ready 后 spawn `sse_loop` |
| `poll_events` | 删除，替换为 `sse_loop`（4.6.3：分帧、过滤、映射、idle 防抖、断线重连一次+快照重建） |
| 新增 `finish_task` | idle 复核 + 末条消息判定 + TaskDone（4.6.3 结束判定） |
| `OpenCodeHandle::run_task` | POST /session + prompt_async（4.6.2）；不再 spawn 轮询线程（SSE 已常驻） |
| 新增 `OpenCodeHandle::resolve_action` | 4.6.4 |
| `OpenCodeHandle::cancel_native` | `POST /session/{id}/abort` |
| `OpenCodeHandle::stop` | 4.6.5 顺序与 Graceful/Forced 判定 |
| 单测 | tiny_http 假服务升级：Basic 校验、`/global/health`、session/prompt_async/abort/status/message/permission reply/dispose、SSE 流式 body（`Response::new` + channel 供数的 `Read` 实现）；场景保留：就绪/健康超时/版本不匹配/401 快速失败/事件流→TaskDone/取消/停止 Graceful/挂死强杀/Debug 与错误不泄漏端口密码；新增：就绪行端口不一致、idle 防抖（stale_idle）、SSE 断线重连、permission→ActionRequest→reply |

### 4.9 halo-testkit 改造清单

**lib.rs**

- `OPENCODE_VERSION` → `"1.18.4"`（与锁定常量同步）；`DEFAULT_PI_VERSION = "0.81.1"`（对齐真实版本形态，非必须但消除误导）。
- `happy_script()` 语义重构：脚本步骤改为**协议无关的抽象步骤**（`Phase(&str)`、`Note(&str)`、`ToolWrite`（含写真实文件）、`FileHint`），由两个 fake 各自翻译成 v2 话术；**删除 `verification` 步骤**（两真实协议无此语义，见 4.3 与 7.3）。`verify_fail_script` 删除；`action_request_script` 改为插入「权限请求」抽象步骤（fake-pi 译为 `extension_ui_request`，fake-opencode 译为 `permission.asked`）。
- `AGENT_FILE_NAME` / `AGENT_FILE_CONTENT` / `HAPPY_SUMMARY` 不变（证据断言锚点）。

**fake-pi（讲 Pi v2）**

- CLI：`--version` → 裸 semver（不变）；进入 RPC 要求 `--mode rpc`（替代 `--rpc`）；**脚本开关改名**避免与真实参数冲突：`--fake-mode <m>`（原 `--mode`）、`--step-delay-ms`、`--report-env`、`--pid-file` 保留；容忍并忽略 `--no-session/--no-approve/--no-extensions/--thinking/--model` 等真实参数。
- 命令处理：平铺命令；`get_state` → `{"type":"response","command":"get_state","success":true,"data":{"isStreaming":false,"isCompacting":false,…}}`；`prompt` → 先回 `success:true`（happy 类），再吐事件流：`agent_start` → `message_update(text_*)` → `tool_execution_start/end(write, args.path=hello_from_agent.txt)`（其间写真实文件）→ `turn_end(stopReason:"stop")` → `agent_end` → `agent_settled`；`get_last_assistant_text` → `data.text = HAPPY_SUMMARY`；`abort` → `success:true` + `message_update(error aborted)` + `agent_settled`；stdin EOF → exit 0。
- fake-mode 语义映射：`happy`（上）；`not_ready`（get_state 永不回）；`garbage`（吐坏帧）；`crash_mid_task`（事件中途 exit 3）；`hang_on_cancel`（无视 abort 与 EOF，需强杀）；`action_request`（脚本中途发 `{"type":"extension_ui_request","id":"req-1","method":"confirm","title":"等待权限确认","message":"允许写入 hello_from_agent.txt？"}`，等 `extension_ui_response` 或 3s 超时后继续完成 happy 脚本——匹配 Pi 真实 timeout 自动默认值行为）。`verify_fail` 模式删除。
- `FAKE_PI_MODE`/`FAKE_PI_VERSION` 环境变量通道保留（testkit 自测用）。

**fake-opencode（讲 OpenCode v2）**

- CLI：`--version` 裸 semver；`serve --hostname 127.0.0.1 --port <n>` 解析不变（仍拒绝非回环）；脚本开关：`--fake-mode <m>`（原 `--mode`）、`--auth-digest-file <path>`（原 `--token-digest-file`，改为对 `OPENCODE_SERVER_PASSWORD` 计 SHA-256 摘要，仍只写摘要不写明文）。
- 启动即向 stdout 打印就绪行 `opencode server listening on http://127.0.0.1:<port>`（就绪行契约）。
- 认证：校验 `Authorization: Basic base64("opencode:<OPENCODE_SERVER_PASSWORD>")`；未设密码 = 全拒（失败关闭不变）；`bad_auth` 模式（原 `bad_token`）对正确凭据也回 401。
- 端点：`GET /global/health` → `{"healthy":true,"version":<OPENCODE_VERSION 或 wrong_version 值>}`（`unhealthy` 模式回 500）；`POST /session` → `{"id":"ses_fake1",…}`；`POST /session/{id}/prompt_async` → 204 并启动脚本线程；`GET /event` → SSE 长连接（`data:` 帧：`server.connected` → 脚本事件 → `session.status(idle)`）；`GET /session/status` → 快照；`GET /session/{id}/message` → 末条 assistant（happy：`error` 空 + `time.completed` + text part = HAPPY_SUMMARY；abort 后：`error:{name:"MessageAbortedError"}`）；`POST /session/{id}/abort` → `true` 并让脚本走 aborted 收尾；`POST /permission/{id}/reply` → `true` + SSE 发 `permission.replied`；`POST /question/{id}/reply`、`/reject` 同构；`POST /global/dispose` → `true` + SSE 发 `server.instance.disposed` 并结束流（进程**不退出**，与真实一致，等 Sidecar 杀）。
- 脚本事件（happy）：`session.status(busy)` → `message.part.updated(text)` → `message.part.updated(tool running/completed, 写真实文件)` → `file.edited` → `session.status(idle)`。
- fake-mode：`happy` / `unhealthy` / `wrong_version` / `bad_auth` / `exit_early`（2s 后自杀）/ `dispose_error`（dispose 回 500，测 Forced）/ `permission_request`（脚本中途发 `permission.asked`，收到 reply 或 5s 超时后继续）/ `sse_drop`（发一半事件后掐断 SSE 连接一次，重连后续完，测快照重建）/ `stale_idle`（先发一条 idle 但 `/session/status` 仍 busy，随后才真正走完脚本，测防抖复核）。`hang_on_shutdown` 语义由 `dispose_error` + 默认「dispose 后不退出」共同覆盖。

### 4.10 halo-sidecar 最小改动（RuntimeEvent 形状不变的前提下）

| 文件 | 改动 |
| --- | --- |
| `state.rs` | `AgentHandle` trait 增 `fn resolve_action(&self, request_id: &str, decision: &halo_runtime::ActionDecision) -> Result<(), RuntimeError>;`（两个生产 impl 转发）；`ActiveTask` 增 `pending_action: Option<PendingAction>`（`struct PendingAction { request_id: String, kind: String }`） |
| `task_flow.rs` | `on_action_request` 记 `pending_action`；`resolve_pending_action` 清 `pending_action`（各一行）；`start_task` 组 `RunTaskSpec` 时带 `title: Some(args.title)`。主循环/终局/取证逻辑零改动 |
| `dispatch.rs` | 删 `CREDENTIAL_ENV_VAR`，`runtime_start` 改用 `halo_config::resolve_credential_env_var`；`LaunchCmd` 填充 `model`/`thinking_level`/`private_home(<HALO_DATA_DIR>\pi-agent，仅 Pi)`；新增路由 `"task.resolve_action" => self.task_resolve_action(params)`（校验 awaiting_action + pending_action 匹配 → `handle.resolve_action` → `{"accepted":true}`；不匹配 → `ACTION_REQUEST_NOT_FOUND`）；`config.save` 校验并保存 `credential_env_var` |
| `mapping.rs` | `runtime_state_payload` 的 `reason`/`recovery_hint` 出口统一过 `halo_core::sanitize`（防御纵深：Pi stderr 摘录入 reason 的新路径） |
| `halo-protocol` | `LaunchConfigInput`/`LaunchConfig` DTO 增 `credential_env_var: Option<String>`；新增 `methods::task::ResolveActionParams`；`ErrorCode` 增 `ActionRequestNotFound` |
| `halo-store` | `launch_configs` 表增列 `credential_env_var TEXT NULL`（迁移 v+1，幂等） |

---

## 5. 差异化点

本模块不裁决差异化功能（裁决权在 15 号），只留两个接口挂点：

1. **操作请求决议挂点**：`task.action_request` 事件（已有）+ `task.resolve_action` 方法（本文档新增）构成完整闭环；UI 侧按钮/表单（approve/reject/answer）由 10 号壳层归位、15 号裁决呈现形态。Python 侧仅需 TaskViewModel 增一个透传方法（非本轮必做）。
2. **file_hint 提示挂点**：两适配器的 `Trace{kind:"file_hint"}` 事件为 15 号「基线感知徽章」提供实时线索源；归因权威始终是 Git 基线算法，本文档不改变该边界。

---

## 6. 实施计划

### 6.1 文件清单

**修改（Rust）**

| 文件 | 内容 |
| --- | --- |
| `sidecar/crates/halo-runtime/src/lib.rs` | 4.8 lib.rs 项 |
| `sidecar/crates/halo-runtime/src/pi.rs` | Pi v2 重写（4.5 / 4.8） |
| `sidecar/crates/halo-runtime/src/opencode.rs` | OpenCode v2 重写（4.6 / 4.8） |
| `sidecar/crates/halo-runtime/src/framing.rs` **新建** | LF 分帧读取器（16MB） |
| `sidecar/crates/halo-runtime/src/encoding.rs` **新建** | base64 / percent 编码（无新外部依赖） |
| `sidecar/crates/halo-config/src/lib.rs` | `credential_env_var` 字段、默认表、`resolve_credential_env_var`、校验 |
| `sidecar/crates/halo-protocol/src/…` | DTO 字段追加、ResolveActionParams、ErrorCode 追加 |
| `sidecar/crates/halo-store/src/…` | launch_configs 迁移追加列 |
| `sidecar/crates/halo-sidecar/src/{state,task_flow,dispatch,mapping}.rs` | 4.10 |
| `sidecar/crates/halo-testkit/src/lib.rs` | 抽象脚本步骤重构 |
| `sidecar/crates/halo-testkit/src/bin/fake_pi.rs` | Pi v2 话术重写 |
| `sidecar/crates/halo-testkit/src/bin/fake_opencode.rs` | OpenCode v2 话术重写 |
| `sidecar/crates/halo-testkit/tests/{fake_pi,fake_opencode}.rs` | v2 断言重写 |
| `sidecar/crates/halo-integration-tests/tests/*.rs` | 7.3 兼容策略逐文件修订 |

**修改（文档）**

| 文件 | 内容 |
| --- | --- |
| `docs/module-contracts.md` | 第 5 节替换为附录 A；第 7 节替换为附录 B；第 3/6 节按 M-1/M-3 增补 |
| `docs/ipc-protocol.md` | I-1 ~ I-4 追加 |
| `docs/architecture.md` | M-5 文字微调 |
| `docs/traceability.md` | v2 对齐条目 |
| `docs/design/README.md` | 14 号状态更新 |

**可选（Python，非阻塞）**：`app/tests/fake_sidecar.py` 增 `task.resolve_action` 脚本化应答；`app/halo_studio/viewmodels/` 透传方法（随 10/15 号落地）。

### 6.2 依赖顺序

1. **halo-config + halo-protocol + halo-store**（凭据 env 解析、DTO、迁移）——无上游依赖，可先行，各自 `cargo test -p` 独立绿。
2. **halo-runtime v2**（pi.rs / opencode.rs / framing / encoding + 单测）——不依赖 1（`LaunchCmd` 新字段为 Option，默认 None 即可编译）。
3. **halo-testkit v2**（与 2 并行开发，同一份附录 A 规范为准绳）。
4. **halo-sidecar 装配**（dispatch/state/task_flow/mapping）——依赖 1+2。
5. **halo-integration-tests 修订**——依赖 2+3+4。
6. **文档合入**（module-contracts / ipc-protocol / architecture / traceability）与 6.1 文档清单同步完成。

crate 零依赖纪律不变：runtime/testkit 互不依赖，均只对齐附录 A 文本。

---

## 7. 测试计划

### 7.1 单元测试（各 crate 内）

- **halo-runtime / pi**：v2 话术全景（4.8 pi.rs 单测行）；`framing` 长行/CRLF/U+2028/超限；`build_task_message` 模板快照；`PiTaskTracker` outcome 判定表（stop/toolUse/aborted/error/retry-fail）。
- **halo-runtime / opencode**：v2 话术全景（4.8 opencode.rs 单测行）；`encoding` base64/percent 向量测试；`Basic` 头构造不泄漏（Debug/错误 message 断言扩展到 password）。
- **halo-config**：`resolve_credential_env_var` 显式优先/前缀推导/未解析失败关闭；`credential_env_var` 校验（白名单冲突/保留名/格式）。
- **halo-sidecar**：task_flow 现有单测**全部原样保留**（直接注入 RuntimeEvent，不受协议话术影响，覆盖 Verification/ActionRequest/取消/脱敏路径）；新增 `task.resolve_action` dispatch 测试（awaiting_action 门禁、request_id 不匹配、decision/kind 不匹配）。
- **halo-testkit 自测**：两个 fake 的 v2 行为逐模式断言（对照附录 B）。

### 7.2 集成测试（真实子进程）

现有场景逐一保留（话术/断言按 v2 修订），新增三个场景：

| 测试文件 | v2 修订 |
| --- | --- |
| `happy_pi.rs` | fake 参数 `--fake-mode happy`；trace.item 断言的 kind 集改为 `phase/agent_note/tool/file_hint`；`task.verification` 断言改为证据 `verification.status=="not_run" && source=="agent"`（见 7.3） |
| `happy_opencode.rs` | `--auth-digest-file`（摘要对象改 password）；「两次启动认证不同」断言保留；hex 泄漏探测（`contains_lower_hex_run`）保留 |
| `runtime_failures.rs` | `--fake-mode not_ready/garbage/wrong_version/unhealthy/bad_auth`；错误码断言不变（NotReady/VersionMismatch/Unauthorized 语义未变） |
| `cancel.rs` | `--fake-mode action_request`（走 Pi extension_ui_request 路径）与 `hang_on_cancel`；native/forced 断言不变 |
| `credential_canary.rs` | config `model="anthropic/claude-sonnet"`，`--report-env ANTHROPIC_API_KEY`；「只报存在性不报值」纪律不变 |
| `manual_edit.rs` / `handoff_boundary.rs` / `interruption.rs` / `workspace_*.rs` / `evidence_versions.rs` / `event_recovery.rs` / `task_running_guard.rs` / `delivery_git_invariance.rs` | 仅 fake 参数名（`--fake-mode`）与 phase 文案跟随调整，场景不动 |
| **新增** `action_request_opencode.rs` | fake-opencode `permission_request` 模式：`task.action_request` 事件 → `task.resolve_action(approve_once)` → 任务完成；再跑 reject 分支 → 任务失败/完成按脚本断言 |
| **新增** `opencode_sse_recovery.rs` | `sse_drop` 模式：断流重连 + 快照重建后任务仍正确终局；`stale_idle` 模式：迟到 idle 不误终结 |
| **新增** `opencode_stop_semantics.rs` | 默认模式 stop → dispose 送达 + Graceful；`dispose_error` → Forced |

### 7.3 测试语义变化说明（覆盖不缩水论证）

1. **verification（源=agent）**：v2 下两个真实协议均无验证结论语义，协议路径不再产生 `RuntimeEvent::Verification` → 集成断言由 `passed/agent` 改为 `not_run/agent`（缺省「Agent 未报告验证结果」），这是**如实化**而非弱化；`on_verification` 代码路径与证据三态字段由 task_flow 单测（直接注入事件）继续全量覆盖；用户标记路径（`task.mark_verification`）测试不变。`verify_fail` fake 模式随语义删除，其「任务完成但结论异常」的场景位由新增 `session.error` 相关断言（`action_request_opencode.rs` reject 分支 + `sse_drop` 失败路径）补足。
2. **action_request**：由 Pi 单侧（v1 自造事件）扩展为双侧真实话术（Pi extension_ui_request + OpenCode permission.asked），并新增决议回路测试——覆盖净增。
3. **优雅停止**：OpenCode 的 Graceful 判据从「/shutdown 送达」改为「abort+dispose 送达」（裁决 OC-9），Forced 判据从「进程挂死」扩展为「停止请求失败或进程挂死」；Pi 停止语义不变。
4. 其余 248 例的场景语义（信任门禁、基线归因、证据追加、交接白名单、中断标记、事件恢复、凭据 canary、路径含空格中文）全部不变。

### 7.4 验收命令

`cd sidecar; cargo build --workspace; cargo test --workspace`（预期 248+新增 ≥ 260 例全绿）；`cd app; ..\.venv\Scripts\python.exe -m pytest tests -q`（57 例不变）；`scripts\test-all.ps1`。

---

## 8. 风险与缓解

| # | 风险 | 缓解 |
| --- | --- | --- |
| 1 | 参考源码为 dev 主干（OpenCode 1.18.4 / Pi 0.81.1），装机二进制行为或版本漂移 | `OPENCODE_LOCKED_VERSION` 验收时按实际装机重锁并全等把关；事件/字段解析对未知一律忽略；Pi 只锁探测可达性不锁版本（其 stdio 协议带 `docs/rpc.md` 稳定性承诺，坏帧仍失败关闭） |
| 2 | SSE 断线丢事件导致任务卡死 | 官方同款三重保障：idle 防抖复核、断线重连+快照重建、重建失败 → `Failed`（task_flow 现有失败路径接管）；`stale_idle`/`sse_drop` 集成测试钉住 |
| 3 | Pi 大帧（message_update 全量 partial）撑爆读取器 | 16MB 独立上限 + 禁用全量命令 + 超限失败关闭（不静默截断协议帧） |
| 4 | 凭据变量名映射错误导致密钥注入到错误变量 | 失败关闭：解析不出即 `CredentialEnvUnresolved`，不猜测；canary 集成测试用真实变量名断言存在性；保留名列表防覆盖适配器自注入变量 |
| 5 | `--fake-mode` 更名遗漏导致集成测试静默走 happy 默认 | fake 进程对无法识别的 `--fake-mode` 值直接 exit 2（快速失败），不再默认 happy；集成测试逐文件 review |
| 6 | OpenCode 权限请求在无人值守时永久挂起任务 | 用户可 `task.cancel`（abort → MessageAbortedError → cancelled）兜底；`task.action_request` 事件带 prompt 提示用户决议；不注入 allow-all 规则（保持原生权限模型） |
| 7 | Pi stderr 摘录进 Failed reason 引入泄漏面 | 摘录截 200 字节 + 仅可打印字符 + sidecar 出口 `sanitize`（mapping.rs 防御纵深）+ 现有「错误不含凭据」断言扩展 |
| 8 | 就绪行解析对 OpenCode 输出格式变化敏感 | 就绪行仅做端口一致性快速失败；权威就绪仍是 `/global/health`（行未匹配到时健康轮询照常，超时语义不变） |

---

## 附录 A — 修订后的 `docs/module-contracts.md` 第 5 节全文（替换用）

> ## 5. sidecar/crates/halo-runtime —— 受管运行时
>
> **职责**：进程监督、Pi stdio RPC 适配器（协议 v2）、OpenCode 回环服务适配器（协议 v2）、取消/停止语义。线程 + `crossbeam-channel`，无 async。
>
> **适配器协议 v2（依据 R3/R4 真实协议分析与 14 号设计裁决；halo-testkit 假进程按此实现）**：
>
> - **Pi**（对齐 pi 0.81.1 `--mode rpc`）：启动 `<exe> --mode rpc --no-session --no-approve --no-extensions [--thinking <off|low|medium|high>] [--model <provider>/<model_id>]` 后 stdio JSONL（仅 LF 分帧、剥尾部 `\r`、入站行上限 16MB）。探测 `<exe> --version` → 首行裸 semver。就绪：发 `{"type":"get_state","id":"<str>"}`，在超时（默认 10s，可注入）内收到 `{"type":"response","command":"get_state","success":true,"data":{…}}` 且 `data.isStreaming==false && data.isCompacting==false`。任务：`{"type":"prompt","id":"<str>","message":<模板拼装 instructions/files/base_diff/notes>}`；响应 `success:true` 仅表示受理（可能晚于首批事件，乱序容忍），`success:false` → 任务失败。事件为顶层 `type` 平铺的 AgentSessionEvent：`tool_execution_*` → Trace(tool/file_hint)、`message_update` 文本块聚合 → Trace(agent_note)、`extension_ui_request`（对话类）→ ActionRequest（回写 `extension_ui_response`；生产 `--no-extensions` 下静默）、`agent_settled` → 终态（随后 `get_last_assistant_text` 取 summary；outcome 按最近 stopReason：stop/toolUse=finished、aborted=cancelled、error 或 auto_retry 失败=failed）；未知 type 一律忽略。取消：`{"type":"abort"}` → aborted 收尾 + agent_settled。停止：（任务中先 abort）+ **关 stdin**（Pi 优雅退出 exit 0），宽限超时 kill。EOF/坏帧/超限行 → Failed{reason}。凭据经 provider 环境变量注入（`resolve_credential_env_var` 解析，如 `ANTHROPIC_API_KEY`）；适配器自注入 `PI_CODING_AGENT_DIR=<数据目录>\pi-agent`、`PI_SKIP_VERSION_CHECK=1`；**禁止** `--api-key` 与写 `auth.json`。
> - **OpenCode**（对齐 opencode 1.18.4 serve）：锁定版本常量 `pub const OPENCODE_LOCKED_VERSION: &str = "1.18.4";`（按装机二进制重锁）。启动 `<exe> serve --hostname 127.0.0.1 --port <p>`，`p` 由 Sidecar 选空闲端口；每次启动生成 32 字节随机 hex 密码，经 `OPENCODE_SERVER_PASSWORD` 注入；所有 HTTP 请求带 `Authorization: Basic base64("opencode:<密码>")` 并显式携带 `?directory=<工作区真实路径>`。就绪双重判定：stdout 就绪行 `listening on http://…:<p>`（端口一致性校验）+ `GET /global/health` → `{"healthy":true,"version":…}`；`version` 与锁定值**完全相等**，否则 Failed{RUNTIME_VERSION_MISMATCH}。任务：`POST /session`（title）→ `POST /session/{id}/prompt_async`（parts=text+file，204 受理）。事件：SSE `GET /event?directory=…` 专用线程（`data:` 行 JSON `{type,properties}`；忽略 heartbeat 与未知 type；按 sessionID 过滤）；`permission.asked`/`question.asked` → ActionRequest（决议 `POST /permission/{id}/reply`{once|reject+message} / `POST /question/{id}/reply|reject`；不暴露 always）；结束判定 = `session.status(idle)` → 复核 `GET /session/status` → `GET /session/{id}/message?limit=1` 末条 assistant（error 空+completed=finished；MessageAbortedError=cancelled；其余/session.error=failed）；断线重连一次+快照端点（/session/status、/permission）重建，失败 → Failed。取消：`POST /session/{id}/abort`。停止：（任务中先 abort）→ `POST /global/dispose` → 关 SSE → kill 子进程（官方停止语义）；Graceful=abort/dispose 均送达，Forced=任一失败或超时。401 → 失败关闭；无密码启动路径不存在。
>
> ```rust
> pub enum RuntimeState { NotProbed, Probing, Starting, Ready, Failed { reason: String, recovery_hint: String }, Stopping, Stopped }
> pub struct RuntimeTraceItem { pub kind: String, pub text: String, pub detail: serde_json::Value }   // sidecar 映射为契约 TraceItem
> pub enum RuntimeEvent { State(RuntimeState), Trace(RuntimeTraceItem), ActionRequest { request_id: String, kind: String, prompt: String }, Verification { status: String, detail: String }, TaskDone { outcome: String, summary: String } }
> // 注：协议 v2 下 Verification 变体不再由适配器产生（两真实协议无验证结论语义），保留供用户标记路径与前向兼容。
> pub struct LaunchCmd { pub exe: String, pub args: Vec<String>, pub env: HashMap<String,String>, pub cwd: String, pub model: Option<String>, pub thinking_level: Option<String>, pub private_home: Option<String> } // env 已由 halo-config 构好；Debug 隐藏 env 值
> pub struct RunTaskSpec { pub title: Option<String>, pub instructions: String, pub files: Vec<String>, pub base_diff: Option<String>, pub notes: Option<String> }
> pub enum ActionDecisionKind { ApproveOnce, Reject, Answer }
> pub struct ActionDecision { pub kind: ActionDecisionKind, pub message: Option<String>, pub answers: Option<Vec<Vec<String>>> }
> pub struct PiRuntime;   impl PiRuntime   { pub fn probe(exe:&str)->Result<String,RuntimeError>; pub fn start(cmd:LaunchCmd, tx:Sender<RuntimeEvent>, opts:Timeouts)->Result<PiHandle,RuntimeError>; }
> pub struct PiHandle;    impl PiHandle    { pub fn run_task(&self, spec:&RunTaskSpec)->Result<(),RuntimeError>; pub fn resolve_action(&self, request_id:&str, d:&ActionDecision)->Result<(),RuntimeError>; pub fn cancel_native(&self); pub fn stop(&self, grace:Duration)->StopOutcome; pub fn state(&self)->RuntimeState; }
> pub struct OpenCodeRuntime / OpenCodeHandle;  // 同构 API；内部持有端口+密码+sessionID，**绝不**出现在任何公开 getter/Debug/错误 message 中
> pub enum StopOutcome { Graceful, Forced }
> pub struct Timeouts { pub ready: Duration, pub cancel_grace: Duration, pub shutdown_grace: Duration } // Default 10s/10s/5s
> pub(crate) const PI_MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
> ```
>
> **测试**：不 spawn 真进程的单元测试用读写对注入内存管道（v2 话术：分帧、乱序 id、prompt 受理/拒绝、agent_settled 终局、EOF、坏帧、超限行、extension_ui 往返）；OpenCode 用 tiny_http 假服务（Basic 校验、/global/health、SSE 流式 body、idle 防抖、断线重连、permission 往返、版本不匹配、401）。真实子进程集成测试用 halo-testkit 的 bin。

## 附录 B — 修订后的 `docs/module-contracts.md` 第 7 节全文（替换用）

> ## 7. sidecar/crates/halo-testkit —— 受控假进程（仅测试）
>
> bins：`fake-pi`、`fake-opencode`，严格实现第 5 节适配器协议 **v2**。脚本开关经命令行参数注入（Sidecar 白名单环境不传宿主 FAKE_* 变量；LaunchConfig.extra_args 附加在适配器固定参数后）：`--fake-mode <m>`（无法识别的值 exit 2，不默认 happy）、`--step-delay-ms <n>`、`--report-env <VAR>`（只报存在性不报值）、`--pid-file <path>`；环境变量 `FAKE_PI_MODE`/`FAKE_PI_VERSION`/`FAKE_OC_MODE`/`FAKE_OC_VERSION` 仅供 testkit 自测。
> `fake-pi`（讲 Pi v2：`--mode rpc` 平铺命令、get_state 空闲字段、prompt→AgentSessionEvent 流→agent_settled、get_last_assistant_text、abort、stdin EOF 退出；容忍并忽略真实启动参数）`--fake-mode` = `happy` | `not_ready`(get_state 永不回) | `garbage`(坏帧) | `crash_mid_task` | `hang_on_cancel`(无视 abort 与 EOF，验证强杀) | `action_request`(中途发 extension_ui_request confirm，等应答或 3s 超时默认继续)。
> `fake-opencode`（讲 OpenCode v2：stdout 就绪行、Basic 认证校验 `OPENCODE_SERVER_PASSWORD`、/global/health、session+prompt_async、SSE /event、/session/status、message 快照、abort、permission/question reply、/global/dispose 后**不退出进程**；只绑 127.0.0.1）`--fake-mode` = `happy` | `unhealthy` | `wrong_version` | `bad_auth`(正确凭据也 401) | `exit_early` | `dispose_error`(dispose 回 500，测 Forced) | `permission_request` | `sse_drop`(断流一次测重连) | `stale_idle`(迟到 idle 测防抖)；`--auth-digest-file <path>` 追加写入密码的 SHA-256 摘要（明文绝不落盘）。
> happy 模式产出固定脚本：phase(running)、工具事件(write)、写一个真实文件（cwd 下 `hello_from_agent.txt`）、file_hint、正常终局 finished + 摘要 —— 集成测试据此断言真实文件变更与证据。**协议 v2 无验证结论语义：happy 脚本不再产出 verification 事件，证据验证状态如实为 not_run。**

---

## 修订记录

- 2026-07-27 初版：基于 R3（opencode-dev 1.18.4）与 R4（pi-main 0.81.1）差异表的适配器 v2 全量裁决与改造设计。
