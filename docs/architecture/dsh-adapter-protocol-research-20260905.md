# DSH 0.1.3-alpha.1 `sdk` / `acp` profile 进程接入协议研究（2026-09-05）

> 状态：研究输入，回答 wayfinder 票 #38；架构级事实（Cordis 插件模型、事件域三分法、SessionEvent 词汇、approval/sandbox 契约）见 `docs/architecture/dsh-upstream-state-research-20260905.md`，本文只补**协议级细节**，不重复。
> 主源：本地 checkout `D:\DeepSeek Harness\deepseek-harness`（HEAD `d347e70390`，0.1.3-alpha.1，2026-09-04）。
> 行文约定：以下 `DH` 指该 checkout 根；引用形如 `DH\path\to\file.ts:123`（本地文件路径 + 行号）。对照物：`docs/development/pi-rpc-adapter.md`（Halo 现有 Pi RPC 适配器的档案/握手/取消模式）。

## 0. TL;DR

- `sdk` profile 是一个**极小的私有 JSON-RPC 2.0 over stdio 协议**：3 个客户端→服务器请求（`initialize` / `session/prompt` / `shutdown`）+ 4 个服务器→客户端通知（`session.event` / `session.status` / `subagent.started` / `subagent.finished`），newline-delimited 单行 JSON 框架。**没有 wire 级取消、没有 session/resume**——取消 = 关子进程，会话生命周期 = 子进程生命周期。
- `acp` profile 说的是**标准 Agent Client Protocol（ACP）v1**（`@agentclientprotocol/sdk`），stdio ndjson 传输；有 `session/new|list|resume|close|prompt|cancel|set_config_option`、`session/update` 语义更新流、`session/request_permission` 一次性机器决议。事件粒度是 **committed 语义更新**（消息/工具生命周期/config/usage），不是 token 流。
- 两者凭据都走 **CredentialRef（环境变量名）per-request 解析**：纯 env 注入即可完全不落盘（子进程 env + `DSH_HOME` 隔离 + `--patch`/`DSH_PERMISSION_MODE` 等 env 开关）。
- Windows 是一等关注点：junction symlink、pwsh 终端/tool、win-x64 打包 runtime、dispose 阶梯在 Windows 直接强杀。
- 对 Halo halo-dsh-adapter：**首选 `sdk` profile（协议面小、与现有 pi-rpc-adapter 的「关 stdin→回收」回收阶梯同构）；`acp` 作为需要跨进程会话恢复时的备选；`sdk-minimal` 只作协议金丝雀/降级通道，不作主接入**（无审批行 + danger-full-access 与 Halo 一次性决议要求冲突）。

---

## 1. 启动方式、命令行参数与入口包路径

### 1.1 共同的 launcher 骨架（app-boot profile 机制）

- CLI 可执行入口是 npm 包 `@deepseek-ai/dsh`（`DH\apps\cli\package.json`，`"bin": {"dsh": "lib/bin.js"}`）；源码入口 `DH\apps\cli\src\bin.ts:24-36`。
- launcher 只解析自己的 flags：`--profile <name>`、`--patch <path>`（可重复）、`--dump-config`、`--dump-default-config`；**第一个不认识的 token 之后的所有 argv 原样（verbatim）交给被 boot 的 app**（`DH\apps\cli\src\args.ts:112-145`，help 文本示例 `args.ts:64-72`）。
- profile = `$DSH_HOME/profiles/<name>` 目录，内含 `package.json`（manifest `dsh.profile.bundles` 有序 bundle 列表）+ 用户自己的 `cordis.patch.yml` 补丁层（`DH\packages\boot\app-boot\src\profile.ts:1-24`）。首次使用按 shipped 模板自动初始化（`profile.ts:137-158` `PROFILE_TEMPLATES`）：
  - `acp` → `['@deepseek-ai/dsh-base', '@deepseek-ai/dsh-acp-app']`，patchReload `startup`
  - `sdk` → `['@deepseek-ai/dsh-base', '@deepseek-ai/dsh-sdk-app']`，patchReload `startup`
  - `sdk-minimal` → `['@deepseek-ai/dsh-sdk-minimal']`（**不叠 dsh-base**），patchReload `startup`
