# R3 — OpenCode 真实服务协议分析与适配器差距

**依据：** `requirements-alignment/03-ide-editor-and-reference-alignment.md`
**参考源码：** `D:\用于参考的开源项目的代码\opencode-dev`（只读；`packages/opencode/package.json` 版本 `1.18.4`）
**对照对象：** `docs/module-contracts.md` 第 5 节的 OpenCode 假设适配器协议（`serve --hostname/--port`、`HALO_OC_TOKEN` Bearer、`/health /version /task /events /cancel /shutdown`）
**分析方法：** 以 `packages/sdk/openapi.json`（OpenAPI 3.1.0，机器可读权威契约）为骨架，逐项回查 `packages/opencode/src/server`、`src/session`、`src/permission` 的实现源码验证；引用一律给出仓库内相对路径。

> 结论先行：我们的假设协议在「本地回环 HTTP 服务 + 事件流 + 取消」这个大方向上是对的，但**六个端点里五个不存在**，认证机制（Bearer token）与事件消费方式（长轮询）与真实实现（HTTP Basic + SSE）完全不同，且真实服务没有任务概念——只有**会话（session）+ 消息（message）**。适配器 v2 必须按「session 语义」重写。

---

## 1. 服务形态

### 1.1 启动命令与参数

服务由 `opencode serve` 启动（`packages/opencode/src/cli/cmd/serve.ts`），网络参数定义在 `packages/opencode/src/cli/network.ts`：

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `--hostname` | `127.0.0.1` | 监听地址 |
| `--port` | `0` | `0` 表示自动选端口：**优先尝试 4096，被占用则随机空闲端口**（`src/server/server.ts` `startWithPortFallback`） |
| `--mdns` / `--mdns-domain` | `false` / `opencode.local` | mDNS 服务发现（回环地址下自动跳过发布） |
| `--cors` | `[]` | 额外允许的 CORS 域 |

CLI 参数可被全局配置 `server.hostname/port/mdns/cors` 覆盖（显式命令行参数优先）。官方 TS SDK 的启动方式即 `opencode serve --hostname=<h> --port=<p>`（`packages/sdk/js/src/server.ts` `createOpencodeServer`），并可用环境变量 `OPENCODE_CONFIG_CONTENT`（JSON 字符串）注入整份配置而不落盘。

### 1.2 就绪确认

serve 启动成功后向 **stdout** 打印一行：

```
opencode server listening on http://<hostname>:<port>
```

官方 SDK 的就绪检测就是解析这一行（正则 `on\s+(https?:\/\/[^\s]+)`，默认 5s 超时；`packages/sdk/js/src/server.ts`）。这一行同时是**实际端口的权威来源**（`--port 0` 时端口由服务决定）。

### 1.3 认证机制：HTTP Basic（不是 Bearer token）

源码：`packages/opencode/src/server/auth.ts`、`src/server/routes/instance/httpapi/middleware/authorization.ts`。

- 凭据来自环境变量：`OPENCODE_SERVER_PASSWORD`（密码）与 `OPENCODE_SERVER_USERNAME`（用户名，缺省 `opencode`）。
- **不设置密码则完全无鉴权**，serve 启动时仅打印警告 `Warning: OPENCODE_SERVER_PASSWORD is not set; server is unsecured.`（`src/cli/cmd/serve.ts`）。
- 校验方式：`Authorization: Basic base64("<user>:<password>")`；另支持 URL 查询参数 `?auth_token=<base64(user:pass)>`（供 EventSource/浏览器场景）。
- 失败返回 `401` + `WWW-Authenticate: Basic realm="Secure Area"`，空 body。
- 服务端**不生成、不下发任何 token**；密钥完全由启动方经环境变量注入——这与我们的凭据注入模型天然契合。

### 1.4 OpenAPI 与 SDK

