# 模块契约与文件所有权

本文件定义每个模块的**职责边界、公共 API 形状与文件所有权**。并行开发时：只允许修改自己所有的路径；跨模块调用一律以本文件与 [legacy-ipc-protocol.md](legacy-ipc-protocol.md) 为准。集成阶段允许 `halo-sidecar` 集成者做**签名级适配**（不得改变语义）。

## 0. 全局纪律

- 语言：标识符英文；文档与用户可读文案中文；代码注释仅在表达"代码本身说不出的约束"时书写，用中文。
- Rust：2021 edition，`thiserror` 定义错误，禁止 `unwrap()`/`expect()` 出现在非测试代码的可达路径（启动期不变量除外）。所有依赖版本从 workspace `[workspace.dependencies]` 继承，不得私自引入重复依赖。
- 凭据红线：`Secret` 值禁止实现 `Display`，`Debug` 输出 `Secret(***)`；任何日志、错误 message、IPC 消息、SQLite 值都不得携带凭据明文。
- 测试替身只能位于 `halo-testkit`、各 crate 的 `#[cfg(test)]`、`app/tests/`。生产路径无 mock 回退。
- **crate 解耦纪律**：`halo-protocol`、`halo-core`、`halo-config`、`halo-store`、`halo-runtime`、`halo-testkit` 六个 crate 相互之间**零依赖**（各自只依赖外部库），可独立 `cargo test -p <crate>`。只有 `halo-sidecar` 依赖全部业务 crate，并负责协议 DTO ↔ 各 crate 自有类型之间的映射。
- 时间戳统一 `time::OffsetDateTime` + RFC3339（UTC）；ID 统一 `uuid v4`，前缀见 IPC 文档（ws-/cfg-/task-/ho-/r-）。

## 1. sidecar/crates/halo-protocol —— 消息契约层

**职责**：IPC 文档第 1–5 节的全部消息类型、serde 序列化、封包读写与校验。纯类型 + IO 助手，不含业务逻辑。

```rust
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

// 封包
pub struct RequestEnvelope { pub v: u32, pub id: String, pub method: String, pub params: serde_json::Value }
pub struct Response { pub v: u32, pub id: String, pub ok: bool, pub result: Option<serde_json::Value>, pub error: Option<ErrorBody> }
pub struct ErrorBody { pub code: ErrorCode, pub message: String, pub details: serde_json::Value }
pub struct Event { pub v: u32, pub seq: u64, pub ts: String, pub task_id: Option<String>, pub event: String, pub payload: serde_json::Value }

pub enum ErrorCode { HelloRequired, ProtocolVersionUnsupported, /* …IPC 文档第 5 节全部，serde 序列化为 SCREAMING_SNAKE_CASE… */ }

// 每个方法的 typed params/result 结构体（模块 methods::workspace / config / runtime / task / review / handoff / history）
// 命名规范：OpenWorkspaceParams / WorkspaceStatus / LaunchConfigInput / TaskSpec / TaskStatus / ReviewBundle / HandoffPackage …
// 与 IPC 文档字段一字不差；enum 一律小写蛇形 serde(rename_all = "snake_case")。

// 封包 IO（同步）
pub fn write_message<W: std::io::Write>(w: &mut W, msg: &impl serde::Serialize) -> Result<(), ProtocolError>;
pub fn read_message(line: &str) -> Result<Inbound, ProtocolError>;   // 校验 v、kind、长度
pub enum Inbound { Request(RequestEnvelope) }
```

**测试**：每种消息 serde round-trip；错误码字符串稳定性快照；超长行、坏 JSON、错误版本的拒绝路径。

## 2. sidecar/crates/halo-core —— 领域状态机（纯逻辑，无 IO）

**职责**：工作区信任、任务状态机、基线/归因、证据版本、审查决定、交接包、脱敏与限长。**不依赖** rusqlite/进程/网络，也不依赖其他 halo crate；只依赖 serde/serde_json/time/uuid/regex/thiserror。领域类型为 core 自有类型，由 halo-sidecar 负责与协议 DTO 互转。

