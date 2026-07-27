# IPC 契约：应用控制层 ↔ Rust Sidecar（JSONL v1）

本文件是 UI（PySide6/QML 应用控制层）与 Rust Sidecar 之间**版本化 stdio JSONL 契约**的唯一权威定义。
Rust 侧类型在 `halo-protocol` crate 中实现；Python 侧在 `app/halo_studio/ipc/` 中实现；两侧均须与本文件一致。
JSON Schema 副本见 `protocol/v1/`。

## 1. 传输与封包

- Sidecar 是 UI 的子进程；UI 写 Sidecar 的 stdin，读 stdout。stderr 仅用于诊断文本，不承载协议。
- 一行一个 UTF-8 JSON 对象（JSONL）。行分隔符 `\n`。单行上限 1 MiB，超限即协议错误。
- 三种封包，由 `kind` 区分：

```jsonc
// 请求（UI → Sidecar）
{"v": 1, "kind": "request", "id": "r-<uuid4>", "method": "task.create", "params": { }}

// 响应（Sidecar → UI，与请求同 id；ok=false 时带 error）
{"v": 1, "kind": "response", "id": "r-<uuid4>", "ok": true,  "result": { }}
{"v": 1, "kind": "response", "id": "r-<uuid4>", "ok": false, "error": {"code": "TASK_ALREADY_RUNNING", "message": "…", "details": {}}}

// 事件（Sidecar → UI，全局单调递增 seq，由唯一写线程分配）
{"v": 1, "kind": "event", "seq": 42, "ts": "2026-07-26T08:00:00Z", "task_id": "task-<uuid4>|null", "event": "task.phase", "payload": { }}
```

- `v` 是协议主版本。当前唯一版本为 `1`。
- 时间戳一律 UTC ISO-8601（`YYYY-MM-DDThh:mm:ssZ`）。
- 事件 `seq` 全局单调递增（不分任务），Sidecar 维护至少最近 **1024** 条事件的环形缓冲以支持界面恢复。

## 2. 握手与版本协商

连接后 UI 必须先调用 `sidecar.hello`；此前任何其他方法一律返回 `HELLO_REQUIRED`。

| 方法 | params | result |
| --- | --- | --- |
| `sidecar.hello` | `{"app_protocol_versions": [1], "app_version": "0.1.0"}` | `{"protocol_version": 1, "sidecar_version": "0.1.0", "capabilities": ["workspace","config","pi","opencode","task","review","handoff","history"]}` |

无公共版本时返回错误 `PROTOCOL_VERSION_UNSUPPORTED`（details 里带 `sidecar_protocol_versions`）。UI 必须把协议版本与不可用原因展示给用户。

## 3. 方法目录

### 3.1 工作区（workspace.*）

| 方法 | params | result | 说明 |
| --- | --- | --- | --- |
| `workspace.open` | `{"path": "D:\\repo"}` | `WorkspaceStatus` | 真实路径校验（存在、可读、canonicalize、`git rev-parse` 确认 Git 仓库）。若已有活动工作区：无运行中任务时自动停止旧运行时并切换；有运行中任务时返回 `TASK_RUNNING`。 |
| `workspace.trust` | `{"workspace_id": "ws-…", "decision": "trust"\|"revoke"}` | `WorkspaceStatus` | revoke 立即停止并清理该工作区全部受管运行时。 |
| `workspace.close` | `{}` | `{"closed": true}` | 停止运行时并清空活动工作区；有运行中任务时返回 `TASK_RUNNING`。 |
| `workspace.status` | `{}` | `WorkspaceStatus \| {"active": false}` | |

`WorkspaceStatus`：

```jsonc
{
  "active": true,
  "workspace_id": "ws-<uuid4>",
  "real_path": "D:\\repo",            // canonicalize 后的真实路径
  "git_root": "D:\\repo",
  "root_commit": "abc123…|null",       // 仓库首个提交，用于目录替换检测
  "trust": "untrusted" | "trusted",
  "identity_changed": false             // true 时信任已被降级，需要重新确认
}
```

