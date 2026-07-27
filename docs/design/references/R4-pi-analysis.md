# R4 — Pi 真实 RPC 协议分析与适配器差距

- 分析对象：`D:\用于参考的开源项目的代码\pi-main`（`@earendil-works/pi-coding-agent`，源码版本 **0.81.1**，MIT 许可证；只提炼协议事实，不复制源码）。
- 对照基准：`docs/module-contracts.md` 第 5 节中 halo-runtime 的**假设 Pi 适配器协议**（`--rpc`、`get_state→idle`、`run_task`、`cancel`、`{method:"event"}` 通知流）。
- 结论先行：**假设协议与真实协议在封包形状、命令语法、任务模型、完成语义、权限请求五个维度全部不一致**；但进程形态（stdio JSONL 子进程）、版本探测（裸 semver）、以帧为单位的事件流这三个基础假设成立，适配器 v2 只需换"话术"，不需换架构。

## 1. 进程形态

### 1.1 CLI 入口与运行模式

| 事实 | 出处（pi-main 相对路径） |
| --- | --- |
| npm bin 名为 `pi`，入口 `dist/cli.js`；Node >= 22.19.0；另有 Bun 编译的单文件二进制 `dist/pi` | `packages/coding-agent/package.json`（`bin`、`engines`、`build:binary`） |
| 四种运行模式：交互 TUI（默认）、print（`--print`/-p 或 stdin/stdout 非 TTY 时自动降级）、`--mode json`（一次性 prompt + JSON 事件流后退出）、`--mode rpc`（常驻 headless 服务） | `packages/coding-agent/src/main.ts` `resolveAppMode()`；`src/cli/args.ts` |
| RPC 模式启动：`pi --mode rpc [options]`，**纯 stdio JSONL，无端口、无 HTTP 服务** | `packages/coding-agent/docs/rpc.md`；`src/modes/rpc/rpc-mode.ts` |
| 另有专用入口 `dist/rpc-entry.js`：等价于自动前置 `--mode rpc` 参数（`main(["--mode","rpc",...argv])`） | `packages/coding-agent/src/rpc-entry.ts` |
| RPC 模式下 stdout 被"接管"：所有杂散 `console.log` 重定向到 stderr，stdout 只输出协议帧（`writeRawStdout`）；计时/诊断信息全部走 stderr | `packages/coding-agent/src/core/output-guard.ts` `takeOverStdout()`；`src/core/timings.ts` |
| 启动**无握手输出**：RPC 模式就绪后不主动打印任何 banner/ready 帧，就绪探测只能靠客户端发命令等响应 | `src/modes/rpc/rpc-mode.ts`（`runRpcMode` 全函数无启动输出） |
| 启动失败即退出：无可用模型/凭据时 stderr 打印指引并 `exit 1`（`appMode !== "interactive" && !session.model`） | `src/main.ts` 约 800 行处 |
| RPC 模式禁止 `@file` 参数；stdin 完全留给 JSONL 命令流 | `src/main.ts`（`parsed.mode === "rpc" && parsed.fileArgs.length > 0` → exit 1） |

### 1.2 帧格式（Framing）

- 严格 JSONL：**仅以 LF（`\n`）为记录分隔符**；输入允许 `\r\n`（剥掉尾部 `\r`）。
- 官方文档明确警告：不得使用会把 `U+2028`/`U+2029` 当换行的通用行读取器（如 Node `readline`），因为它们是 JSON 字符串内的合法字符。
- Pi 侧**没有行长上限**；`get_messages`/`get_entries`/`get_tree` 的响应帧可以远超 1MB。
- 出处：`docs/rpc.md`（Framing 节）、`src/modes/rpc/jsonl.ts`。

## 2. 消息协议

### 2.1 封包三类：命令（stdin）/ 响应（stdout）/ 事件（stdout）

**不是 JSON-RPC**。没有 `method`/`params`/`result` 包装，命令类型放在顶层 `type` 字段，参数平铺：

```json
{"id": "req-1", "type": "prompt", "message": "Hello", "streamingBehavior": "steer"}
```