```rust
// 工作区信任
pub struct WorkspaceIdentity { pub real_path: String, pub root_commit: Option<String> }
pub enum TrustState { Untrusted, Trusted }
pub fn evaluate_trust(saved: Option<&TrustRecord>, current: &WorkspaceIdentity) -> TrustEvaluation; // identity_changed 降级

// 任务状态机（唯一合法迁移表；非法迁移返回 TransitionError）
pub enum TaskState { Created, Running, WaitingDeveloper, AwaitingAction, Finishing, ReviewReady, Accepted, Rejected, Cancelled, Failed, Interrupted }
pub enum TaskEvent { Started, RoundCompleted, FollowUpSent, ActionRequested, ActionResolved, FinishRequested, Finishing, EvidenceReady, Accept, Reject, CancelledNative, CancelledForced, Fail(String), MarkInterrupted }
impl TaskState { pub fn apply(self, ev: &TaskEvent) -> Result<TaskState, TransitionError>; pub fn is_terminal(&self) -> bool; pub fn is_reviewable(&self) -> bool; }

// 归因：基线前修改永不归因 Agent；人工介入 → Mixed
pub enum Attribution { AgentOnly, Mixed { reasons: Vec<String> } }
pub enum ManualEditOp { Write, CreateFile, CreateDir, Rename }
pub fn manual_edit_note(op: ManualEditOp, path: &str, to_path: Option<&str>, local_hhmm: &str) -> String;
pub struct Baseline { pub head: Option<String>, pub tree: String, pub dirty_files: Vec<String>, pub captured_at: String }

// 证据（追加式：只能 new 下一个版本，禁止修改旧版本）
pub struct FileEvidence { pub path: String, pub change: ChangeKind, pub diff: String, pub truncated: bool, pub end_hash: Option<String> }
pub struct EvidenceVersion { pub version: u32, pub outcome: Outcome, pub attribution: Attribution, pub summary: String, pub files: Vec<FileEvidence>, pub verification: Verification, pub created_at: String }
pub struct EvidenceLog(/* 私有 Vec */);
impl EvidenceLog { pub fn append(&mut self, draft: EvidenceDraft) -> &EvidenceVersion; pub fn latest(&self) -> Option<&EvidenceVersion>; }

// 验证结论
pub enum VerificationStatus { Passed, Failed, NotRun }
pub struct Verification { pub status: VerificationStatus, pub detail: String, pub source: VerificationSource /* Agent | UserMarked */ }

// 交接包：构造函数只接收白名单字段，类型上不可能塞入对话/日志/凭据
pub struct HandoffDraft { pub goal: String, pub summary: String, pub selected_changes: Vec<SelectedChange>, pub verification: Verification }
pub fn build_handoff(evidence: &EvidenceVersion, goal: &str, selected: Option<&[String]>) -> HandoffDraft;

// 脱敏与限长（store 与 sidecar 出口双重使用）
pub fn sanitize(text: &str) -> String;                 // 常见密钥模式替换为 [REDACTED]
pub fn cap(text: &str, max_bytes: usize) -> (String, bool /*truncated*/);
pub mod limits { pub const SUMMARY_MAX: usize = 16*1024; pub const FILE_DIFF_MAX: usize = 256*1024; pub const VERSION_TOTAL_MAX: usize = 4*1024*1024; pub const TRACE_TEXT_MAX: usize = 4*1024; pub const MANUAL_EDIT_REASONS_MAX: usize = 64; }
```

**测试**：状态机全迁移表（合法+非法）；identity_changed 降级；追加式证据不可覆盖；sanitize 对典型密钥样式（`sk-…`、`AKIA…`、`Bearer …`、`password=`、PEM 头）全部命中；cap 截断标记。

## 3. sidecar/crates/halo-config —— 启动配置 / 凭据边界 / 配置事务