- 未信任（untrusted）状态下：`config.*` 读操作允许，但 `runtime.start`、`task.create` 一律返回 `WORKSPACE_NOT_TRUSTED`；不加载项目内任何 Agent 配置或插件。
- 信任决定持久化键 =（real_path, root_commit）。任一不匹配（目录被替换/重建）→ `identity_changed: true` 且降级为 untrusted。
- 路径不存在/不可读/非 Git 仓库分别返回 `WORKSPACE_PATH_INVALID` / `WORKSPACE_NOT_READABLE` / `WORKSPACE_NOT_GIT`，message 为用户可读中文。空格与 CJK 路径必须正常工作。

### 3.2 受管启动配置（config.*）

凭据明文**永不**出现在本节任何 params/result 中。UI 只处理**凭据引用名**（Windows 凭据管理器条目名）。录入走 Sidecar CLI：`halo-sidecar cred set <ref>`（从 stdin 读密钥）。

| 方法 | params | result |
| --- | --- | --- |
| `config.list` | `{}` | `{"configs": [LaunchConfig]}` |
| `config.save` | `LaunchConfigInput` | `{"config": LaunchConfig}` |
| `config.delete` | `{"config_id": "cfg-…"}` | `{"deleted": true}` |
| `config.credential_check` | `{"credential_ref": "halo/pi/openai"}` | `{"exists": true, "store_available": true}` |

```jsonc
// LaunchConfigInput（LaunchConfig 额外含 config_id/created_at/updated_at）
{
  "name": "Pi + GPT",
  "agent": "pi" | "opencode",
  "executable_path": "C:\\tools\\pi\\pi.exe",
  "model": "gpt-5",
  "thinking_level": "off" | "low" | "medium" | "high",
  "credential_ref": "halo/pi/openai",     // 或 null
  "credential_env_var": "ANTHROPIC_API_KEY",// 或 null=按 provider 默认表推导（14 号设计文档 4.7）；
                                            // 显式值须匹配 ^[A-Z][A-Z0-9_]{2,63}$、不得与环境白名单冲突、
                                            // 不得为保留名（OPENCODE_SERVER_PASSWORD/OPENCODE_SERVER_USERNAME/
                                            // PI_CODING_AGENT_DIR/PI_SKIP_VERSION_CHECK），违规 INVALID_PARAMS
  "extra_args": [],
  "env_overrides": {}                       // 仅白名单内变量名可出现，违规返回 ENV_NOT_WHITELISTED
}
```

- 操作系统凭据存储不可用时：`config.save`（含 credential_ref 的）与 `runtime.start` 一律**失败关闭**，返回 `CREDENTIAL_STORE_UNAVAILABLE`，绝不回退到明文文件。
- 子进程环境 = 显式白名单（`SYSTEMROOT`、`WINDIR`、`PATH`、`TEMP`、`TMP`、`USERPROFILE`、`COMSPEC`、`PATHEXT`、`SystemDrive`、`NUMBER_OF_PROCESSORS`、`PROCESSOR_ARCHITECTURE`）+ Sidecar 在启动瞬间注入的凭据变量。宿主其余环境变量一律不继承。

### 3.3 受管运行时（runtime.*）

| 方法 | params | result |
| --- | --- | --- |
| `runtime.probe` | `{"agent": "pi"\|"opencode", "config_id": "cfg-…"}` | `{"agent": "pi", "version": "1.4.0", "supported": true}` |
| `runtime.start` | `{"agent": …, "config_id": …}` | `{"state": RuntimeState}` |
| `runtime.stop` | `{"agent": …}` | `{"state": RuntimeState}` |
| `runtime.status` | `{}` | `{"pi": RuntimeStateInfo, "opencode": RuntimeStateInfo}` |

```jsonc
// RuntimeStateInfo — 每个受管应用独立健康状态，绝不合并为“全局在线”
{
  "state": "not_probed"|"probing"|"starting"|"ready"|"failed"|"stopping"|"stopped",
  "reason": "…|null",              // failed 时用户可读原因
  "recovery_hint": "…|null",       // failed 时恢复建议
  "version": "1.4.0|null"
}
```

