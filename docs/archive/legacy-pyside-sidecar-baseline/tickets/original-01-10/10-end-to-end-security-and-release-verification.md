# 10 - 端到端安全与发布验证

**构建目标：** 为 Pi/OpenCode 原生工作台的完整任务交付链路提供可重复的安全、集成和 Windows 发布验证证据，确保没有模拟回退、凭据泄漏或旧 Electron 运行入口。

**依赖：** 04 - Pi 受管运行时；05 - OpenCode 受管运行时；06 - 单任务与结构化运行轨迹；07 - 任务基线与交付证据；08 - 只读审查与交付决定；09 - 手动交接与中断恢复边界。

**状态：** 条件验收（自动化门禁通过；发布资格受任务 03、04、05 的真实环境条件阻塞）。

- [x] 受控 Pi/OpenCode 进程集成测试覆盖完整任务、取消、失败、审查和交接链路。
- [ ] 凭据 canary 的真实正向注入仍待可写 Credential Manager 会话；当前环境已验证失败关闭和不回显。
- [x] Windows 原生应用烟测证明应用不依赖 Electron、React、Vite 或浏览器窗口。
- [x] 迁移验收明确拒绝旧多 Agent、Mock 回退、通用终端和 WebView 运行入口。

**验收与 TDD 证据：** `docs/requirements-alignment/03-original-ten-task-acceptance-and-tdd-baseline.md`、`docs/traceability.md`。