- `id` 是**可选的 string**（不是 number），仅用于请求/响应关联；事件一律无 `id`。
- 响应固定形状：`{"id?": string, "type": "response", "command": "<命令type>", "success": bool, "data?": {...}, "error?": "message"}`。
- 解析失败的行回 `{"type":"response","command":"parse","success":false,"error":"..."}`（无 id）。
- 未知命令回 `success:false, error:"Unknown command: <type>"`。
- 出处：`src/modes/rpc/rpc-types.ts`（`RpcCommand`/`RpcResponse` union）、`src/modes/rpc/rpc-mode.ts`（`handleCommand`/`handleInputLine`）。

### 2.2 命令全集（RpcCommand union，0.81.1）

| 分组 | 命令 `type` | 关键字段 |
| --- | --- | --- |
| 提示 | `prompt` | `message`, `images?: ImageContent[]`, `streamingBehavior?: "steer"\|"followUp"` |
| 提示 | `steer` / `follow_up` | `message`, `images?`（流式中排队的两种投递语义） |
| 提示 | `abort` | 中止当前 agent 运行（进程不退出） |
| 会话 | `new_session` | `parentSession?`；`switch_session`(`sessionPath`)、`fork`(`entryId`)、`clone`、`get_fork_messages`、`get_entries`(`since?` 游标)、`get_tree`、`set_session_name`(`name`)、`get_session_stats`、`export_html`(`outputPath?`) |
| 状态 | `get_state` / `get_messages` / `get_last_assistant_text` / `get_commands` | 见 §3 |
| 模型 | `set_model`(`provider`,`modelId`) / `cycle_model` / `get_available_models` | |
| 思考 | `set_thinking_level`(`level`) / `cycle_thinking_level` / `get_available_thinking_levels` | level ∈ `off/minimal/low/medium/high/xhigh/max` |
| 队列 | `set_steering_mode` / `set_follow_up_mode` | `mode: "all"\|"one-at-a-time"` |
| 压缩 | `compact`(`customInstructions?`) / `set_auto_compaction`(`enabled`) | |
| 重试 | `set_auto_retry`(`enabled`) / `abort_retry` | |
| Bash | `bash`(`command`,`excludeFromContext?`) / `abort_bash` | 宿主直接注入 bash 结果进上下文 |

### 2.3 prompt 提交与响应时序（重要陷阱）

- `prompt` 的响应在 **preflight 通过后异步发出**：`success:true` 表示"已接受/已排队/已处理"，**不代表任务完成**；接受之后的失败只走事件流，不会再发第二个响应。
- 响应帧与首批事件帧**没有先后保证**（代码里 `void session.prompt(...)` 异步执行，`preflightResult` 回调时才输出响应）——适配器不得假设"先收响应再收事件"。
- agent 正在流式输出时，不带 `streamingBehavior` 的 `prompt` 直接返回 `success:false`。
- 出处：`src/modes/rpc/rpc-mode.ts`（case `"prompt"`）、`docs/rpc.md`。

### 2.4 事件流（type 平铺，非 `{method:"event"}`）

每行一个顶层事件对象，与 `--mode json` 打印模式同构（`AgentSessionEvent`）：

| 事件 `type` | 载荷要点 |
| --- | --- |
| `agent_start` | agent 开始处理 |
| `turn_start` / `turn_end` | 一轮 = 一条 assistant 消息 + 其工具调用；`turn_end` 带 `message`、`toolResults` |
| `message_start` / `message_update` / `message_end` | `message_update` 含 `assistantMessageEvent` 增量：`text_start/text_delta/text_end`、`thinking_start/thinking_delta/thinking_end`、`toolcall_start/toolcall_delta/toolcall_end`、`done`(reason: stop/length/toolUse)、`error`(reason: aborted/error) |
| `tool_execution_start` / `tool_execution_update` / `tool_execution_end` | `toolCallId`、`toolName`、`args`；update 的 `partialResult` 是**累积值**而非增量；end 带 `result`、`isError` |
| `agent_end` | 一次底层运行结束，`messages` + `willRetry`（可能还有重试/压缩/续跑） |
| `agent_settled` | **彻底安定**：无自动重试、无压缩重试、无排队消息——这才是任务终态信号 |
| `queue_update` | 当前 steering/followUp 排队全量 |
| `compaction_start/end`、`auto_retry_start/end`、`summarization_retry_*` | 压缩与重试生命周期 |
| `extension_error` | 扩展抛错（`extensionPath`,`event`,`error`） |