- Pi：版本探测 + RPC `get_state` 就绪检查通过后才允许 `ready`。
- OpenCode：仅回环地址 + 每次启动新认证信息 + 健康检查 + 精确版本握手通过后才 `ready`。**端口与认证信息不出现在任何 result/event 中**。
- 状态变化推送事件 `runtime.state`（payload = `{"agent": …} ∪ RuntimeStateInfo`）。

### 3.4 任务（task.*）

| 方法 | params | result |
| --- | --- | --- |
| `task.create` | `TaskSpec` | `{"task": TaskStatus}` |
| `task.cancel` | `{"task_id": …}` | `{"accepted": true}`（结果经事件） |
| `task.mark_manual_edit` | `{"task_id": …, "note": "…"}` | `{"attribution": "mixed"}` |
| `task.mark_verification` | `{"task_id": …, "status": "not_run", "note": "…"}` | `{"ok": true}`（用户显式标记未执行） |
| `task.status` | `{"task_id": …}` 或 `{}`（当前任务） | `{"task": TaskStatus \| null}` |
| `task.snapshot` | `{"after_seq": 40}` | `{"task": TaskStatus\|null, "last_seq": 42, "events": [Event...]}`；缓冲不足覆盖 after_seq 时返回错误 `EVENT_GAP`，UI 应整体重建视图 |

```jsonc
// TaskSpec — 任务只携带用户显式提供的内容，绝不自动附带完整工作区或历史
{
  "agent": "pi" | "opencode",
  "config_id": "cfg-…",
  "title": "修复登录超时",
  "instructions": "…",                  // 必填任务目标
  "files": ["src/auth.rs"],             // 用户主动选取，可空
  "base_diff": "…|null",                // 用户提供的已有 Diff，可空
  "notes": "…|null",                    // 补充说明
  "handoff_id": "ho-…|null"             // 从交接包接续时携带
}

// TaskStatus
{
  "task_id": "task-…",
  "agent": "pi",
  "title": "…",
  "state": "created"|"running"|"awaiting_action"|"finishing"|"review_ready"|"accepted"|"rejected"|"cancelled"|"failed"|"interrupted",
  "attribution": "agent_only" | "mixed",
  "baseline": {"head": "…|null", "captured_at": "…"},
  "created_at": "…", "ended_at": "…|null",
  "cancel_mode": "native"|"forced"|null,
  "latest_evidence_version": 2
}
```

约束：
- 一个活动工作区同一时刻只允许一个非终态任务；违反返回 `TASK_ALREADY_RUNNING`。
- 前置条件：工作区 trusted、目标运行时 ready，否则 `WORKSPACE_NOT_TRUSTED` / `RUNTIME_NOT_READY`。
- 任务创建时 Sidecar 记录 Git 基线（HEAD、临时索引 write-tree 的树对象、脏文件清单）；基线前已有修改永不归因给 Agent。
- 当前任务处于 `created` / `running` / `awaiting_action` / `finishing` 时，成功的 `fs.write`、`fs.create_file`、`fs.create_dir`、`fs.rename` 会自动发出 `task.manual_edit`（`source: "fs_write"`）。每次成功写入都推送事件；持久化归因原因与 `manual_edit_paths` 按（任务、路径）去重。`review_ready` 及之后的写入不再改变该任务归因。
- 自动归因的持久化或事件发送失败不影响文件操作本身的成功响应；归因只负责诚实标记，绝不充当保存门禁。
- 取消流程：先经原生通道请求停止 → 超时（默认 10s，可测试注入）→ 强制终止；事件 `task.cancelled` 的 payload 带 `{"mode": "native"|"forced"}`。
- Sidecar 重启后发现非终态任务 → 标记 `interrupted`，不自动恢复或重放。

### 3.5 交付审查（review.* / delivery.*）

| 方法 | params | result |
| --- | --- | --- |
| `review.get` | `{"task_id": …, "version": 2}`（version 省略 = 最新） | `ReviewBundle` |
| `delivery.accept` | `{"task_id": …, "evidence_version": 2}` | `{"decision": Decision}` |
| `delivery.reject` | `{"task_id": …, "evidence_version": 2, "reason": "…|null"}` | `{"decision": Decision}` |

