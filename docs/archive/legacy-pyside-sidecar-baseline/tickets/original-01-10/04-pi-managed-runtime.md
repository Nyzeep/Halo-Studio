# 04 - Pi 受管运行时

**构建目标：** 让受信任工作区中的 Pi 被真实探测、以 RPC 模式受管启动、完成 `get_state` 就绪检查，并将启动、停止、异常与取消状态呈现给本地开发者。

**依赖：** 02 - 可信单工作区生命周期；03 - 受管启动配置与凭据隔离。

**状态：** 条件验收（受控 Pi 协议进程通过；本机未发现真实 Pi，发布前资格验证待执行）。

- [ ] 真实安装版 Pi 的版本探测和 `get_state` 就绪检查待在安装 Pi 的 Windows 环境执行。
- [x] EOF、无效协议、异常退出、停止与取消均产生真实且可恢复的状态。
- [x] Pi 启动配置不通过 UI 或公开 IPC 暴露 Provider 凭据。
- [x] 受控 Pi 进程集成测试覆盖分帧、乱序响应、就绪失败和停止行为。

**验收与 TDD 证据：** `docs/requirements-alignment/03-original-ten-task-acceptance-and-tdd-baseline.md`、`docs/traceability.md`。