出处：`docs/rpc.md`（Events 节）、`docs/json.md`、`packages/agent/src/types.ts`（`AgentEvent`）。

### 2.5 权限/确认请求：核心没有，只有"扩展 UI 子协议"

- **Pi 核心无权限系统**：内建工具（read/bash/edit/write）直接以宿主用户权限执行，官方立场是"要边界就上容器/沙箱"（README「Permissions & Containerization」）。
- 唯一的交互请求通道是**扩展 UI 子协议**：扩展调用 `ctx.ui.confirm()` 等时，RPC 模式在 stdout 发：

```json
{"type":"extension_ui_request","id":"uuid","method":"confirm","title":"Allow dangerous command?","message":"...","timeout":10000}
```

- 对话类 `method`：`select`（`options[]`）、`confirm`、`input`、`editor`（`prefill`）——阻塞等待 stdin 上匹配 `id` 的应答：
  - `{"type":"extension_ui_response","id":"uuid","value":"Allow"}`（select/input/editor）
  - `{"type":"extension_ui_response","id":"uuid","confirmed":true}`（confirm）
  - `{"type":"extension_ui_response","id":"uuid","cancelled":true}`（任意对话取消）
- 带 `timeout`（毫秒）的请求超时后 Pi 侧自动以默认值（undefined/false）resolve，客户端无需守时。
- 通知类（无需应答）：`notify`(`notifyType: info/warning/error`)、`setStatus`、`setWidget`、`setTitle`、`set_editor_text`。
- 出处：`src/modes/rpc/rpc-mode.ts`（`createExtensionUIContext`/`createDialogPromise`）、`docs/rpc.md`（Extension UI Protocol 节）。

## 3. 状态查询与版本

### 3.1 get_state（有该命令，但形状与假设完全不同）

```json
{"type":"get_state","id":"s1"}
```
→
```json
{"id":"s1","type":"response","command":"get_state","success":true,
 "data":{"model":{...},"thinkingLevel":"medium","isStreaming":false,"isCompacting":false,
         "steeringMode":"one-at-a-time","followUpMode":"one-at-a-time",
         "sessionFile":"/path/session.jsonl","sessionId":"abc123","sessionName":"...",
         "autoCompactionEnabled":true,"messageCount":5,"pendingMessageCount":0}}
```

- **没有 `state:"idle"` 字段**。空闲判定 = 响应成功且 `isStreaming===false && isCompacting===false`（严格空闲还应 `pendingMessageCount===0`）。
- 就绪探测 = 启动后发 `get_state` 等响应（启动无任何主动输出）。
- 出处：`src/modes/rpc/rpc-types.ts`（`RpcSessionState`）、`src/modes/rpc/rpc-mode.ts`（case `"get_state"`）。

### 3.2 版本输出

- `pi --version` 或 `pi -v` → stdout 打印**裸 semver 一行**（`console.log(VERSION)`，如 `0.81.1`），随后 `exit 0`。`VERSION` 来自 package.json 的 `version`，缺省 `"0.0.0"`。
- 与我们"首行 semver"假设**兼容**。
- 出处：`src/main.ts`（`if (parsed.version)`）、`src/config.ts`（`export const VERSION`）。

## 4. 取消与停止

| 语义 | 真实行为 | 出处 |
| --- | --- | --- |
| 中止当前任务 | 命令 `{"type":"abort"}` → 响应 `success:true`；agent 流以 `message_update.assistantMessageEvent.error(reason:"aborted")` / assistant 消息 `stopReason:"aborted"` 收尾，最终必出 `agent_settled`。进程存活，可继续下一个 prompt | `rpc-mode.ts` case `"abort"`；`docs/rpc.md` |
| 中止 bash / 中止重试 | `abort_bash`、`abort_retry` | 同上 |
| 退出进程 | **无 exit/quit/shutdown 命令**。三条路径：① 客户端关闭 stdin → `stdin end` → 优雅 shutdown（dispose 运行时、flush stdout、`exit 0`）；② SIGTERM → 杀 tracked 子进程、`exit 143`（不 flush stdout）；③ 非 Windows 另挂 SIGHUP（129）。扩展也可经 `shutdownHandler` 请求退出（在 `agent_settled` 后生效） | `rpc-mode.ts`（`shutdown()`/`registerSignalHandlers`/`onInputEnd`） |
| Windows 注意 | SIGHUP 不注册；SIGTERM 依赖 Node 对 `process.kill` 的模拟。**优雅停止的可靠手段是关 stdin**，宽限期后强杀 | 同上 |

