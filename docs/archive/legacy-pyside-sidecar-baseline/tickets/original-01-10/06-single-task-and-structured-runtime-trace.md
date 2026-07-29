# 06 - 单任务与结构化运行轨迹

**构建目标：** 让本地开发者在活动工作区中显式选择 Pi 或 OpenCode 创建一个任务，并在主界面查看有序、结构化的任务阶段、操作请求、验证状态和取消结果。

**依赖：** 04 - Pi 受管运行时；05 - OpenCode 受管运行时。

**状态：** 已验收（自动化通过）。

- [x] 一个活动工作区同一时刻只允许一个运行中的 Agent 任务。
- [x] 任务只接收用户显式提供的说明、选定文件、已有 Diff 和补充信息。
- [x] Pi/OpenCode 原生输出被统一为有序事件和可恢复快照，主界面不以原始终端输出作为主内容。
- [x] 取消先请求原生停止，超时才强制终止，并清楚显示最终状态。

**验收与 TDD 证据：** `docs/requirements-alignment/03-original-ten-task-acceptance-and-tdd-baseline.md`、`docs/traceability.md`。