- `packages/sdk/openapi.json`：完整 OpenAPI 3.1.0 文档（约 1MB），全部端点、请求/响应 schema、事件联合类型都在其中，是修订 halo-runtime 类型时的首选比对材料；运行期也可由 `Server.openapi()` 生成（`src/server/server.ts`）。
- `packages/sdk/js`：官方 TS SDK（生成的客户端 + `createOpencodeServer` 进程管理）。
- `sdks/vscode`：VS Code 扩展用 SDK。
- `specs/`：v2 架构设计文稿（`specs/v2/session.md` 等），描述演进方向，**不是**当前 HTTP 契约的权威。

### 1.5 多目录实例路由（我们首期不用，但必须理解）

一个 server 进程可同时服务多个项目目录：每个请求经 `?directory=<路径>` 查询参数或 `x-opencode-directory` 请求头选择实例，缺省 `process.cwd()`（`src/server/routes/instance/httpapi/middleware/workspace-routing.ts` `defaultDirectory`；SDK 侧 `packages/sdk/js/src/client.ts` 会把 header 改写为 GET 查询参数）。**适配器 v2 应始终显式携带 directory=受信任工作区真实路径**，不依赖服务端 cwd 缺省。

---

## 2. 会话、消息与事件流

### 2.1 会话模型

真实服务没有 `/task`。工作单元是会话（session，id 前缀 `ses_`），端点定义于 `src/server/routes/instance/httpapi/groups/session.ts`（`SessionPaths` 常量表）：

| 端点 | 方法 | 说明 |
| --- | --- | --- |
| `/session` | POST | 创建会话。body 可选：`{parentID?, title?, agent?, model?, metadata?, permission?(规则集), workspaceID?}`（`src/session/session.ts` `CreateInput`）→ 返回 `Session.Info` |
| `/session` | GET | 列表（支持 `search/limit/start` 等） |
| `/session/{id}` | GET / PATCH / DELETE | 查询 / 更新（title、metadata、permission、归档）/ 删除 |
| `/session/status` | GET | **全部会话状态快照** `{<sessionID>: {type: "idle"\|"busy"\|"retry"}}` |
| `/session/{id}/message` | GET | 消息列表（`limit`、`before` 分页） |
| `/session/{id}/diff` | GET | 会话产生的文件变更 `SnapshotFileDiff[]`：`{file, patch, additions, deletions, status: added\|deleted\|modified}` |
| `/session/{id}/fork`、`/revert`、`/unrevert`、`/share`、`/summarize`、`/children`、`/todo` | — | 高级能力（首期不接） |

### 2.2 发送 prompt

两种形态（`src/server/routes/instance/httpapi/handlers/session.ts`）：

- `POST /session/{id}/message`（同步）：**阻塞到整轮结束**，返回 `{info: AssistantMessage, parts: Part[]}`。
- `POST /session/{id}/prompt_async`（异步）：立即 `204 No Content`；执行失败以 `session.error` 事件补发。**适合我们**。

请求体 `PromptInput`（`src/session/prompt.ts` 1499 行起）：

```jsonc
{
  "messageID": "msg…",          // 可选，幂等键
  "model": { "providerID": "…", "modelID": "…" },   // 可选
  "agent": "…",                 // 可选，agent 名
  "system": "…",                // 可选，附加 system prompt
  "parts": [                     // 必填，判别字段 type
    { "type": "text", "text": "任务说明…" },                      // TextPartInput
    { "type": "file", "mime": "text/plain", "url": "file://…",
      "filename": "src/a.rs" },                                   // FilePartInput
    { "type": "agent", "name": "…" },                             // AgentPartInput
    { "type": "subtask", "prompt": "…", "description": "…", "agent": "…" } // SubtaskPartInput
  ]
}
```

### 2.3 消息模型（`packages/schema/src/v1/session.ts`）