```jsonc
// ReviewBundle — 只读，无任何写入/编辑/保存能力
{
  "task_id": "task-…",
  "evidence_version": 2,
  "is_latest": true,
  "outcome": "finished"|"cancelled"|"failed"|"interrupted",
  "attribution": "agent_only"|"mixed",
  "attribution_reasons": ["用户于 08:12 标记人工编辑"],
  "summary": "…",                              // 脱敏、大小受限
  "files": [{"path": "src/auth.rs", "change": "modified"|"added"|"deleted"|"renamed", "diff": "…", "truncated": false,
             "end_hash": "sha256:<64位hex>|null"}],   // 结束树中该文件字节的 sha256（deleted 或 >8MiB 为 null）；
                                                       // 供编辑器归因 gutter 判断"证据是否仍与磁盘一致"（15 号设计文档）
  "verification": {"status": "passed"|"failed"|"not_run", "detail": "…", "source": "agent"|"user_marked"},
  "baseline_dirty_files": ["docs/x.md"],       // 任务前已有修改，明确与关联变更区分
  "manual_edit_paths": ["src/auth.rs"]         // 任务活跃期内发生过 fs 写类人工介入的工作区相对路径去重清单
}

// Decision
{"kind": "accepted"|"rejected", "task_id": "…", "evidence_version": 2, "decided_at": "…", "reason": "…|null"}
```

约束：
- 只有**最新**证据版本可以 accept/reject；对旧版本操作返回 `EVIDENCE_NOT_LATEST`。
- accept/reject 只写入本地结论记录：不执行任何 `git commit/push/branch/tag`、不回滚、不删除文件。
- 验证结果只来自 Agent 原生运行时事件或用户 `task.mark_verification` 显式标记 not_run。
- `end_hash` 缺失、文件 diff 被截断或当前文件哈希失配时，编辑器只能展示文件级变更徽章，不能展示行级归因装饰。

### 3.6 交接（handoff.*）

| 方法 | params | result |
| --- | --- | --- |
| `handoff.preview` | `{"task_id": …, "selected_files": ["a.rs"]\|null}` | `{"package": HandoffPackage}`（null = 默认全部关联文件） |
| `handoff.create` | `{"task_id": …, "target_agent": "opencode", "selected_files": […]}` | `{"handoff_id": "ho-…", "package": HandoffPackage}` |

```jsonc
// HandoffPackage — 构造上就不可能包含完整对话、原始工具日志、凭据或配置文件
{
  "handoff_id": "ho-…|null",              // preview 时为 null
  "task_id": "task-…",
  "source_agent": "pi", "target_agent": "opencode|null",
  "goal": "…",                              // 任务目标
  "summary": "…",                           // 主 Agent 摘要（脱敏、限长）
  "selected_changes": [{"path": "…", "diff": "…"}],
  "verification": {"status": "…", "detail": "…"},
  "created_at": "…|null"
}
```

约束：任务必须处于 `review_ready` 之后的终态（review_ready/accepted/rejected/cancelled/failed/interrupted 中含可审查交付的状态）；运行中任务调用返回 `TASK_STILL_RUNNING`。不实现自动委派/自动重试/自动故障转移。

### 3.7 历史（history.*）

| 方法 | params | result |
| --- | --- | --- |
| `history.list` | `{"limit": 50}` | `{"tasks": [TaskStatus], "decisions": [Decision]}` |
| `history.evidence` | `{"task_id": …}` | `{"versions": [ReviewBundle 摘要形式（不含逐文件 diff 正文）]}` |

本地历史只保存脱敏、大小受限的摘要与 Diff 证据（单文件 diff ≤ 256 KiB，总量 ≤ 4 MiB/版本，summary ≤ 16 KiB，超限截断并带 `truncated: true`）。


## 4. 事件目录

