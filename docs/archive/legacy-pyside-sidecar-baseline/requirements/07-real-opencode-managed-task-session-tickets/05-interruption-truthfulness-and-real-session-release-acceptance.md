# 05 - 中断如实化与真实会话发布验收

**What to build:** 受管任务会话在应用或 Sidecar 意外退出后如实显示为中断且不会自动重放；项目同时具备可重复的自动化证据和开发者可执行的真实会话验收清单，作为 P0 发布门槛。

**Blocked by:** 04 - 追问、显式结束与交付审查.

**Status:** ready-for-agent

**实现说明：** 自动化实现与测试见 `sidecar/crates/halo-sidecar/src/main.rs`、`sidecar/crates/halo-sidecar/src/task_flow.rs` 和 `sidecar/crates/halo-integration-tests/tests/interruption.rs`；真实 OpenCode 原生验收仍须按 [发布验收清单](05-real-opencode-release-acceptance-checklist.md) 由开发者执行。

- [x] 应用或 Sidecar 重启后，未结束任务被标为中断；不会自动重连远程会话、重发消息或重复 Agent 写入。
- [x] 中断后的活动会话记录按隐私边界清除，已产生的工作区改动和可审查证据仍按事实保留。
- [x] 自动化测试覆盖中断、无自动恢复、消息不持久化、凭据不泄漏和 Git 不变性。
- [ ] 发布验收清单要求开发者在验收工作区通过原生界面完成真实 OpenCode 启动、首轮消息、一次性请求处理、追问、无害改动、显式结束和审查。
- [ ] 真实验收记录只包含 OpenCode 版本、兼容性档案结果、脱敏后的任务结论和交付证据，不包含密钥或完整对话。
- [x] 新增 OpenCode 1.x 兼容范围时，必须补充对应自动化协议证据和真实会话验收记录；2.x 不能沿用本票据的 1.x 假设自动放行。