- patch 层叠放顺序：bundle 层（按 `dsh.profile.bundles` 序）→ profile 自己的 `cordis.patch.yml` → `$DSH_HOME/cordis.patch.yml`（home 层，机器本地偏好，优先级高于 profile 层）→ `--patch` overlays → `DSH_TELEMETRY_DISABLED` 开关补丁（`DH\apps\cli\src\profile-boot.ts:126-174`）。
- 模块解析双锚点：bundle 先从 dsh 安装本体解析、再从 profile 目录解析（`profile.ts:778-789`）；Windows 上 profile 模块 fallback 用 **junction symlink**（`profile.ts:255` `symlinkSync(target, link, 'junction')`）。
- profile 目录每次 boot 都会重写空根 `cordis.yml`（整个树由补丁层组成）（`profile-boot.ts:106-123`）。

### 1.2 `sdk` profile

- 启动命令：`dsh --profile sdk [--patch <yml>]...`。app 插件是**零选项命令**，自带 help：`dsh --profile sdk` "Serve DeepSeek Harness SDK clients over stdio JSON-RPC."（`DH\packages\bundle\sdk-app\src\index.ts:38-47`）。成功 parse 后发布 `sdkAppStartup` 服务并 `exitOnStdinEnd`——**stdin EOF = 有界优雅关停**（`sdk-app\src\index.ts:55-62`；`DH\packages\boot\cmdline\src\index.ts:112-145`）。
- 树组成（`DH\packages\bundle\sdk-app\cordis.patch.yml`）：dsh-base 之上 insert 两行——`sdk-app-startup`（:12-15）与 `sdk-jsonrpc-server`（`@deepseek-ai/dsh-sdk-jsonrpc-server`，inject `[sdkAppStartup, loader]`，:17-21）；`maxTokensAsSuccess` 由 env `DSH_MAX_TOKENS_AS_SUCCESS` 控制、**默认 true**（:21）。文件头注释明确 "Stdout belongs exclusively to JSON-RPC"（:1）。
- 服务器插件：`DH\packages\sdk\server\src\index.ts:46-102`。stdio 上挂 `JsonRpcLineTransport(process.stdin, process.stdout)`（:59）；**`initialize` 请求会先 `await ctx.get('loader')?.await()` 等 Loader 树 settle** 才应答（:84-86），所以握手返回 = 运行时真正就绪；`shutdown` → flush → 根 fiber dispose → `exit(0)`（:66-93）。
- TS 客户端侧的启动解析（`DH\packages\sdk\client\src\launch.ts:128-157` `resolveDshLaunch`）：默认 profile `sdk`（:132）；最终 argv = `[node, dshBin, '--profile', profile, '--patch', p1, '--patch', p2, ...]`（:143）；`dshHome` 选项 → 子进程 env `DSH_HOME`（:140,148）；`env` 选项**整体替换**子进程环境（`DH\packages\sdk\client\src\types.ts:35-43`，注释指明调用方借此拥有凭据策略，并提示 `scrubbedParentEnv` 模式）；`dshBin` 未指定时按 package manifest 解析并**强校验 dsh 与 client 版本完全一致**（`launch.ts:55-66`）。
- Python SDK 是同协议孪生（`DH\python\README.md:3-13`）：自带打包 `dsh` runtime wheel，**强制显式指定 Harness home，绝不默默读 `~/.dsh`**（`python/README.md:15`、`python/sdk/README.md:15`）。

### 1.3 `acp` profile

- 启动命令：`dsh --profile acp`，同样零选项 app（`DH\packages\bundle\acp-app\src\index.ts:25-34`），成功 parse → `acpAppStartup` + stdin EOF 绑定（:41-48）。
- 树组成（`DH\packages\bundle\acp-app\cordis.patch.yml`）：dsh-base 之上 insert `acp-app-startup`（:12-14）与 `acp`（`@deepseek-ai/dsh-acp`，config `provider: deepseek-official` / `model: deepseek-v4-flash`，:15-20）。
- 服务器插件 `apply`（`DH\packages\acp\acp\src\index.ts:97-437`）：用 `@agentclientprotocol/sdk` 的 `agent()` app + `ndJsonStream(Writable.toWeb(process.stdout), Readable.toWeb(process.stdin))` 接 stdio（:374-391）；测试可注入 `config.stream`（:83）。`inject = ['agents','llm','sessionPersistence','sessions']`（:62）——**ACP profile 必须有持久化**。
- 上游自己的 ACP 客户端是 `dsh-subagent-acp`（`DH\packages\acp\acp\README.md` "Start a server" 一节）；没有官方 TS/Python 通用客户端包（TS client 只服务 sdk 协议）。

---