- `UserMessage`：`{id, sessionID, role:"user", time:{created}, agent, model{providerID,modelID}}`。
- `AssistantMessage`：`{id, sessionID, role:"assistant", parentID, time:{created, completed?}, error?, cost, tokens{input,output,reasoning,cache}, modelID, providerID, path{cwd,root}, finish?}`。**`time.completed` 有值即该条回复结束；`error` 有值即异常结束**。
- `Part` 联合（判别字段 `type`）：`text` / `reasoning` / `file` / `tool` / `step-start` / `step-finish` / `snapshot` / `patch` / `agent` / `retry` / `compaction` / `subtask`。
- 工具调用 = `ToolPart`：`{type:"tool", callID, tool, state: ToolState}`，`ToolState` 为 `pending → running → completed | error` 状态机（带 `time`、`input`、`output`、`error`）。

### 2.4 事件流：SSE（不是长轮询，也没有 after 序号）

两条 SSE 通道（`text/event-stream`，每条 `data:` 为一个 JSON）：

- `GET /event?directory=<路径>`：**单实例事件流**（`groups/event.ts` + `handlers/event.ts`）。事件形状 `{id, type, properties}`；连接建立先发 `server.connected`；每 10s 一条 `server.heartbeat`；实例销毁时发 `server.instance.disposed` 并结束流。
- `GET /global/event`：全局事件流，事件包一层 `{directory, project?, workspace?, payload:{id,type,properties}}`（`handlers/global.ts`）。官方 headless CLI（`src/cli/cmd/run/stream.transport.ts`）用的就是全局流。

**没有事件序号、没有断线重放**：SSE 断开即丢失区间事件，官方客户端的补偿策略是重连后用 `GET /session/status`、`GET /permission`、`GET /session/{id}/message` 等快照端点重建状态（`stream.transport.ts` 顶部注释明确说明「事件可能丢、要轮询 status 兜底」）。

**事件类型清单**（`packages/sdk/openapi.json` 的 `Event` 联合 + `packages/schema/src/event-manifest.ts`，节选与我们相关者）：

| 事件 `type` | payload 要点 | 适配意义 |
| --- | --- | --- |
| `server.connected` / `server.heartbeat` | `{}` | 连接确认 / 保活 |
| `session.created` / `session.updated` / `session.deleted` | `{sessionID, info: Session}` | 会话生命周期 |
| `session.status` | `{sessionID, status:{type:"idle"\|"busy"\|"retry",…}}` | **运行中/结束的权威信号**（`schema/src/session-status-event.ts`） |
| `session.idle` | `{sessionID}` | 旧版结束信号，源码标注 deprecated，与 `session.status(idle)` 同时发 |
| `session.error` | `{sessionID?, error:{name,…}}`，name ∈ ProviderAuthError / UnknownError / MessageOutputLengthError / MessageAbortedError / StructuredOutputError / ContextOverflowError / ContentFilterError / APIError | 失败结论 |
| `message.updated` / `message.removed` | `{sessionID, info: Message}` | 消息级更新（含 assistant `time.completed`/`error`） |
| `message.part.updated` / `message.part.removed` | `{sessionID, part: Part}` | 工具调用状态、文本/推理块落定 |
| `message.part.delta` | `{sessionID, messageID, partID, field, delta}` | 流式增量文本 |
| `session.diff` | `{sessionID, diff: SnapshotFileDiff[]}` | 文件变更快照 |
| `file.edited` | `{file}` | 单文件被改 |
| `todo.updated` | `{sessionID, todos}` | Agent 计划列表 |
| `permission.asked` / `permission.replied` | 见第 5 节 | **权限请求** |
| `question.asked` / `question.replied` / `question.rejected` | 见第 5 节 | 澄清提问 |
| `server.instance.disposed` | `{directory}` | 实例销毁，流终止 |

其余（`installation.*`、`pty.*`、`mcp.*`、`vcs.*`、`worktree.*`、`tui.*`、`permission.v2.*` 实验版等）首期与我们无关，适配器按未知类型忽略即可（**必须容忍未知事件类型**，清单随版本增长）。