| event | payload | 说明 |
| --- | --- | --- |
| `sidecar.state` | `{"state": "ready", "protocol_version": 1}` | 启动后首条事件 |
| `workspace.changed` | `WorkspaceStatus` | 打开/信任/撤销/关闭 |
| `runtime.state` | `{"agent": …} ∪ RuntimeStateInfo` | 每个受管应用独立推送 |
| `task.state` | `{"state": …, "task": TaskStatus}` | 任务状态机迁移 |
| `task.phase` | `{"phase": "planning"\|"editing"\|"verifying"\|"summarizing", "detail": "…"}` | 结构化阶段（来自 Agent 原生输出的规范化） |
| `trace.item` | `TraceItem` | 结构化运行轨迹条目 |
| `task.action_request` | `{"request_id": …, "kind": "permission"\|"clarification", "prompt": "…", "channel": "native"}` | Agent 暂停等待用户经其原生通道决定；Halo 不提供通用批准对话框或决议 API。Agent 经原生通道获得决定并恢复输出后，适配器再把任务带回 `running`。 |
| `task.verification` | `{"status": "passed"\|"failed"\|"not_run", "detail": "…", "source": "agent"\|"user_marked"}` | |
| `task.manual_edit` | `{"note": "…", "source": "user_marked"\|"fs_write", "path": null\|"src/auth.rs"}` | 显式标记使用 `user_marked` 与 `path: null`；任务活跃期成功的文件系统写入自动使用 `fs_write` 与非空工作区相对路径。 |
| `task.cancelled` | `{"mode": "native"\|"forced"}` | |
| `task.finished` | `{"outcome": "finished"\|"failed", "evidence_version": 1\|null, "reason": "evidence_persistence_failed"\|null}` | 证据成功落库时给出版本并进入可审查状态；本地证据写入失败时 `outcome=failed`、`evidence_version=null`，不得进入可审查状态。 |

```jsonc
// TraceItem — 主界面的结构化过程视图；原始终端输出永不作为主内容
{
  "kind": "phase"|"agent_note"|"file_hint"|"action_request"|"verification"|"lifecycle",
  "text": "…",                 // 规范化、脱敏后的单条内容（≤ 4 KiB）
  "detail": {}                  // 事件类型相关的结构化附加字段
}
```

## 5. 错误码

`error.code` 为稳定字符串（SCREAMING_SNAKE_CASE），`message` 为中文用户可读文案：

`HELLO_REQUIRED` · `PROTOCOL_VERSION_UNSUPPORTED` · `METHOD_NOT_FOUND` · `INVALID_PARAMS` · `INTERNAL` ·
`WORKSPACE_PATH_INVALID` · `WORKSPACE_NOT_READABLE` · `WORKSPACE_NOT_GIT` · `WORKSPACE_NOT_TRUSTED` · `WORKSPACE_NOT_ACTIVE` · `WORKSPACE_IDENTITY_CHANGED` ·
`CREDENTIAL_STORE_UNAVAILABLE` · `CREDENTIAL_NOT_FOUND` · `ENV_NOT_WHITELISTED` · `CONFIG_NOT_FOUND` · `CONFIG_CONFLICT` ·
`RUNTIME_NOT_READY` · `RUNTIME_PROBE_FAILED` · `RUNTIME_VERSION_MISMATCH` · `RUNTIME_ALREADY_RUNNING` ·
`TASK_ALREADY_RUNNING` · `TASK_RUNNING`（阻止工作区切换/关闭） · `TASK_NOT_FOUND` · `TASK_STILL_RUNNING` · `TASK_NOT_REVIEWABLE` ·
`EVIDENCE_NOT_FOUND` · `EVIDENCE_NOT_LATEST` · `EVENT_GAP` · `HANDOFF_NOT_FOUND` · `LINE_TOO_LONG` · `PARSE_ERROR`

## 6. 契约纪律

- 本文件与 `protocol/v1/*.schema.json` 同步演进；任何消息形状变更必须先改这里。
- 破坏性变更 → `v` 递增并保留旧版本协商；首期只有 v1。
- 生产 Sidecar 不实现任何“模拟模式”开关；测试 Sidecar / 受控假进程只存在于 `app/tests/` 与 `halo-testkit`。