## 2. 传输形态、消息框架、会话建立与恢复

### 2.1 `sdk`：newline-delimited JSON-RPC 2.0 over stdio

- **传输**：纯 stdio（`stdio: ['pipe','pipe','pipe']`，`DH\packages\sdk\client\src\client.ts:214-218`）。没有 HTTP 端口、没有 socket 选项（服务器插件只留测试用 `input/output` hook，`server\src\index.ts:53-55`）。
- **消息框架**：每帧一行 `JSON.stringify(message) + '\n'`（`DH\packages\sdk\protocol\src\transport.ts:260-262`）；`id+method`=请求、仅 `id`=响应、仅 `method`=通知（:1-7）；畸形行**静默忽略**（:201-209）；缺 handler 的请求回 `-32601`、handler 抛错回 `-32603`（:226-238）。与 Halo pi-rpc-adapter 的「严格 LF JSONL framing」同型（`docs/development/pi-rpc-adapter.md` §2）。
- **方法面**（`DH\packages\sdk\protocol\src\types.ts:114-119`）：
  - `initialize {cwd, provider, model, reasoningEffort?, maxTokens?} → {serverInfo:{name:'deepseek-harness-sdk-runtime', version}}`（:16-33；name 固定值见 `server\src\server.ts:168`）。服务器在握手时校验 provider/model 路由（无适配器且 provider 非 `deepseek-official` 直接失败；是则挂 DeepSeek fallback adapter，`server.ts:135-169`），`maxTokens` 继承给 SDK 创建的 agent 及其进程内后代。
  - `session/prompt {sessionId, contentBlocks} → {messageId}`（:36-59）；inline base64 图像块会在服务器侧转为持久 attachment（`server.ts:39-52`）。
  - `shutdown {} → {}`。
- **会话建立**：无显式 open——`session/prompt` 对未知 `sessionId` 惰性 `ctx.agents.create`（`server.ts:259-292`），sessionId 由**客户端任意指定**。同一 runtime 进程内复用 sessionId 即延续会话；每个 session 一个 `AgentHandle`，prompt 前校验 agent 仍活着（`server.ts:195-199`）。
- **会话恢复**：**协议面不存在**。请求 map 里没有 resume/list（`types.ts:114-119`）；且新 runtime 进程对已持久化的同 id 会话会在 `persistence.create` 处抛 `SessionAlreadyExistsError`（`DH\packages\session\session-persistence-jsonl\src\index.ts:229-231`）——所以跨进程重放同 sessionId 到 `sdk` profile 是**硬失败**，不是续聊。持久化本身仍发生（dsh-base 的 `session-persistence-jsonl` 写 `$DSH_HOME/sessions`，`DH\packages\bundle\base\cordis.patch.yml:110-113`），只是 SDK 协议打不开它。`agents.resume`（`resumeSessionId`）在 Cordis 层存在（`DH\packages\core\agent\src\index.ts:419-433`），但没被接到 SDK wire 上。
- **关闭**：协议 `shutdown` → 顺序 dispose 订阅、SDK agents、fallback adapter → 返回后 transport flush → 根 fiber dispose → exit 0（`server.ts:206-237`；`server\src\index.ts:66-93`）。客户端 `close()` = shutdown 请求（默认 1s 界）→ **stdin EOF（默认 6s 协作 quiesce）→ POSIX SIGTERM（3s）→ SIGKILL** 阶梯；Windows 跳过 SIGTERM 直接强杀（Node 把两个信号都映射到 `TerminateProcess`）（`DH\packages\sdk\client\src\client.ts:394-410`；`dispose.ts:82-99`）。这与 pi-rpc-adapter 的「abort → 宽限 → 关 stdin → 回收」语义同构。

### 2.2 `acp`：标准 ACP v1 over stdio ndjson

