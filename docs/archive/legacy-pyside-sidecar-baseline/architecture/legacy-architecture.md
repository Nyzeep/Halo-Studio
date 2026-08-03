# 架构说明（历史归档）

> 本文只记录旧 PySide/QML + Rust Sidecar 的迁移前架构，包括旧 OpenCode HTTP/SSE 适配。它不是 Halo Studio 当前 P0 的架构依据；当前链路以 Halo Workbench Runtime → Pi RPC 为准。

## 分层与职责

| 层 | 技术 | 职责 | 明确不做 |
| --- | --- | --- | --- |
| 原生界面 | PySide6 + QML | 开发工作台体验：工作区/任务/轨迹/审查/交接视图，展示真实状态与不可用原因 | 不接触凭据明文；不直接运行 git/agent；不内置编辑器 |
| 应用控制层 | Python（`ipc/`, `viewmodels/`） | 契约客户端、版本协商、事件→视图模型 | 无业务旁路、无模拟在线状态 |
| Runtime Sidecar | Rust 多 crate | 任务、运行时、配置事务、本地数据、安全敏感能力的**独占**执行者 | 不执行任意验证命令；不自动 Git 操作 |
| 受管应用 | Pi / OpenCode 原生进程 | 按各自原生权限模型读写工作区、产生验证结果 | Halo 不代为应用补丁/统一权限 |

进程模型：UI 进程 ⇄（stdio JSONL v1）⇄ `halo-sidecar.exe` ⇄（Pi: stdio RPC / OpenCode: 127.0.0.1 HTTP）⇄ 受管应用子进程。

## Sidecar 线程模型

- **stdin 读线程**：逐行解析请求 → dispatch。
- **stdout 唯一写线程**：所有响应与事件经 channel 汇聚，写线程分配全局 `seq` 并维护 1024 条环形缓冲（`task.snapshot` / `EVENT_GAP` 恢复语义的依据）。
- **运行时线程**：每个受管子进程一组读/监督线程，经 `RuntimeEvent` channel 汇报，由任务编排规范化为契约事件。
- 共享状态 `AppState` 用 `Mutex` 保护；无 async runtime。

## 关键决策记录

1. **QML ↔ Rust 采用版本化 stdio JSONL**（对齐记录 01）：首期不引入命名管道/二进制协议。
2. **凭据录入走 Sidecar CLI（out-of-band）**：`halo-sidecar cred set <ref>` 从 stdin 读入并写 Windows 凭据管理器；UI/IPC 只见引用名。这使"UI、IPC、日志、Diff、备份、数据库不暴露明文"在结构上成立，凭据 canary 测试可全链路断言。
3. **基线用临时索引 write-tree**：`GIT_INDEX_FILE=<tmp> git add -A && git write-tree` 捕获含未跟踪文件的树对象，不动用户索引与工作树；任务关联变更 = 基线树与结束树的 diff。不使用 stash/commit。
4. **适配器协议即权威**：Pi RPC 与 OpenCode 回环服务的线协议在 module-contracts.md 第 5 节锁定；`halo-testkit` 假进程与真实适配器实现同一协议，生产代码无任何 mock 分支。
5. **单工作区、单任务**：并发边界收敛为"一个活动工作区 + 一个非终态任务"，跨 Agent 协作只能经审查后的手动交接。
6. **中断如实标记**：Sidecar 启动时把库中非终态任务标记 `interrupted`；不自动恢复/重放。
7. **同步线程模型**：JSONL + 子进程监督的负载下，std 线程 + crossbeam channel 比 async 栈更小的复杂度与审计面。
8. **OpenCode 回环端口的 TOCTOU 属设计内边界**：Sidecar 先探测空闲端口、再启动 `opencode serve` 绑定该端口，两步之间端口理论上可被本机其他进程抢占（time-of-check-to-time-of-use）。影响面仅限本机同用户环境：服务只绑定 `127.0.0.1`，不出网。既有缓解：每次启动生成新的私有密码，仅经 `OPENCODE_SERVER_PASSWORD` 注入子进程；Sidecar 以用户名 `opencode` 的 HTTP Basic 认证请求 `GET /global/health`。此外，只有子进程 stdout 的 ready 行确认其正在监听预期的 `http://127.0.0.1:<port>` 后，才进行认证健康检查。端口被抢占、ready 行地址不符、认证失败或健康检查不通过均表现为启动失败（`Failed{reason, recovery_hint}`；用户重试会换新端口），不会把非预期服务伪造为 ready。据此不引入端口移交/句柄继承等额外机制。

## 数据位置

- SQLite：`%LOCALAPPDATA%\HaloStudio\halo.db`（测试用 tempdir）。仅存脱敏、限长的摘要/Diff/结论；无对话档案、无原始日志、无凭据。
- 凭据：Windows 凭据管理器（service=`HaloStudio`）。存储不可用 → 失败关闭。

## 测试接缝（对齐记录 01·测试决策）

- 主接缝 = 版本化 IPC 契约：`app/tests/fake_sidecar.py`（界面测试用测试 Sidecar）；`halo-testkit` 的 `fake-pi`/`fake-opencode`（Sidecar 集成测试用受控进程）。
- 生产不回退：Sidecar 与 App 均无 mock 开关；测试替身只存在于测试路径。
