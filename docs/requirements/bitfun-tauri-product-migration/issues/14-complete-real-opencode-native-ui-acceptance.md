# 14 - 完成真实 OpenCode 原生 UI 验收

**What to build:** 发布负责人可以在可删除独立 Git 工作区、系统凭据存储、本机 OpenCode 1.x 和 Halo 原生 Tauri UI 中完成一条真实受管任务主链，证明 Halo 已复用 OpenCode 的 Provider/模型/Session/Agent 执行，同时留下足以放行但不泄露敏感信息的验收结论。

**Blocked by:** 12 - 证明旧六票到 OpenCode Tauri 产品的行为等价迁移；13 - 演练 BitFun 上游同步与许可证门槛.

**Status:** ready-for-agent

## 验收前授权

- 用户必须明确确认可删除验收工作区、真实 `credential_ref`、Provider/模型选择和任何工作区外写入。
- 不在聊天、命令回显、截图、日志、证据或文档中记录密钥、完整 Base URL 凭据、完整对话、端口、Authorization、OpenCode Session/Message/Request 标识。

## 验收清单

- [ ] 从正式 Halo Tauri 入口打开并信任验收 Git 工作区，确认 UI 为已对齐的 BitFun 工作台而非旧 QML 或静态页面。
- [ ] UI 显示真实 OpenCode 1.x 版本、`opencode-server-1.x` 兼容性结果、Provider/模型就绪状态和失败关闭行为；不暴露连接细节。
- [ ] 创建受管任务并完成首轮 Prompt/回复，状态进入 `waiting_developer`，不会因 OpenCode idle 自动完成。
- [ ] 触发并处理至少一个真实的一次性 permission 或 question；确认不存在永久放行，且原生确认后 UI 才移除请求。
- [ ] 在同一 Session 中发送追问，并让 OpenCode 在验收工作区产生一项可安全丢弃的无害文件改动。
- [ ] 用户显式结束会话，在只读审查中核对 Diff、摘要、归因、验证结论和证据新鲜度，并自行选择接受或拒绝。
- [ ] 验证接受/拒绝不会自动暂存、提交、推送、回滚、删文件、建分支或改写历史。
- [ ] 另建运行，在进行中关闭应用或 OpenCode，重启后确认任务中断、无自动重连、无重发、无请求重放和无重复写入。
- [ ] 退出后确认没有残留 OpenCode 子进程、回环监听、临时认证材料或受管 profile；标准会话历史不受影响。

## 验收证据

- 只保存 Halo/OpenCode 公开版本、兼容性档案、每个清单项的通过/失败/未执行、脱敏截图、Git 前后事实和进程清理结论。
- 任何无法可靠观察或尚未执行的项目必须标为未完成；自动化、HTTP smoke 或受控替身不得替代真实原生 UI 结论。

## 不在本票

- 不测试 Pi、BitFun 内置 Code Agent、OpenCode 2.x 或未批准的真实外部写入。
- 不提交、推送、关闭 GitHub issue 或更改 Git 历史。