---

## 3. 任务（会话轮次）生命周期

以官方 headless 客户端 `src/cli/cmd/run/stream.transport.ts` 的做法为准绳：

1. **启动轮次**：`POST /session/{id}/prompt_async`（204 即受理）。
2. **运行中**：`session.status(busy)`；过程细节靠 `message.part.updated`（ToolPart running/completed）、`message.part.delta`（文本流）、`file.edited`、`todo.updated`。
3. **结束判定**：收到 `session.status(idle)`。官方实现有两层防抖，值得照抄：
   - 收到 idle 事件后**再调 `GET /session/status` 复核**，防止旧轮次的迟到 idle 错误终结新轮次；
   - 事件流可能漏事件，**辅以周期轮询 `GET /session/status`** 兜底。
4. **成败结论**：轮次结束后取最后一条 assistant message（`GET /session/{id}/message?limit=…`）——`error` 为空且 `time.completed` 有值 = 成功；`error` 有值 = 失败（`MessageAbortedError` = 被中止）。`session.error` 事件是失败的实时信号。
5. **结果/变更**：`GET /session/{id}/diff` 拿 `SnapshotFileDiff[]`（自带 per-file patch 与增删行数，可直接喂给我们的证据模型做交叉验证；但**任务关联变更的权威仍是我们自己的 Git 基线算法**）。
6. **取消**：`POST /session/{id}/abort` → 响应 `true`（`handlers/session.ts` `abort` → `SessionPrompt.cancel`）；随后流上出现 assistant `error: MessageAbortedError` 与 `session.status(idle)`。
7. **服务停止**：**没有任何 HTTP shutdown 端点**。`POST /global/dispose` 只销毁实例并广播 disposed 事件，**进程仍监听**；`POST /instance/dispose` 同理只针对单实例。官方 SDK 的停止方式就是杀进程：Windows 上 `taskkill /pid <pid> /T /F`，其余平台 `proc.kill()`（`packages/sdk/js/src/process.ts`）。HTTP 层自带 1s graceful-shutdown（`src/server/server.ts` `serverLayer`）。

---

## 4. 版本与健康

- `GET /global/health` → `200 {"healthy": true, "version": "<InstallationVersion>"}`（`groups/global.ts` + `handlers/global.ts`）。**健康与版本是同一个端点**，没有独立 `/version`，也没有 `/health`。
- CLI 侧 `opencode --version` 仍可用作启动前探测（与 Pi 探测同构）。
- 参考源码版本为 `1.18.4`，我们契约里锁定的 `0.4.2` 是占位假设值，必须按实际装机的二进制重新锁定。

---

## 5. 权限模型（对 `task.action_request` 映射至关重要）

### 5.1 permission：工具权限审批

Schema：`packages/schema/src/v1/permission.ts`；运行时语义：`packages/opencode/src/permission/index.ts`；端点：`groups/permission.ts`。

**请求（`permission.asked` 事件，同 `GET /permission` 列表项）：**

```jsonc
{
  "id": "per_…",
  "sessionID": "ses_…",
  "permission": "edit" ,            // 权限种类字符串（如 edit / bash / webfetch…）
  "patterns": ["src/**"],           // 本次触发的具体模式（命令行、文件路径等）
  "metadata": { … },                 // 工具相关展示信息（自由形状）
  "always": ["src/**"],             // 若用户选 always，将被固化为 allow 规则的模式
  "tool": { "messageID": "msg_…", "callID": "…" }   // 可选，关联的工具调用
}
```

**回复：`POST /permission/{requestID}/reply`，body：**

```jsonc
{ "reply": "once" | "always" | "reject", "message": "可选，仅 reject 时作为给 Agent 的纠正反馈" }
```

服务端语义（`permission/index.ts` `reply`）：

