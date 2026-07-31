# 08 - 完成首轮 OpenCode 受管任务会话

**What to build:** 本地开发者可以在受信任 Git 工作区显式创建受管任务，并通过 P0 唯一生产执行 Adapter——OpenCode 1.x——发送首轮任务消息。OpenCode 真实回复后任务进入“等待开发者”，同时记录任务基线和脱敏运行轨迹。

**Blocked by:** 05 - 打开工作区并运行 OpenCode 标准编码会话.

**Status:** ready-for-agent

## 验收标准

- [ ] 受管交付模式必须由用户显式选择，并要求用户确认规范化真实路径对应的受信任 Git 工作区。
- [ ] 创建任务时记录 HEAD、工作树和已有改动基线，区分任务前与任务期间改动，不要求工作树预先干净。
- [ ] P0 直接使用 `opencode-server-1.x`；UI 不显示未实现的 Code Agent/Pi 选择器，也不接受任意执行器标识。
- [ ] Halo Runtime 创建隔离的 OpenCode 受管 Session，并通过 `/session/:id/prompt_async` 发送首轮；Session/Message 标识只保留在 Adapter 私有实现中。
- [ ] `/event` 中与该 Session 相关的消息、工具阶段、权限/澄清和错误被规范化为 Halo 有序事件；无关 Session 事件被隔离。
- [ ] 首轮 Agent 回复确认后任务进入 `waiting_developer` 而非自动完成；OpenCode idle 或 prompt 返回本身不得被误判为交付完成。
- [ ] 界面只展示用户消息和经脱敏、限长整理的 Agent 回复与结构化运行轨迹，不展示完整原始工具日志。
- [ ] 凭据、Authorization、端口、Base URL 敏感查询、原始远程标识和完整对话不进入状态、日志、证据或持久化。

## 验证要求

- Adapter/Runtime 契约测试覆盖创建 Session、首轮 Prompt、SSE 顺序、回复确认、等待状态、错误、迟到/无关事件和凭据不泄漏。
- Tauri/前端测试覆盖信任门槛、任务创建、发送中闸门、等待开发者和真实错误展示。
- 自动化使用受控 OpenCode 替身；真实首轮只在工单 14 获得凭据与验收工作区授权后执行。

## 不在本票

- 不实现 Pi、BitFun 内置 Code Agent、多执行器选择或自动交接。
- 不处理操作请求决议、追问、显式结束或交付审查；分别由工单 09–10 完成。