- **传输**：stdio ndjson Stream（`index.ts:374-377`）；协议版本 = `@agentclientprotocol/sdk` 的 `PROTOCOL_VERSION`（:180-189 应答）。`Config.stream` 仅测试注入，生产恒 stdio。**没有 HTTP 端口**（`mcpCapabilities.http: true` 指的是它支持挂 HTTP 型 MCP 服务器，不是自身传输）。
- **能力宣告**（`index.ts:175-190`）：`agentInfo {name:'deepseek-harness-acp', version:'0.0.1'}`；`sessionCapabilities {close, list, resume}`；`promptCapabilities {image: 按路由能力, audio: false, embeddedContext: false}`；`authMethods: []`，`authenticate` 立即成功（:192-194）。
- **会话建立** `session/new`（:196-237）：要求绝对路径 cwd、拒绝 `additionalDirectories`（:516-525）；`mcpServers`（stdio 需绝对路径 command+env；http 需 http(s) URL+headers）先校验后挂载，失败即回滚未发布的 Agent（`DH\packages\acp\acp\src\mcp.ts:36-74`）；成功 = sessionId（服务器生成的 UUID）+ 完整 `configOptions`（模型/reasoning-effort 选择器，`session.ts:197-212`）。
- **会话恢复** `session/list`（:292-331）：枚举持久化的、非 subagent、带 cwd 的根会话，keyset 分页（base64url 不透明 cursor）；`session/resume`（:239-290）：校验「未激活 + 持久化存在 + cwd 物理等同（realpath）」后 `ctx.agents.resume`，**恢复历史但不重放旧 update**（README protocol contract 行 67，`DH\packages\acp\acp\README.md`）。`session/close` 只处置被点名的 Agent scope，其余会话不受影响（:347-359；quiescent teardown `session.ts:431-471`：取消→drain→回收 continuable 子代理→persistence flush→dispose）。
- **每会话一个 in-flight prompt**（`session.ts:252`）；`session/prompt` 结算发生在 **Agent idle + 有序 update 排空之后**（quiescence-before-settlement，`session.ts:329-330, 486-526`）。

---

## 3. 事件流形态与 abort/取消语义

### 3.1 `sdk`

- `session.event`：把 **每一条已提交的 SessionEvent 原封广播**——`ctx.on('session/event')` → `transport.notify('session.event', {sessionId, event})`（`DH\packages\sdk\server\src\server.ts:95-98`）。注意广播范围是 **runtime 内所有会话**（含非 SDK 创建的），文档明示 "every session in the runtime, not only SDK-created ones"（`types.ts:66-70`）；按会话/子代理谱系过滤是**客户端职责**（`client.ts:370-381` `subscribeSessionTree` 用 `subagent.started` 谱系边推导后代）。
- `session.status`：`agent/status` → `{sessionId, status: 'idle'|'running'}`（`server.ts:99-101`；`types.ts:72-78`）。
- `subagent.started` / `subagent.finished`：`session/created`（有 parent）与 `subagent/end`；**只报 in-process local 子代理**（`server.ts:102-127`），`finished` 带 provider/stopReason/lastAssistantMessage，`max-tokens` 可按 `maxTokensAsSuccess` 映射为 `ok`（:60-68）。
- 事件粒度 = **durable 事件**（无 token 级增量）；`assistant/message` 内嵌精确压缩模型流（见上游研究文档 §3，勿重复）。
- 高层 `run()` 语义（`DH\packages\sdk\client\src\api.ts:176-224`）：prompt → 等到自己的 inbox receipt（`agent/inbox/spliced` 含返回的 messageId，:289-293）→ 收集至下一个 `session.status: idle`。`finalResponse` 是区间内**最后一条** assistant 文本，不是因果归因（`api.ts:300-310`；client README 同义）。
- **abort/取消：协议层不存在**。客户端 docstring 原文："There is no wire-level cancel: a timed-out request stays running server-side until the runtime is closed"（`client.ts:176-184`）；请求超时只是客户端**放弃等待**（从 pending 表删除条目并拒绝，服务器侧工作继续跑到底；`transport.ts:121-137`、`client.ts:320-335`）。`turn/end` 的 `aborted` reason（`user/parent/disposed/legacy/hook`）只反映 **runtime 内部**取消源（`api.ts:237-261`）。

### 3.2 `acp`

- `session/update` 通知，全部由**已提交 SessionEvent 投影**（`DH\packages\acp\acp\src\updates.ts:16-102`）：
  - `assistant/message` → 按内容块顺序的 `agent_thought_chunk` / `agent_message_chunk` + 可选 `usage_update`（used/size）；
  - `tool/call` → `tool_call {toolCallId, title=工具名, kind:'other', status:'in_progress', rawInput}`（arguments 解析失败时原样透传为 opaque，:104-111）；
  - `tool/result` → `tool_call_update {status: 'completed'|'failed', content}`。