- `once`：放行这一次；
- `always`：放行并把 `always` 里的模式追加为会话内 `allow` 规则，**同会话所有能被新规则覆盖的挂起请求自动放行**；
- `reject`：该工具调用以 `PermissionRejectedError` 失败（带 `message` 则为 `CorrectedError`，反馈文本会传回给 Agent），且**同会话其余全部挂起权限请求被级联拒绝**；
- 请求不存在 → `PermissionNotFoundError`。
- 决议结果以 `permission.replied` 事件广播 `{sessionID, requestID, reply}`。
- 规则来源：配置 `permission` 规则集（`allow/deny/ask` × 通配模式），`deny` 命中直接失败不发请求，`ask` 命中才产生 `permission.asked`。
- 旧端点 `POST /session/{id}/permissions/{permissionID}`（body `{response}`）已标 deprecated，不要用。

### 5.2 question：澄清提问（`task.action_request` 的 clarification 分支）

端点：`groups/question.ts`；事件 `question.asked`：

```jsonc
{
  "id": "que_…", "sessionID": "ses_…",
  "questions": [ { "question": "完整问题", "header": "≤30字符短标签",
                    "options": [ {label…} ], "multiple": false, "custom": false } ],
  "tool": { … }    // 可选
}
```

回复 `POST /question/{requestID}/reply`，body `{"answers": [["选中的label"], …]}`（answers 与 questions 顺序对应，每项是选中 label 数组）；或 `POST /question/{requestID}/reject` 拒答。挂起列表 `GET /question`。

---

## 6. 真实协议 vs 假设协议差异表

假设协议 = `docs/module-contracts.md` 第 5 节 OpenCode 条目。

| # | 项 | 假设协议 | 真实协议（opencode-dev） | 差异等级 |
| --- | --- | --- | --- | --- |
| 1 | 启动命令 | `<exe> serve --hostname 127.0.0.1 --port <p>` | 相同（另有 `--mdns/--cors`；`--port 0` 时自动 4096→随机） | 兼容 |
| 2 | 就绪确认 | 轮询 `GET /health` | stdout 行 `opencode server listening on http://…`（端口权威来源）＋可再查 `/global/health` | **改写** |
| 3 | 认证 | 每次启动生成 32 字节 hex token，经 `HALO_OC_TOKEN` env 注入，请求带 `Authorization: Bearer <token>` | 密码经 `OPENCODE_SERVER_PASSWORD` env 注入（用户名 `OPENCODE_SERVER_USERNAME`，默认 `opencode`），请求带 `Authorization: Basic base64(user:pass)`；不设密码=无鉴权 | **改写**（随机值生成逻辑可复用，仅换 env 名与 header 形式） |
| 4 | 健康检查 | `GET /health` → `{"status":"ok"}` | `GET /global/health` → `{"healthy":true,"version":…}` | **改写** |
| 5 | 版本查询 | `GET /version` → `{"version":"0.4.2"}`，与锁定值全等 | 无独立端点；版本在 `/global/health` 返回；真实版本 1.18.4 | **改写**（全等比较纪律保留，锁定值按装机二进制重定） |
| 6 | 任务提交 | `POST /task` | 无 /task。`POST /session` 建会话 → `POST /session/{id}/prompt_async`（204）；任务说明/文件映射为 `parts`（text/file part） | **重写** |
| 7 | 事件流 | 长轮询 `GET /events?after=<n>` → `{"events":[…],"done":bool,"outcome":…}` | SSE `GET /event?directory=…`（`data:` 行 JSON `{id,type,properties}`；heartbeat 10s；无序号无重放，断线后靠快照端点重建） | **重写** |
| 8 | 取消 | `POST /cancel` | `POST /session/{id}/abort` → `true`，随后 `MessageAbortedError` + `session.status(idle)` | **改写** |
| 9 | 优雅停止 | `POST /shutdown`，超时 kill | **无 shutdown 端点**；`/global/dispose` 仅清实例不退进程；官方停止方式=杀进程（Windows `taskkill /T /F`） | **重写**（“原生优雅停止”降级为 abort+dispose+杀进程） |
| 10 | 权限/操作请求 | 事件 `action_request`（自造形状） | `permission.asked`（id/permission/patterns/metadata/always/tool）+ `POST /permission/{id}/reply` `{reply: once\|always\|reject, message?}`；澄清另走 `question.asked`/reply/reject | **重写** |
| 11 | 结果表达 | 长轮询响应内 `outcome` | `session.status(idle)` + 末条 assistant message 的 `time.completed`/`error` + `GET /session/{id}/diff` | **重写** |
| 12 | 实例模型 | 每任务一进程、一端口 | 一进程多目录实例（`?directory=` / `x-opencode-directory`），需显式携带工作区路径 | 新增认知（我们仍按每工作区一进程使用） |