## 5. 配置注入（决定受管启动的注入方式）

### 5.1 CLI 参数（`src/cli/args.ts`）

- 模型：`--provider <name>`、`--model <pattern>`（支持 `provider/id` 与 `:<thinking>` 后缀如 `sonnet:high`）、`--models <p1,p2>`（模型循环范围）。
- 思考等级：`--thinking off|minimal|low|medium|high|xhigh|max`（显式值优先于 `--model` 后缀）。
- 凭据：`--api-key <key>`（要求同时给出模型；仅运行期覆盖不落盘）。**不采用**——Windows 下进程命令行对其他进程可见，违反凭据红线。
- 会话：`--no-session`（内存会话不落盘）、`--session-dir <dir>`、`--session-id`、`--name/-n <显示名>`、`-c/--continue`、`--resume`、`--fork`。
- 资源开关：`--no-extensions/-ne`、`--no-skills`、`--no-prompt-templates`、`--no-themes`、`--no-context-files`、`--no-tools`、`--tools/-t <list>`、`--exclude-tools/-xt <list>`、`--system-prompt`、`--append-system-prompt`。
- 项目信任：`--approve/-a`（信任项目级 `.pi` 资源）、`--no-approve/-na`（拒绝）。RPC 模式无 UI，**不给覆盖标志时信任提问一律返回否**（`src/cli/project-trust.ts`：非 interactive 模式 select/confirm 直接返回 undefined/false）。
- 其他：`--offline`（= `PI_OFFLINE=1` + 跳过版本检查）、`--list-models`、`--export`。

### 5.2 环境变量

| 变量 | 作用 | 出处 |
| --- | --- | --- |
| `ANTHROPIC_API_KEY`（`ANTHROPIC_OAUTH_TOKEN` 优先于它） | Anthropic 凭据 | `packages/ai/src/env-api-keys.ts`（envMap）；`docs/providers.md` |
| `OPENAI_API_KEY` / `GEMINI_API_KEY` / `DEEPSEEK_API_KEY` / `GROQ_API_KEY` / `MISTRAL_API_KEY` / `XAI_API_KEY` / `OPENROUTER_API_KEY` / `ZAI_API_KEY` / `KIMI_API_KEY` / `MINIMAX_API_KEY` / `HF_TOKEN` / `AWS_BEARER_TOKEN_BEDROCK` / `AZURE_OPENAI_API_KEY` 等 | 各 provider API key（完整映射见 `docs/providers.md` 表格与 `env-api-keys.ts`） | 同上 |
| `PI_CODING_AGENT_DIR` | 覆盖 agent 配置目录（默认 `~/.pi/agent/`，内含 `auth.json`、`settings.json`、`models.json`、`sessions/`、`extensions/`、`skills/`） | `src/config.ts`（`ENV_AGENT_DIR`/`getAgentDir()`） |
| `PI_CODING_AGENT_SESSION_DIR` | 覆盖会话存储目录（默认 `~/.pi/agent/sessions/<编码后cwd>/`）；被 `--session-dir` 覆盖 | `src/config.ts`（`ENV_SESSION_DIR`）；`src/core/session-manager.ts` |
| `PI_OFFLINE` / `PI_SKIP_VERSION_CHECK` | 离线模式 / 跳过启动版本检查（避免出网） | `src/main.ts` |
| `PI_PACKAGE_DIR` | 覆盖包目录探测 | `src/config.ts` |
| `PI_CODING_AGENT=true` | Pi 自身设置，标记子孙进程 | `src/cli.ts` |

