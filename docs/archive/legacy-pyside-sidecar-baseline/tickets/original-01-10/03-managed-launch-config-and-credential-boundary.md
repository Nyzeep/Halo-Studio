# 03 - 受管启动配置与凭据隔离

**构建目标：** 让本地开发者建立运行任务所需的最小受管启动配置，并把模型、思考级别、Provider 凭据引用和启动选项安全地交给 Pi/OpenCode，而不让界面或本地历史接触凭据明文。

**依赖：** 01 - 原生工作台最小闭环；02 - 可信单工作区生命周期。

**状态：** 条件验收（失败关闭已验证；真实 Credential Manager 正向注入待交互式会话资格验证）。

- [x] 用户可选择受管启动配置，但 UI、IPC、日志、Diff、备份和数据库不暴露凭据明文。
- [x] 操作系统保护的凭据存储不可用时，保存和启动都会失败关闭，不回退到明文文件。
- [x] 运行时只获得显式白名单环境，不能继承宿主进程的任意敏感变量。
- [ ] 凭据 canary 的真实正向注入待可写的 Windows Credential Manager 会话；当前环境已自动验证失败关闭和不回显明文。

**验收与 TDD 证据：** `docs/requirements-alignment/03-original-ten-task-acceptance-and-tdd-baseline.md`、`docs/traceability.md`。