- 每会话 update **串行化**（`outputTail` 链，`session.ts:348-392`）；模型 config 变化推 `config_option_update`（:214-237）。
- **取消双路径**（`DH\packages\acp\acp\src\index.ts:361-370`）：`session/cancel` 通知（无 in-flight prompt 时取消 autonomous work；未知 session 是 no-op）与 JSON-RPC `$/cancel_request`（requestSignal abort，进入 `prompt` 的 abort 监听，`session.ts:272-275`）。取消语义：admission 阶段 abort 直接不排队（:314-317）；已排队则 `agent.cancel({kind:'user'})` + 等待 quiescence，最终以 `stopReason: 'cancelled'` 结算（`session.ts:336-341, 477-526`）。
- stopReason 映射（`DH\packages\acp\acp\src\codec.ts:14-34`）：`completed→end_turn`、`max-tokens→max_tokens`、`aborted→end_turn`（hook 等所有者取消视为普通结束）、`interrupted→cancelled`、`blocked/error→end_turn`。
- **权限决议**：`approval/request` 瀑布 → `session/requestPermission {sessionId, toolCall:{toolCallId}, options:[allow-once, reject-once]}` 一次性机器应答；`cancelled` outcome → `'cancelled'`，**从不对未知响应推断持久放行**（`index.ts:155-173`）。与 Halo「一次性决议」（ADR-0012）直接同构。

---

## 4. 凭据与配置注入路径、Windows 可用性

### 4.1 凭据：只传引用、可以完全不落盘

- 配置面只有 **CredentialRef**（环境变量名，branded shell-identifier）；值由 credential provider 持有，消费方 per-operation 解析、从不缓存（`DH\docs\subsystems\credentials.md` 开头 Identity/Resolution 节）。DeepSeek adapter 的 `apiKeyEnv` 默认 `DEEPSEEK_API_KEY`，schema 标注 `role('credential-ref')`，**每次模型请求重新解析**（`DH\packages\llm\llm-deepseek\src\index.ts:126-127, 178, 377`；`adapter.ts:81-87` 注释："Configuration carries only this name — a literal key is not a configuration value"）。
- 解析顺序（`index.ts:430-446` `resolveApiKey`）：挂了 `credentials` 服务（dsh-base 挂 `dsh-credentials-local`：受管 `$DSH_HOME/.credentials.yaml`，**env 永远优先**，project/user `.env` 兜底；受管文件"从不物化进进程环境"，`DH\packages\bundle\base\cordis.patch.yml:97-99`）→ 否则回落 launch environment。**空值视为不存在**（credentials.md seam-wide rule）。
- 纯 env 注入路径（不落盘）：子进程 env 直接带 `DEEPSEEK_API_KEY` 即可——TS 客户端 `env` 整体替换子进程环境并明示"callers own credential policy"（`DH\packages\sdk\client\src\types.ts:35-43`）；Python 客户端 `api_key`/`base_url` 显式覆盖 child env（`DH\python\sdk\README.md:33`）。`DEEPSEEK_API_KEY=… dsh` 每次运行覆盖存储值是官方口径（`DH\packages\credentials\README.md` Summary）。
- **home 隔离**：`DSH_HOME` env（`DH\packages\util\home-paths\src\index.ts:17-18, 87-91`；优先级 configured > `$DSH_HOME` > `~/.dsh`）+ 客户端 `dshHome` 选项（`launch.ts:140,148`）。Python SDK 更进一步**强制**显式 home（`python/README.md:15`）。
- env 分层快照：`loadLayeredEnv` = 继承环境 > 调用目录 `.env` > home `.env`，且 `DSH_*`/`XDG_*` 等 bootstrap 名**禁止出现在任何 `.env`**（决定进程怎么启动/代码从哪加载；`DH\packages\boot\app-boot\src\index.ts:120-141, 198-219`）——给 Halo 的启示：受管子进程的 `.env` 不能成为配置注入通道，env 本身可以。
- 配置注入（不动 profile 文件）：`--patch` overlay（repeatable，`args.ts:132`）、profile/home 两级 `cordis.patch.yml`、`!!js` 表达式可读 `process.env`（patch 模板，`profile.ts:171-175`）。dsh-base 还暴露纯 env 开关：`DSH_PERMISSION_MODE`（sandbox-policy 默认 workspace-write，`base\cordis.patch.yml:214-218`；approval policy 联动，:230-234）、`DSH_TELEMETRY_DISABLED`/`DSH_TELEMETRY_MODE`/`DSH_TELEMETRY_OTLP_URL`（:186-212 区域）、`DSH_MAX_TOKENS_AS_SUCCESS`（sdk-app patch :21）、`DSH_CONTEXT_WINDOW`/`DSH_SYSTEM_PROMPT`（sdk-minimal patch :30, 96）。
- **ACP 会话级注入**：`mcpServers`（stdio env / http headers）随 `session/new`/`session/resume` 传入，是另一个"只传引用/瞬时凭据"入口（`index.ts:206-214, 257-265`；`mcp.ts:36-74`）。
- 对 Halo 契约的差距点：**SDK/ACP 协议面上没有"按请求传凭据"的方法**——凭据只在子进程 env / 受管 store / MCP headers 三个入口出现，Halo 的「凭据明文只在启动瞬间短暂存在」模型与 env 注入路径兼容，与"传引用"要求兼容（引用=CredentialRef 进 patch/初始化参数，值走 env）。