**职责**：LaunchConfig 校验、凭据存取抽象（失败关闭）、子进程环境白名单、原生配置文件的配置事务。

```rust
pub struct Secret(String);           // Debug=“Secret(***)”；无 Display；提供 expose(&self)->&str 仅限启动注入点使用
pub trait CredentialStore: Send + Sync {
    fn set(&self, ref_name: &str, secret: &Secret) -> Result<(), CredentialError>;
    fn get(&self, ref_name: &str) -> Result<Secret, CredentialError>;   // 不存在 => CredentialError::NotFound
    fn exists(&self, ref_name: &str) -> Result<bool, CredentialError>;
    fn available(&self) -> bool;     // OS 存储不可用 => 一切失败关闭
}
pub struct WindowsCredentialStore;   // keyring(windows-native)。service 固定 "HaloStudio"
pub struct CredentialError { … }     // 变体：StoreUnavailable / NotFound / Backend(String)——message 不含明文

// LaunchConfig 为 config 自有类型（字段与 IPC 文档 LaunchConfigInput 同构），由 halo-sidecar 映射。
// 凭据只保存引用名；不接受任意启动参数、凭据环境变量名或环境覆盖。
pub struct LaunchConfig { pub id: String, pub name: String, pub agent: AgentKind, pub executable_path: String, pub model: String, pub thinking_level: ThinkingLevel, pub credential_ref: Option<String>, pub created_at: String, pub updated_at: String }
pub fn validate_launch_config(cfg: &LaunchConfig) -> Result<(), ConfigError>;
pub fn credential_env_var_for(agent: AgentKind, model: &str) -> Result<&'static str, ConfigError>; // OpenCode provider/model -> 固定白名单变量

pub const ENV_WHITELIST: &[&str] = &["SYSTEMROOT","WINDIR","PATH","TEMP","TMP","USERPROFILE","COMSPEC","PATHEXT","SystemDrive","NUMBER_OF_PROCESSORS","PROCESSOR_ARCHITECTURE"];
pub fn build_child_env(host: &HashMap<String,String>, injected: Vec<(String, Secret)>) -> HashMap<String,String>;

// 配置事务（对 Pi/OpenCode 原生配置文件；与 Agent 任务完全无关）
pub struct ConfigTransaction { … }
impl ConfigTransaction {
    pub fn begin(path: &Path, new_content: String) -> Result<Self, TxError>;     // 记录原内容 sha256
    pub fn preview(&self) -> String;                                             // similar 文本 diff
    pub fn commit(self) -> Result<TxReceipt, TxError>;  // 冲突检测(原文件 hash 变了=>Conflict) → 备份 → 临时文件+rename 原子写
    // TxReceipt { backup_path } ；pub fn rollback(receipt: &TxReceipt) -> Result<(), TxError> 从备份可验证恢复
}
```

**测试**：内存 `FakeCredentialStore`（含 available=false 模式）验证失败关闭；build_child_env 仅透传白名单、注入变量只在返回 map 中出现一次；事务的冲突/原子写/回滚（tempfile）；`format!("{:?}", secret)` 不含明文。

## 4. sidecar/crates/halo-store —— 本地持久化（SQLite）

**职责**：`rusqlite`（bundled）。**不依赖其他 halo crate**：记录结构体为 store 自有类型（字段与 core/协议同构，sidecar 负责映射）。脱敏由 sidecar 在入库前经 `halo_core::sanitize` 完成；store 自身以注入的 `StoreLimits`（默认与 core::limits 一致）强制执行大小上限与截断标记（防御纵深）。数据库路径由调用方传入（生产：`%LOCALAPPDATA%\HaloStudio\halo.db`；测试：tempdir）。