### 5.3 配置文件

- `~/.pi/agent/auth.json`：`{"anthropic":{"type":"api_key","key":"sk-ant-..."}}` 形状，也存 OAuth token（`/login` 写入，自动刷新）。**这是明文凭据文件**——受管模式下我们不写它，改用环境变量注入。
- `~/.pi/agent/settings.json` + 项目级 `.pi/settings.json`：默认 provider/model/thinking、httpProxy、compaction 等。
- `~/.pi/agent/models.json`：自定义 provider/模型目录；`models-store.json` 为目录缓存。
- 出处：`docs/providers.md`、`docs/settings.md`、`src/core/settings-manager.ts`。

## 6. 差异表：真实协议 vs 假设协议（module-contracts.md 第 5 节）

| # | 维度 | 假设协议（现契约） | 真实协议（pi 0.81.1） | 差异级别 |
| --- | --- | --- | --- | --- |
| 1 | 启动参数 | `<exe> --rpc` | `<exe> --mode rpc`（或 `dist/rpc-entry.js`）；建议附 `--no-session -na` 等 | 参数改名 |
| 2 | 封包 | JSON-RPC 风：`{"id":1,"method":"...","params":{...}}` | 平铺命令：`{"id?":"str","type":"...",...字段}`；`id` 是**可选 string** 而非 number | **结构性** |
| 3 | 响应 | `{"id":N,"result":{...}}` | `{"id?","type":"response","command","success",data?/error?}` | **结构性** |
| 4 | 就绪探测 | `get_state` → `{"result":{"state":"idle"}}` | 有 `get_state` 但无 `state` 字段；空闲 = `isStreaming=false && isCompacting=false`；启动无握手输出，探测方式（发命令等响应）本身成立 | 字段映射 |
| 5 | 任务提交 | `run_task{instructions,files,base_diff,notes}` | **无 run_task**。只有 `prompt{message,images?,streamingBehavior?}`；files/base_diff/notes 需拼装进 message 文本 | **结构性** |
| 6 | 任务完成 | `run_task` 的最终响应 `{"result":{"outcome":"finished"/"failed","summary"}}` | **无 per-task 终局响应**：`prompt` 响应只表示"已接受"；终态信号是 `agent_settled` 事件；summary 需另发 `get_last_assistant_text`；失败从 `stopReason:"error"`、`auto_retry_end{success:false}`、`extension_error` 推断 | **结构性** |
| 7 | 事件流 | `{"method":"event","params":{kind: phase/agent_note/file_hint/action_request/verification}}` | 顶层 `type` 平铺的 `AgentSessionEvent`（§2.4）；**无 phase/file_hint/verification 概念**，文件变更需从 `tool_execution_*`（edit/write 工具 args）推断 | **结构性** |
| 8 | 权限请求 | 事件 `kind:"action_request"` | 核心无权限系统；仅扩展触发 `extension_ui_request`（select/confirm/input/editor + 应答子协议，支持 timeout 自动默认值） | **模型差异** |
| 9 | 取消 | `{"method":"cancel"}` 结束 run_task 并回 result | `{"type":"abort"}` → `success:true`；流以 aborted 收尾 + `agent_settled`；进程存活 | 语义相近、话术不同 |
| 10 | 退出 | 未定义（依赖 kill） | 无 exit 命令；**关 stdin = 优雅退出(0)**；SIGTERM=143；Windows 首选关 stdin + 宽限强杀 | 补充定义 |
| 11 | 版本探测 | `--version` 首行 semver | `--version`/`-v` 输出裸 semver（如 `0.81.1`），exit 0 | **一致** |
| 12 | 帧纪律 | JSONL + `MAX_LINE_BYTES=1MB` | JSONL（仅 LF；剥 `\r`；禁 U+2028/2029 分行）；**Pi 侧无行上限**，`get_messages`/`get_entries` 响应可远超 1MB | 上限需分层 |
| 13 | 凭据注入 | 未定 env 名 | `ANTHROPIC_API_KEY`（`ANTHROPIC_OAUTH_TOKEN` 优先）、`OPENAI_API_KEY`、`GEMINI_API_KEY` 等（§5.2）；或 auth.json / `--api-key`（后两者违反我们凭据红线，不用） | 补真实值 |
| 14 | thinking level | `ThinkingLevel`（我方枚举） | `off/minimal/low/medium/high/xhigh/max` 七档；经 `--thinking` 或 `set_thinking_level` 注入 | 枚举对齐 |
| 15 | 会话模型 | 无（一任务一 RPC 调用） | 常驻会话树（JSONL 持久化、fork/branch/compaction）；`--no-session` 可退化为纯内存 | 模型差异 |

