# 03 - 原始 01–10 任务验收与 TDD 基线

> **历史基线记录：** 本文保留旧产品的 OpenCode/Pi 运行时验收事实，不是当前 P0 规格，也不能作为新的 OpenCode 或 Pi 实现入口。

**状态：** 2026-07-27 验收复核完成（发布资格条件已单列）

## 验收范围

**目的：** 验收已确认的原始 01–10 任务及其依赖链，修复可复现的实现或测试缺口。

**纳入：** 原生工作台、受信任工作区、受管启动配置、Pi/OpenCode 运行时、单任务轨迹、交付证据、只读审查、手动交接和 Windows 自动化门禁。

**不纳入：** 完整编辑器、文件写入、`task.resolve_action`、新配置编辑工作流、MCP/插件/Skills/Prompts。这些内容不得借本次验收进入 v1 IPC；后续需求另行形成确认记录。

## 验收结论

| 任务 | 结论 | 说明 |
| --- | --- | --- |
| 01 | PASS | PySide6/QML、Rust Sidecar、v1 JSONL、根旧前端入口红线和 Windows 烟测均有自动化证据。 |
| 02 | PASS | 信任、切换、路径/ACL 和运行中任务守卫均有真实 Sidecar 集成测试。 |
| 03 | CONDITIONAL | 凭据隔离、白名单和不可用时失败关闭通过；当前非交互式凭据管理器会话只能验证失败关闭，不能证明真实凭据正向注入。 |
| 04 | CONDITIONAL | 受控 Pi 协议进程的就绪、异常和取消测试通过；本机未发现真实 Pi 可执行文件，发布前仍需人工资格验证。 |
| 05 | BLOCKED | 受控 OpenCode 服务测试通过，但本机 `opencode --version` 为 `1.18.5`，与实现锁定的 `0.4.2` 不匹配。锁定版本是产品决策，不能在验收中擅自改写。 |
| 06 | PASS | 单任务守卫、结构化轨迹、事件顺序、快照恢复和原生/强制取消均有端到端证据。 |
| 07 | PASS | 临时索引基线、未跟踪文件、人工介入、追加证据和脱敏限长均已覆盖。 |
| 08 | PASS | 只读审查、最新证据限制、接受/拒绝不改 Git、验证来源限制均已覆盖。 |
| 09 | PASS | 交接白名单、运行中拒绝、重启中断且不重放均已覆盖。 |
| 10 | CONDITIONAL | 全量自动化、烟测和静态红线通过；其发布资格依赖任务 03、04、05 的真实环境资格验证。 |

`PASS` 只表示原始任务文件中可自动验证的验收项通过；它不替代第三方受管应用的真实安装版资格验证。

## 本轮 TDD 回归

以下缺口均先新增失败用例，再修复为通过：

- 证据 SQLite 落库失败时，任务不得进入 `review_ready`：`task_flow.rs::tests::evidence_persistence_failure_never_marks_task_review_ready`。
- 强制取消必须实际终止子进程：`cancel.rs::cancel_forced_after_grace_timeout_kills_agent` 使用独占锁文件确认资源已释放。
- 工作区切换、撤销信任和 Sidecar 退出不得留下运行时：`workspace_boundary.rs` 使用同一锁文件机制确认进程退出。
- 不可读工作区必须先由 ACL 证明确实不可读，再断言 `WORKSPACE_NOT_READABLE`：`workspace_not_readable.rs::unreadable_directory_maps_to_workspace_not_readable`。
- Credential Manager 的可用性探测改为无敏感值的写入/删除探测；不可用时 `cred set` 和启动配置必须失败关闭且不回显 canary。
- 事件缓冲缺口必须重建轨迹：`app/tests/test_viewmodels.py::TestTraceViewModel::test_snapshot_event_gap_rebuilds_from_oldest_available`。
- 不可恢复的快照错误必须呈现给界面：`app/tests/test_viewmodels.py::TestTraceViewModel::test_snapshot_error_is_visible`。
- Python UI 接收端必须拒绝 v1 封包缺失必填字段或含未知字段：`app/tests/test_connection.py::test_inbound_envelope_rejects_missing_or_unknown_fields`。

## 已知范围差异

1. `halo-config` 已实现配置事务的 Diff、冲突检测、原子写入与回滚单元测试，但这项能力尚未暴露为 IPC 或 UI 工作流。它来自基础规格的后续配置写入决策，不计入当前 01–10 任务文件的完成结论。
2. 受控假进程仅证明 Halo 的协议适配、生命周期和安全边界；真实 Pi/OpenCode 的版本、协议和安装布局必须分别完成资格验证。当前环境的 OpenCode 版本不匹配，Pi 未安装。
3. 根目录旧 Electron/React/Vite 源码仅作只读历史参考；根 `package.json` 已不含运行入口，`scripts/smoke-windows.ps1` 对根入口、`app/`、`sidecar/` 和 QML 执行静态红线检查。

## 执行记录

验收使用以下命令，并在文档回填后重新执行：

```powershell
.\scripts\test-all.ps1
.\scripts\smoke-windows.ps1
```

定向 TDD 命令：

```powershell
.\.venv\Scripts\python.exe -m pytest app\tests\test_viewmodels.py -q
.\.venv\Scripts\python.exe -m pytest app\tests\test_connection.py -q
```

**本轮结果：** `scripts\test-all.ps1` 通过（Rust workspace 252 例，Python 63 通过、1 条设计内条件跳过）；`scripts\smoke-windows.ps1` 通过（默认平台 `SMOKE-OK` 与全部静态红线）。`opencode --version` 实测为 `1.18.5`；Pi 不在 PATH。