```rust
pub struct Store { … }
impl Store {
    pub fn open(path: &Path, limits: StoreLimits) -> Result<Store, StoreError>;   // 内嵌迁移，schema_version 表；StoreLimits 提供 Default
    // 信任记录
    pub fn get_trust(&self, real_path: &str) -> Result<Option<TrustRecord>, StoreError>;
    pub fn put_trust(&self, rec: &TrustRecord) -> Result<(), StoreError>;
    pub fn revoke_trust(&self, real_path: &str) -> Result<(), StoreError>;
    // 启动配置（credential_ref 只存引用名）
    pub fn list_configs(&self) / put_config / delete_config;
    // 任务与证据（证据 INSERT-only；UPDATE 旧版本行 => 直接拒绝）
    pub fn put_task(&self, t: &TaskRecord) / get_task / list_tasks(limit);
    pub fn append_evidence(&self, task_id: &str, e: &EvidenceVersion) -> Result<u32, StoreError>; // 返回版本号=max+1
    pub fn list_evidence(&self, task_id) / latest_evidence(task_id);
    pub fn put_decision(&self, d: &Decision) / list_decisions;
    pub fn put_handoff(&self, h: &HandoffRecord) / get_handoff;
    // 中断恢复：启动时把所有非终态任务标记 interrupted
    pub fn mark_non_terminal_interrupted(&self) -> Result<Vec<String>, StoreError>;
}
```

表：`schema_version` / `trust_records(real_path PK, root_commit, trusted, decided_at)` / `launch_configs` / `tasks`（含 JSON `manual_edit_paths`）/ `evidence(task_id, version, …, PRIMARY KEY(task_id,version))`（files JSON 含可选 `end_hash`）/ `decisions` / `handoffs`。

**测试**：迁移幂等；append-only（尝试重写同版本 → 错误）；mark_non_terminal_interrupted；超限内容截断并带 truncated 标记。（脱敏断言在 sidecar 单测与集成测试层完成。）

## 5. sidecar/crates/halo-runtime —— 受管运行时

**职责**：进程监督、Pi RPC 适配器、OpenCode 回环服务适配器、取消/停止语义。线程 + `crossbeam-channel`，无 async。

**适配器协议（本项目的权威定义，halo-testkit 假进程按此实现）**：

- Pi：`<exe> --rpc` 后 stdio JSONL。探测 `<exe> --version` → 首行 semver。就绪：发 `{"id":1,"method":"get_state"}` 期待 `{"id":1,"result":{"state":"idle"}}`（超时默认 10s，可注入）。任务：`{"id":N,"method":"run_task","params":{instructions,files,base_diff,notes}}`；Pi 以 `{"method":"event","params":{TraceItem 同构}}` 流式通知，`kind` 含 phase/agent_note/file_hint/action_request/verification，最后 `{"id":N,"result":{"outcome":"finished"|"failed","summary":…}}`。取消：`{"id":M,"method":"cancel"}` → Pi 应结束 run_task。EOF/坏帧 → Failed{reason}。
- OpenCode：兼容性档案为 `OPENCODE_COMPATIBILITY_PROFILE = "opencode-server-1.x"`，只接受稳定 `>= 1.18.5, < 2.0.0`；未知主版本、预发布或畸形版本均失败关闭。模型以受支持的 `provider/model` 形式选择，Sidecar 用内置白名单映射把凭据引用短暂注入相应的真实 Provider 环境变量；不接受任意变量名，未知 Provider 失败关闭。启动 `<exe> serve --hostname 127.0.0.1 --port <p>`，`p` 由 Sidecar 选空闲端口。每次启动生成私有随机密码，仅以 `OPENCODE_SERVER_PASSWORD` 注入受管进程；全部 HTTP 请求以用户名 `opencode` 使用 Basic 认证。就绪请求为 `GET /global/health`，必须返回 `{"healthy":true,"version":"…"}`。任务建立私有 `POST /session`；每一轮先建立已认证 `GET /event` 流，再向同一 `POST /session/{id}/prompt_async` 发送首条或后续显式消息。`idle` 后以 session status/message 读取新的 completed assistant text，运行时用私有消息游标排除上一轮回复并只发出 `SessionReply` 与规范化轨迹。远程 session/message 标识不离开私有句柄；显式结束只在已完成轮次释放该句柄，不调用 abort，取消才调用原生 abort。OpenCode 的 `TaskDone` 不能触发正常交付结束。停止实例仍经 `/global/dispose` 与子进程监督。端口、认证用户名、密码与 Authorization 值绝不进入 Debug、错误、事件、IPC、日志或存储。

