# 01 - 原生工作台最小闭环

**构建目标：** 提供可启动的 PySide6/QML 原生工作台和 Rust Runtime Sidecar，使界面能通过版本化 JSONL 契约获取真实的 Sidecar 状态，而不是依赖 Electron、浏览器窗口或模拟在线状态。

**依赖：** 无（可立即开始）。

**状态：** 已验收（自动化通过）。

- [x] Windows 可启动原生应用，并能显示 Sidecar 连接、协议版本和不可用原因。
- [x] 界面与 Sidecar 的请求、事件和错误均经版本化 JSONL 契约验证。
- [x] 生产路径不再启动旧 Electron/WebView 外壳或使用 Mock Agent 回退。
- [x] 原生应用基础、Sidecar 契约和 Windows 烟测均有自动化验证。

**验收与 TDD 证据：** `docs/requirements-alignment/03-original-ten-task-acceptance-and-tdd-baseline.md`、`docs/traceability.md`。