---

## 7. 适配器 v2 建议（halo-runtime OpenCodeRuntime / OpenCodeHandle）

### 7.1 启动与认证

- 启动：`<exe> serve --hostname 127.0.0.1 --port <Sidecar 选定空闲端口>`；仍由 Sidecar 选端口（不用 `--port 0`，避免解析 stdout 端口的额外路径），但**就绪判定改为双重**：先读 stdout 就绪行（校验端口一致），再 `GET /global/health` 200 且 `healthy=true`（总超时保持默认 10s 可注入）。
- 认证：保留“每次启动生成 32 字节随机 hex”的现有逻辑，注入方式改为 `OPENCODE_SERVER_PASSWORD=<hex>`（该 env 名加入子进程环境的注入清单，属凭据类，不进日志）；所有 HTTP 请求带 `Authorization: Basic base64("opencode:<hex>")`。密码与端口继续封在 `OpenCodeHandle` 内部，不出现在任何公开 getter/Debug——与现契约一致。
- 每个请求显式带 `?directory=<受信任工作区真实路径>`，不依赖服务端 cwd。
- 版本锁定：`OPENCODE_LOCKED_VERSION` 改为按实际装机二进制重新锁定；比较源改为 `/global/health` 的 `version` 字段，**全等纪律不变**，不匹配 → `Failed{RUNTIME_VERSION_MISMATCH}`。

### 7.2 任务映射

- `run_task(spec)` ⇒
  1. `POST /session`（body `{title: 任务标题}`；可选注入 `permission` 规则集收紧写权限——留给 14 号设计定夺）记下 `sessionID`；
  2. `POST /session/{sessionID}/prompt_async`，parts = `[{type:"text", text: instructions(+notes)}] + files.map(f => {type:"file", mime, url:"file://…", filename})`；`base_diff` 并入 text part（OpenCode 无独立 diff 输入通道）。
- 一个 Agent 任务 = 一个 session 的一轮 prompt；**多会话并发、fork/revert/share/summarize/todo、subtask part、question 多选/自定义答案、pty/mcp/worktree/多目录路由、`permission reply=always`、`/global/upgrade`、mdns、sync 首期一律不接**。

### 7.3 事件流消费（专用线程，无 async）

- 启动后立即建立 `GET /event?directory=…` 的 SSE 长连接：阻塞读、按行解析，空行分帧，取 `data:` 载荷 JSON；忽略 `server.heartbeat` 与一切未知 `type`（前向兼容红线）。
- 事件 → `RuntimeEvent` 映射：

| OpenCode 事件 | RuntimeEvent |
| --- | --- |
| `session.status(busy)` | `Trace{kind:"phase", text:"running"}` |
| `message.part.updated`（ToolPart pending/running/completed/error） | `Trace{kind:"tool", text:<tool+state>, detail:<限长后的 input/output>}` |
| `message.part.updated`（text/reasoning 落定） | `Trace{kind:"agent_note", text:<cap 限长>}`（**不消费 `message.part.delta`**，首期无需逐字流式） |
| `file.edited` / `session.diff` | `Trace{kind:"file_hint", …}` |
| `todo.updated` | `Trace{kind:"phase", …}`（可选） |
| `permission.asked` | `ActionRequest{request_id: per id, kind:"permission", prompt: permission+patterns+metadata 摘要}` |
| `question.asked` | `ActionRequest{kind:"clarification", prompt: questions 摘要}` |
| `session.error` | 失败结论输入（缓存 error） |
| `session.status(idle)` | 触发结束判定（见下） |