**兼容性发布门槛：** 扩大 `opencode-server-1.x` 范围时，必须同时增加对应协议能力的自动化证据和一次真实原生 UI 会话验收记录；OpenCode `2.x` 不得自动继承该档案，必须以独立兼容性档案重新验证。
- OpenCode 操作请求：SSE `permission.asked`、`question.asked` 仅规范化为脱敏限长的 `ActionRequest`，并把远程请求标识映射保留在私有句柄；不得放入 IPC、日志、错误、存储或 Debug。Sidecar 对精确匹配的本次权限决定调用 `POST /permission/{id}/reply`（`{"reply":"once"}` 或 `{"reply":"reject"}`），对澄清调用 `POST /question/{id}/reply`（P0 标量答案）或 `POST /question/{id}/reject`。只有同一请求的 `permission.replied`、`question.replied` 或 `question.rejected` SSE 反馈才产生 `ActionResolved`；HTTP 成功本身不能把任务推进为运行中。取消后不再接受决议，重复或不匹配请求失败关闭。
- OpenCode 运行隔离：运行时在私有临时目录中创建配置、数据、缓存和状态根，并以 `XDG_CONFIG_HOME`、`XDG_DATA_HOME`、`XDG_CACHE_HOME`、`XDG_STATE_HOME` 注入每次启动；这些目录由私有运行时句柄持有，不会复用用户全局 OpenCode 状态。

```rust
pub enum RuntimeState { NotProbed, Probing, Starting, Ready, Failed { reason: String, recovery_hint: String }, Stopping, Stopped }
pub struct RuntimeTraceItem { pub kind: String, pub text: String, pub detail: serde_json::Value }   // runtime 自有类型；sidecar 映射为契约 TraceItem
pub enum RuntimeEvent { State(RuntimeState), Trace(RuntimeTraceItem), ActionRequest { request_id: String, kind: String, prompt: String }, ActionResolved { request_id: String }, Verification { status: String, detail: String }, SessionReply { text: String }, TaskDone { outcome: String, summary: String } }
pub struct LaunchCmd { pub exe: String, pub env: HashMap<String,String>, pub cwd: String } // env 已由 halo-config 构好

pub struct RunTaskSpec { pub instructions: String, pub files: Vec<String>, pub base_diff: Option<String>, pub notes: Option<String> }  // runtime 自有类型
pub struct PiRuntime;   impl PiRuntime   { pub fn probe(exe:&str)->Result<String,RuntimeError>; pub fn start(cmd:LaunchCmd, tx:Sender<RuntimeEvent>, opts:Timeouts)->Result<PiHandle,RuntimeError>; }
pub struct PiHandle;    impl PiHandle    { pub fn run_task(&self, spec:&RunTaskSpec)->Result<(),RuntimeError>; pub fn cancel_native(&self); pub fn stop(&self, grace:Duration)->StopOutcome; pub fn state(&self)->RuntimeState; }
pub struct OpenCodeRuntime / OpenCodeHandle;  // 同构启动/停止 API；内部持有端口和认证信息，**绝不**出现在任何公开 getter/Debug 中
pub enum StopOutcome { Graceful, Forced }
pub struct Timeouts { pub ready: Duration, pub cancel_grace: Duration, pub shutdown_grace: Duration } // Default 10s/10s/5s
```