### 4.2 Windows 可用性

- 代码级 Windows 适配遍布主源：env 名大小写折叠（`DH\packages\util\launch-environment\src\index.ts:60-63`）；profile 模块 fallback 用 junction（`profile.ts:255`）；dispose 阶梯在 Windows 直接强杀（`dispose.ts:92-96`）；dsh-base 的 bash 工具/沙箱行按 `process.platform` 门控切 pwsh（`base\cordis.patch.yml:221-227, 253-257`；sdk-minimal patch :53-64, 130-157）。
- 打包：Python runtime wheel 官方平台含 **win-x64**（`DH\python\sdk-runtime\platforms.json`）。
- 已知保留风险：Windows 沙箱 = 受限令牌 ACL，自报 **partial enforcement**（上游研究文档 §4 已载，此处不重复）；sdk-minimal 的 `danger-full-access` 默认值部分规避但引入另一问题（见 §5）。
- `dsh --profile sdk`/`acp` 本身无 POSIX-only 依赖（stdin/stdout、node:child_process、junction）；`dsh plugin` 需要 pnpm（仅管理外置插件时，`python/sdk/README.md:45`）。

---

## 5. `sdk-minimal` 是否是更干净的嵌入式形态

**结构上干净，语义上更危险；适合当协议金丝雀/最小依赖面验证，不适合当 Halo 的主接入形态。**

干净的部分：

- 它是**唯一不叠 dsh-base 的 shipped profile**——cordis.patch.yml 的 insert 就是**完整 Cordis 树**，每一行服务显式在案（`DH\packages\bundle\sdk-minimal\cordis.patch.yml:1-3, 5-168`）：agent 内核（timer/llm/session/system-prompt/tools/agent/agent-loop `agents: []`/llm-retry/jobs/invariants×4）+ 执行面（sandbox-local + `danger-full-access` sandbox-policy + subprocess-local + pty + 平台门控 bash/pwsh 持久终端 + str-replace-editor）+ `session-persistence-jsonl`（root `$DSH_HOME/sessions`，`compression: none`）+ `llm-deepseek`（`apiKeyEnv: DEEPSEEK_API_KEY`、`DSH_CONTEXT_WINDOW` 默认 1M、48h stream idle）。
- 砍掉的：settings、managed credentials、telemetry（OTLP 上报整行不存在）、user-questions、storage、attachment-local、session-query、web 工具、完整默认工具花名册（官方口径：`DH\python\sdk\README.md:62`）。
- 无 credentials 服务时凭据解析**只能**走 launch environment（`llm-deepseek\src\index.ts:437-443`）——对「不落盘」反而是最简形态。
- system-prompt 无 harness identity/runtime context，可用 `DSH_SYSTEM_PROMPT` 覆盖（patch :91-96）；session-title 用纯启发式 fallback（:84-89），**不烧模型**。

危险的部分（对 Halo 的接入要求而言）：

- `sandbox-policy: danger-full-access` 硬编码 + `workspaceRoot: process.cwd()`（patch :41-45），且**不挂 `user-approval` 行**——approval answerer 缺失时按 fail-closed 契约落 `unavailable`（上游研究文档 §4），工具执行会被拒绝而不是被询问。也就是说：最小树里**高风险操作既没有审批通道、沙箱又全开**，与 Halo「一次性决议 + 最小权限」的产品要求正面冲突；要修就得靠 `--patch` 换掉 policy（但 approval 服务还得自己 insert，复杂度又回来了）。
- `maxTokensAsSuccess: false` 硬编码（patch :15），与 `sdk` profile 的 env 可控默认 true 不同。
- 协议本身与 `sdk` profile **完全相同**（同一个 `sdk-jsonrpc-server` 行，patch :11-15），所以"更干净"不省任何客户端实现——省的只是运行时依赖面与资源占用。

