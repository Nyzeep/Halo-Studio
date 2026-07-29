# 01 - 固化可迁移能力基线与仓库卫生

**What to build:** 维护者可以从一份可重复的自动化结果和状态记录确认旧六票中哪些行为已成为可迁移能力基线、哪些真实验收仍未完成，并在不删除旧产品实现的前提下清除可确认的临时产物。

**Blocked by:** None - can start immediately.

**Status:** ready-for-review（2026-07-29：MSVC 工具链、workspace check/build/test、Python/QML、Schema 和凭据清理验证均通过；详见 [工单 01 执行证据](../../../verification/migratable-capability-baseline/issue-01-freeze-evidence.md)）

- [x] 在 MSVC 开发环境中记录 workspace 检查、构建、Rust 测试、Python/QML 验证和 Schema 验证的精确命令与结果。合格非沙箱 Windows 用户会话中的 MSVC 探测、linker 顺序以及 `cargo check --workspace`、`cargo build --workspace`、`cargo test --workspace` 均通过，退出码均为 0，失败归类与历史受限环境复现见执行证据。
- [x] 将 GitHub #9–#14（旧六票）明确标为“可迁移能力基线”，不得表述为目标 Tauri 产品已验收或 P0 已放行。
- [x] 将真实 OpenCode 原生 UI 验收保持为未完成门槛，并保留此前失败的归类与复现证据。
- [x] 只移除可确认的临时测试目录、缓存和安装器；本轮删除了隔离 worktree 中本轮创建的 pytest 临时目录和 Cargo 生成的 `sidecar\\target`，未触碰主工作区用户资产、迁移文档、旧 QML/Sidecar 源码或外部上游参考树。
- [x] 基线代码、测试与状态文档形成可独立审查的差分边界，不混入迁移实现或无关工作树改动。当前差分仍未提交；独立 Git 提交的形成等待用户明确授权。

## 本轮状态

本轮继续完成工单 01 的基线审计和证据补录，没有进入工单 02，也没有执行真实 OpenCode 原生 UI 验收或旧产品删除。MSVC 子进程已成功启动且工具链选择正确；生命周期、测试锁和测试契约问题已在隔离 worktree 中以最小改动修复。受限沙箱中 Credential Manager 正向资格不可用的 101 退出已保留为环境复现；在获授权的非沙箱 Windows 用户会话中，合成凭据写入、读取、注入、删除和前缀残留检查均通过，`cargo check/build/test` 也均通过。`runtime_failures.rs:99` 的畸形版本断言已按既有三段 semver 解析契约修正，定向测试通过。

可复现命令、结果、失败归类、主工作区保护范围和进入下一工单的判定见 [工单 01 执行证据](../../../verification/migratable-capability-baseline/issue-01-freeze-evidence.md)。