**测试**：Pi 的不 spawn 真进程单元测试用 transport trait 注入内存管道（分帧、乱序 id 响应、EOF、坏 JSON）；OpenCode 用临时 HTTP 服务覆盖 Basic 认证、`/global/health`、版本不兼容、缺少版本与认证失败，以及 session/SSE/idle/message 回路与认证信息不泄漏。真实子进程集成测试放 `sidecar/tests/`（用 halo-testkit 的 bin），覆盖 OpenCode `task.create` 的首轮真实会话、规范化回复、`waiting_developer` 和无旧协议回退。

## 6. sidecar/crates/halo-sidecar —— 可执行入口

**职责**：stdio JSONL 服务；方法路由；EventBus（全局 seq + 1024 环形缓冲 + 快照）；AppState 编排（workspace/config/runtime/task/评审/交接全流程）；GitClient；CLI 子命令。

```
src/main.rs        — CLI: 默认 serve；`cred set <ref>`(stdin 读密钥,写 CredentialStore)；`cred check <ref>`
src/server.rs      — stdin 读循环、stdout 唯一写线程、EventBus{next_seq, ring}
src/dispatch.rs    — method 字符串 → handler；hello 门禁；统一错误映射
src/state.rs       — AppState { workspace, store, configs, pi, opencode, task }（Mutex 保护）
src/git.rs         — GitClient：canonicalize+校验、rev-parse、临时索引 write-tree 捕获基线树、diff-tree 生成任务关联 diff、status 脏文件、只读 cat-file 取结束树文件字节
src/task_flow.rs   — 任务编排：前置校验→基线→runtime.run_task→事件规范化→终态→证据落库
```

Git 基线/关联变更算法（锁定）：
1. 基线：`GIT_INDEX_FILE=<tmp> git add -A` + `git write-tree` → `baseline_tree`（含未跟踪文件；不动真实索引/工作树）；同时记录 `HEAD` 与 `git status --porcelain` 脏文件清单。
2. 结束：同法得 `end_tree`；关联变更 = `git diff <baseline_tree> <end_tree>` 按文件切分。
3. 基线时已脏的文件出现在 `baseline_dirty_files`，UI 单独展示，不归因 Agent。

**约束**：绝不执行 `git commit/push/branch/checkout/stash apply` 等修改性命令（只读命令 + 临时索引 write-tree 除外）。

## 7. sidecar/crates/halo-testkit —— 受控假进程（仅测试）

bins：`fake-pi`、`fake-opencode`，严格实现第 5 节适配器协议。行为经环境变量脚本化：
`FAKE_PI_MODE` = `happy` | `not_ready`(get_state 永不回) | `garbage`(输出坏帧) | `crash_mid_task` | `hang_on_cancel`(忽略 cancel，验证强杀) | `action_request`(中途发权限请求) | `verify_fail`；`FAKE_PI_VERSION` 覆盖版本输出。
`FAKE_OC_MODE` = `happy` | `initial_idle` | `initial_busy_then_idle` | `stale_idle` | `missing_busy_eof` | `fast_initial_round` | `message_error` | `permission_once` | `permission_reject` | `clarification_once` | `clarification_reject` | `unhealthy` | `old_version` | `wrong_version` | `major_version` | `malformed_version` | `pre_release_version` | `missing_health_version` | `bad_auth`(401) | `wrong_ready_address` | `missing_ready_line` | `exit_early` | `dispose_failure` | `hang_on_dispose`。`initial_*` 模拟 prompt 前的状态事件；`missing_busy_eof` 在持久化回复后不发送 busy 并关闭事件流；`fast_initial_round` 在 `prompt_async` 返回 204 前持久化回复、发送重复 idle 并关闭事件流；`permission_*` 与 `clarification_*` 分别发出一次 `permission.asked` 或 `question.asked`，允许测试本次允许/拒绝或单项回答后的真实 SSE 回执。

假服务必须只绑定 `127.0.0.1`，校验 `OPENCODE_SERVER_PASSWORD` 对应的 Basic 认证，并实现 `GET /global/health`、`POST /session`、`GET /event`、`POST /session/{id}/prompt_async`、`GET /session/status`、`GET /session/{id}/message`、`POST /permission/{id}/reply`、`POST /question/{id}/reply`、`POST /question/{id}/reject` 与 `POST /global/dispose`。happy 覆盖首轮真实会话；不提供旧任务、事件或取消端点，也不伪造交付完成。