---

## 6. 对照表

| 维度 | `sdk` profile | `acp` profile | `sdk-minimal` profile |
|---|---|---|---|
| 启动命令 | `dsh --profile sdk [--patch yml]...` | `dsh --profile acp` | `dsh --profile sdk-minimal` |
| 入口包 | `@deepseek-ai/dsh`（bin）→ `dsh-base` + `@deepseek-ai/dsh-sdk-app` → `@deepseek-ai/dsh-sdk-jsonrpc-server` | 同 launcher → `dsh-base` + `@deepseek-ai/dsh-acp-app` → `@deepseek-ai/dsh-acp` | launcher → 仅 `@deepseek-ai/dsh-sdk-minimal`（完整显式树，无 dsh-base） |
| 传输 | stdio（无端口） | stdio（无端口） | stdio（无端口） |
| 消息框架 | newline-delimited JSON-RPC 2.0（私有协议，3 请求/4 通知） | ACP v1 标准 wire（`@agentclientprotocol/sdk`，ndjson） | 与 `sdk` 完全相同 |
| 握手/就绪 | `initialize`（等 Loader settle 才应答；返回 `deepseek-harness-sdk-runtime`）；默认 10s 界 | ACP `initialize`（protocolVersion + capabilities）+ `authenticate`（立即成功） | 同 `sdk` |
| 会话建立 | `session/prompt` 惰性 create，sessionId 客户端指定 | `session/new`（服务器发 UUID，cwd/mcp 校验，失败回滚） | 同 `sdk` |
| 会话恢复 | **无**（跨进程重放同 id 硬失败 `SessionAlreadyExistsError`） | `session/list` + `session/resume`（cwd realpath 校验，历史不重放） | 无 |
| 事件流 | `session.event`（全 runtime 广播，客户端过滤）+ `session.status` + `subagent.started/finished`；durable 事件粒度 | `session/update`（thought/message chunk、tool_call 生命周期、config_option、usage），每会话串行，committed 粒度 | 同 `sdk` |
| 取消 | **无 wire cancel**；客户端超时=放弃等待；取消=关子进程（EOF→SIGTERM→SIGKILL，Windows 直接强杀） | `session/cancel` + `$/cancel_request`；quiescence 后 `stopReason:'cancelled'` 结算；`session/close` 单会话静默回收 | 同 `sdk` |
| 权限决议 | 无 wire 面（approval 在 runtime 内，answerer 由组合决定；dsh-base 默认 `ask`） | `session/request_permission` 一次性 allow-once/reject 机器应答 | 无 approval 行 → fail-closed `unavailable`；sandbox 默认 `danger-full-access` |
| 模型/凭据 | `initialize` 传 provider/model/reasoningEffort/maxTokens；`apiKeyEnv` CredentialRef per-request 解析 | bundle config 定 provider/model；会话内 `session/set_config_option` 可改；MCP per-session 注入 | `initialize` 的 model 是唯一选择（无 advisory catalog 限制）；凭据仅 env |
| 配置注入 | `--patch`、两级 `cordis.patch.yml`、`DSH_PERMISSION_MODE`/`DSH_MAX_TOKENS_AS_SUCCESS` 等 env、`DSH_HOME` 隔离 | 同左 + per-session `mcpServers`/`set_config_option` | 同左（`DSH_CONTEXT_WINDOW`/`DSH_SYSTEM_PROMPT`） |
| 持久化 | dsh-base `session-persistence-jsonl` → `$DSH_HOME/sessions`（协议打不开） | 同左（且 resume 依赖它，`inject` 强制） | 自带 jsonl（compression none） |
| 遥测/设置 | dsh-base：OTel telemetry（`DSH_TELEMETRY_DISABLED` 可关）+ settings.yaml 热重载 | 同左 | **无** |
| 官方客户端 | TS `@deepseek-ai/dsh-sdk-client`（同版本强校验）+ Python `deepseek-harness-sdk`（同协议） | 上游自用 `dsh-subagent-acp`；标准协议有公开 spec/多语言实现 | 任一 `sdk` 客户端 |
| Windows | junction、pwsh 门控、win-x64 wheel、Windows dispose 直接强杀；沙箱 partial enforcement（见上游文档） | 同左 | 同左；bash/pwsh 行平台门控最显式 |