## 7. 适配器 v2 具体映射建议（halo-runtime PiRuntime/PiHandle）

### 7.1 探测与启动

- `probe(exe)`：`<exe> --version` → 首行裸 semver。**保持现签名**。
- `start(cmd)`：参数固定为
  `--mode rpc --no-session -na --no-extensions --thinking <level> --model <provider>/<model_id>`
  - `--no-session`：任务级隔离，避免写 `~/.pi/agent/sessions`（后续若做"会话续跑"再改受管 `--session-dir`）。
  - `-na`（--no-approve）：显式拒绝项目级 `.pi` 资源信任，防止工作区内恶意 `.pi/extensions` 被加载（RPC 模式无 UI，缺省已是拒绝，显式传参消除歧义）。
  - `--no-extensions`：v2 先关闭扩展面，协议面最小化；代价是没有 `extension_ui_request` 通道（见 7.5）。
  - 模型/思考等级由 LaunchConfig 的 `model`/`thinking_level` 映射；`thinking_level` 枚举需与 Pi 七档对齐（至少映射 off/low/medium/high）。
- 环境注入（`halo-config::build_child_env` 的 injected 通道）：
  - `credential_ref` → 按 provider 映射 env 名：anthropic→`ANTHROPIC_API_KEY`、openai→`OPENAI_API_KEY`、google→`GEMINI_API_KEY`（其余按 §5.2 表扩展）。**绝不使用 `--api-key`（命令行可见）、绝不写 auth.json（明文落盘）**。
  - `PI_CODING_AGENT_DIR=%LOCALAPPDATA%\HaloStudio\pi-agent`：把配置目录指向受管空目录，隔离用户全局 auth.json/extensions/settings，保证行为可复现。
  - `PI_SKIP_VERSION_CHECK=1`（或 `PI_OFFLINE=1`，若允许模型目录走内置缓存）：削减启动出网。
  - `ENV_WHITELIST` 现值已覆盖 Pi 依赖（`USERPROFILE` 供 homedir、`PATH`、`TEMP/TMP`）；如用户配置代理需追加 `HTTP_PROXY/HTTPS_PROXY` 进白名单评估。
- 失败关闭：启动后进程早退 + stderr 含 "No models"/凭据指引 → `Failed{reason: PI_NO_MODEL_OR_CREDENTIAL}`。

### 7.2 就绪（get_state）

- 发 `{"type":"get_state","id":"<uuid>"}`，在 `Timeouts.ready`（默认 10s）内等 `command=="get_state" && success` 的响应 → `Ready`。
- 空闲判定改为 `data.isStreaming==false && data.isCompacting==false`；不再匹配 `state:"idle"` 字符串。

### 7.3 run_task 映射

- `RunTaskSpec{instructions,files,base_diff,notes}` → 单条 `prompt`：
  - `message` = 模板拼装（任务指令 + 关注文件清单 + 基线 diff 摘要 + 备注），一次任务一条 prompt，适配器内部串行，不使用 steer/follow_up（v2 范围外）。
  - 帧：`{"type":"prompt","id":"task-<uuid>","message":"..."}`。
- 响应语义：`success:true` 仅表示接受；**响应可能晚于首批事件到达**，关联逻辑必须容忍乱序。`success:false` → 任务直接 `Failed{reason}`。

### 7.4 事件规范化（Pi 事件 → RuntimeEvent/RuntimeTraceItem）