## 8. app/halo_studio —— PySide6/QML

```
halo_studio/ipc/connection.py   — 纯 Python（无 Qt）：spawn sidecar(路径来自 HALO_SIDECAR_EXE 或默认 sidecar/target/debug/halo-sidecar.exe)、
                                  JSONL 读写线程、request(method,params)→Future、事件回调、hello 版本协商、断连原因
halo_studio/ipc/client.py       — Qt 包装（QObject+Signal），把 connection 的回调转主线程信号
halo_studio/viewmodels/*.py     — AppViewModel(连接状态/协议版本/不可用原因)、WorkspaceViewModel、ConfigViewModel、
                                  RuntimeViewModel(pi/opencode 独立状态)、TaskViewModel(创建表单+状态)、TraceViewModel(列表模型)、
                                  ReviewViewModel(文件列表+只读diff+验证+accept/reject)、HandoffViewModel、HistoryViewModel
halo_studio/qml/Main.qml        — ApplicationWindow：左侧工作区/运行时状态栏，中部 Task/Trace，审查页，交接对话框，配置页；
                                  底部状态条常显 Sidecar 连接、协议版本、不可用原因
halo_studio/main.py             — 入口；`--smoke` 加载 QML 校验根对象后打印 "SMOKE-OK" 退出 0（不依赖 Electron/浏览器）
halo_studio/differentiation/**  — 任务上下文草稿、人工介入通知、审查跳转、基线徽章与归因 gutter；只消费 Sidecar/编辑器公开事实，不读本地工作区
```

- ViewModel 只经 client 说契约语言，无业务旁路；审查视图**只读**（TextArea readOnly，无保存/编辑动作）。
- `app/tests/fake_sidecar.py`：符合 v1 契约的测试 Sidecar（stdin/stdout JSONL，可脚本化响应）——界面/单元测试专用测试接缝。
- 测试：connection 层（真实子进程 fake_sidecar）、各 ViewModel（pytest-qt，QCoreApplication）、`--smoke`。

## 9. scripts/

`dev.ps1`（构建 sidecar → venv 启动 app）、`test-all.ps1`（cargo test --workspace + pytest）、`smoke-windows.ps1`（构建 + `python -m halo_studio --smoke` + 断言进程模块不含 electron/chrome/webview、检查仓库无 electron/react/vite 依赖）。

## 10. 文件所有权矩阵（并行开发期间不得越界）

| 所有者 | 路径 |
| --- | --- |
| rust-protocol | `sidecar/crates/halo-protocol/**` |
| rust-core | `sidecar/crates/halo-core/**` |
| rust-config | `sidecar/crates/halo-config/**` |
| rust-store | `sidecar/crates/halo-store/**` |
| rust-runtime | `sidecar/crates/halo-runtime/**` |
| rust-sidecar(集成) | `sidecar/crates/halo-sidecar/**`（+ 集成期跨 crate 签名级修复） |
| rust-testkit | `sidecar/crates/halo-testkit/**` |
| rust-integration | `sidecar/tests/**`（workspace 级集成测试 crate：`sidecar/crates/halo-integration-tests` 亦归其所有） |
| py-ipc | `app/halo_studio/ipc/**`, `app/tests/fake_sidecar.py`, `app/tests/test_connection*.py` |
| py-viewmodels | `app/halo_studio/viewmodels/**`, `app/tests/test_viewmodels*.py` |
| py-qml | `app/halo_studio/qml/**`, `app/halo_studio/main.py`, `app/halo_studio/app.py`, `app/tests/test_smoke*.py` |
| py-differentiation | `app/halo_studio/differentiation/**`, `app/tests/test_differentiation.py` |
| scripts/docs | `scripts/**`, `docs/traceability.md` |