关键出处在 §1–§5 各论断行号；协议类型单一事实源：`DH\packages\sdk\protocol\src\types.ts`。

---

## 7. 对 halo-dsh-adapter 接入形态的建议排序

1. **首选：`dsh --profile sdk`，自写薄 JSON-RPC 客户端（或复用 `@deepseek-ai/dsh-sdk-client` 的协议层）**。理由：(a) 协议面只有 3+4 个方法，单行 JSON 框架可在 Rust 契约测试里用 fake 进程全覆盖（与 `pi_rpc_contract.rs` 的模式同构）；(b) `initialize` 是天然的能力/就绪探测点（返回前树已 settle），可承载 Halo 的「能力档案」检查；(c) stdin EOF 优雅关停 + 客户端 dispose 阶梯与 pi-rpc-adapter 的回收语义同构；(d) 凭据 env 注入 + `DSH_HOME` 隔离 + `--patch`/env 开关完整满足「不落盘、只传引用」；(e) Windows 一等支持。**必须接受的限制**：无 wire cancel——Halo 的取消语义要落成「放弃等待 + 关 stdin + 回收子进程」，会话生命周期绑定子进程生命周期（每个 Halo 会话一个受管 runtime，或接受「同一 runtime 内多会话、取消只能整进程」的二选一）。
2. **备选：`dsh --profile acp`**——仅当决策票认定 Halo 需要**跨进程会话恢复**（`session/list`/`session/resume`）、per-session MCP 挂载、或把「一次性决议」直接映射到 `session/request_permission`。成本：实现 ACP 客户端侧（标准协议但面更大），且事件流是语义投影（拿不到原始 SessionEvent 全量与 `interrupted`/`attempt` 语义）。
3. **`sdk-minimal`：只作协议金丝雀 / 升级 smoke / 极简降级通道**。用它验证新版本 wire 兼容性（协议与 `sdk` 相同、树显式可读）和最小依赖面；不作主接入——无审批行 + danger-full-access 与 Halo 一次性决议冲突。
4. **不推荐：进程内嵌 Cordis 树**。版本锚定成本（client↔runtime 同版本强校验体现了上游自己的立场）+ developer preview 漂移速度（上游研究文档 §5），进程外受管子进程的故障隔离对 Halo 更有利。

无论选哪条，契约测试都应覆盖：`initialize` 握手与就绪语义（loader settle）、`shutdown`→exit 0、stdin EOF 回收阶梯、`session.event` 全量广播的客户端过滤、取消=回收、`SessionAlreadyExistsError` 硬失败、env 凭据注入不落盘、`DSH_HOME` 隔离。

---

## 参考

- 协议：`DH\packages\sdk\protocol\src\{transport.ts,types.ts}`；服务器 `DH\packages\sdk\server\src\{index.ts,server.ts}`；客户端 `DH\packages\sdk\client\src\{launch.ts,client.ts,dispose.ts,api.ts,types.ts}`
- ACP：`DH\packages\acp\acp\src\{index.ts,session.ts,updates.ts,mcp.ts,codec.ts,content.ts}`；`DH\packages\acp\acp\README.md`
- boot/launcher：`DH\apps\cli\src\{bin.ts,args.ts,profile-boot.ts}`；`DH\packages\boot\app-boot\src\{index.ts,profile.ts}`；`DH\packages\boot\cmdline\src\index.ts`
- bundle：`DH\packages\bundle\{base,sdk-app,acp-app,sdk-minimal}\cordis.patch.yml` 及各自 `src/index.ts`
- 凭据/环境：`DH\docs\subsystems\credentials.md`；`DH\packages\credentials\README.md`；`DH\packages\llm\llm-deepseek\src\{index.ts,adapter.ts}`；`DH\packages\util\{home-paths,launch-environment}\src\index.ts`
- 会话恢复原语：`DH\packages\core\agent\src\index.ts`；`DH\packages\core\agent-loop\src\index.ts`；`DH\packages\session\session-persistence-jsonl\src\index.ts`
- Python SDK：`DH\python\README.md`、`DH\python\sdk\README.md`、`DH\python\sdk-runtime\platforms.json`
- Halo 侧对照：`docs/development/pi-rpc-adapter.md`；`docs/architecture/dsh-upstream-state-research-20260905.md`（架构级事实）
- 上游：https://github.com/deepseek-ai/deepseek-harness · Agent Client Protocol https://agentclientprotocol.com