- 结束判定采用官方 `stream.transport.ts` 同款防抖：收到 idle 后**复核 `GET /session/status`**；再拉 `GET /session/{id}/message?limit=1` 取末条 assistant message——`error` 空 ⇒ `TaskDone{outcome:"finished", summary:<末条文本摘要>}`；`error` 非空 ⇒ `failed`（`MessageAbortedError` 对应取消路径）。SSE 意外断连：重连一次并以快照端点（`/session/status`、`/permission`）重建；重连失败 ⇒ `Failed{reason}`（与现有 EOF/坏帧纪律同构）。
- 权限决议回传：新增 `resolve_action(request_id, decision)` 通道——permission ⇒ `POST /permission/{id}/reply`（首期只暴露 `once`/`reject` 两种，`reject` 可带 message；**不暴露 `always`**，避免 Halo 替用户固化放行规则）；clarification ⇒ `POST /question/{id}/reply` 或 `/reject`。这是对「Agent 操作请求经原生通道决定」的落实：serve 模式下 HTTP reply 端点**就是** OpenCode 的原生通道（其 TUI/CLI 也走同一 API）。需在 14 号设计文档中把 IPC 的 `task.action_request` 增补 `resolve` 方法后再实施。

### 7.4 取消与停止

- `cancel_native()` ⇒ `POST /session/{id}/abort`；随后等待 `session.status(idle)`/`MessageAbortedError`；`cancel_grace`（默认 10s）内未见即走强杀——现有「原生优先、超时强制」语义完整保留。
- `stop(grace)` ⇒ 顺序执行：任务在跑先 abort → `POST /global/dispose`（释放实例、结束事件流）→ 关闭 SSE 连接 → 终止子进程（Windows：`taskkill /pid <pid> /T /F`，参照 `packages/sdk/js/src/process.ts`；这就是官方语义下的“优雅停止”）→ `shutdown_grace` 超时后强杀。`StopOutcome::Graceful/Forced` 语义不变。

### 7.5 halo-testkit fake-opencode 改造要点

- 假服务改为：校验 **Basic** auth（`FAKE_OC_MODE=bad_token` 改为返回 401 的 Basic 校验失败）；实现 `GET /global/health`（版本可由 `FAKE_OC_VERSION` 覆盖）、`POST /session`、`POST /session/{id}/prompt_async`、SSE `GET /event`（脚本化吐 `session.status`、`message.part.updated`、`permission.asked`、`file.edited`、idle 收尾）、`GET /session/status`、`GET /session/{id}/message`、`POST /session/{id}/abort`、`POST /permission/{id}/reply`、`POST /global/dispose`；仍只绑 127.0.0.1。
- 新模式建议：`sse_drop`（中途断流，测重连与快照重建）、`stale_idle`（先发迟到 idle，测防抖复核）。

### 7.6 风险标注

- 参考源码为 dev 主干（1.18.4），端点带有 experimental 注记且事件清单持续增长；适配器必须**按锁定二进制版本验收**，且对未知事件/未知字段一律忽略而非报错。
- `session.idle` 事件已标 deprecated——结束判定以 `session.status` 为准，`session.idle` 只作冗余信号。
- 无鉴权模式（不设密码）绝不允许出现在 Halo 启动路径中：适配器必须无条件注入 `OPENCODE_SERVER_PASSWORD`。

---

## 修订记录

- 2026-07-27 初版（依据 opencode-dev @ package 版本 1.18.4 源码与 packages/sdk/openapi.json）。