| Pi 事件 | 映射 |
| --- | --- |
| `agent_start` | `Trace{kind:"phase", text:"started"}` |
| `message_update` + `text_delta`/`thinking_delta` | 聚合为 `Trace{kind:"agent_note"}`（按块聚合，勿逐 delta 发事件，防事件风暴；detail 里保留块类型） |
| `tool_execution_start/end` | `Trace{kind:"tool", text:"<toolName>"}`；当 `toolName ∈ {edit,write}` 时额外发 `Trace{kind:"file_hint", text:<args.path>}`。**文件归因的权威仍是我们的 git 基线算法（module-contracts §6），file_hint 仅作 UI 提示** |
| `auto_retry_start/end`、`compaction_start/end` | `Trace{kind:"phase"}` |
| `agent_settled` | 终态收口：随后发 `{"type":"get_last_assistant_text","id":...}` 取 summary → `TaskDone{outcome, summary}` |
| outcome 判定 | 最后一条 assistant `stopReason=="stop"/"toolUse"` → `finished`；`"aborted"` → 取消路径；`"error"` 或 `auto_retry_end{success:false}` → `failed` |
| `extension_error` | `Trace{kind:"warning"}`（v2 关扩展后理论上不出现） |
| verification | Pi **无验证概念**；`RuntimeEvent::Verification` 在 Pi 适配器 v2 不再由协议产生，由任务编排层（task_flow）依据用户标记/后续验证命令生成 |

### 7.5 ActionRequest（权限/确认）

- v2 决策：`--no-extensions` 下 Pi 不会发任何 `extension_ui_request`，`RuntimeEvent::ActionRequest` 在 Pi 路径**暂时不产生**——与我们"审查只读、基线归因兜底"的模型一致（Pi 工具直接执行，事后由证据/审查把关）。
- 若未来需要执行前审批门禁：保留扩展面（去掉 `--no-extensions`），注入我们自己的受控扩展（经 `--extensions <path>` 指定受管目录），把工具调用转成 `ctx.ui.confirm()`；届时映射为：`extension_ui_request{id,method,title,message,timeout}` → `ActionRequest{request_id:id, kind:method, prompt:title+message}`，用户决定 → `{"type":"extension_ui_response","id":...,"confirmed":bool}`（或 `value`/`cancelled:true`）。timeout 由 Pi 侧自动默认值兜底，适配器无需守时。
- 通知类 `extension_ui_request`（notify/setStatus/…）→ 降级为 `Trace` 或丢弃。

### 7.6 取消与停止

- `cancel_native()` → `{"type":"abort","id":...}`；等待 `agent_settled`（宽限 `cancel_grace` 10s），到达 → 任务 `Cancelled`；未到达 → 走 stop 强杀路径。
- `stop(grace)` → **关闭子进程 stdin**（Pi 收 EOF 优雅退出 exit 0）→ 宽限内退出 = `StopOutcome::Graceful`；超时 kill = `Forced`。SIGTERM 在 Windows 不可靠，不作为首选。
- EOF/坏帧：Pi stdout 提前 EOF 或连续无法解析 → `Failed{reason}`（现语义保持）。

### 7.7 帧读取与上限

- 读取器必须：仅按 LF 切分、剥尾部 `\r`、不把 U+2028/2029 当换行。
- **入站行上限不能沿用 1MB 全局值**：`message_update` 帧含全量 `partial` 消息，可超 1MB。建议 Pi 适配器读取上限独立设为 16MB，并且 v2 避免调用 `get_messages`/`get_entries`/`get_tree` 这类全量命令（只用 `get_state`/`get_last_assistant_text`）。`halo-protocol::MAX_LINE_BYTES` 只约束我们自己的 UI↔Sidecar IPC，不约束 Pi 适配器。

### 7.8 halo-testkit fake-pi 同步修改

- `fake-pi` 按本文档协议重写：`--mode rpc`/`--version` 裸 semver、平铺命令/响应、`prompt`→事件流→`agent_settled`、`abort`、stdin EOF 退出；各 `FAKE_PI_MODE` 场景（not_ready=不回 get_state 响应、crash_mid_task=事件流中途退出、hang_on_cancel=无视 abort、action_request=发 `extension_ui_request`）在新话术下语义不变。

## 8. 修订记录

- 2026-07-27 初版：基于 pi-main 0.81.1 源码与官方 `docs/rpc.md` 的协议还原与差异分析。
