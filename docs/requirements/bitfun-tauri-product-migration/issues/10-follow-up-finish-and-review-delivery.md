# 10 - 追问、显式结束并审查 OpenCode 交付

**What to build:** 本地开发者可以在同一 OpenCode 受管 Session 中继续追问，让 OpenCode 产生真实工作区改动，随后显式结束 Halo 受管会话并在只读交付审查中接受或拒绝结果；Halo 不代替用户执行 Git 交付操作。

**Blocked by:** 09 - 映射 OpenCode 一次性操作请求.

**Status:** ready-for-agent

## 验收标准

- [ ] `waiting_developer` 允许向同一 Adapter 私有 Session 发送追问；发送中、等待操作请求或结束中不可并发重复提交。
- [ ] 每轮通过 OpenCode `prompt_async` 和事件流确认真实回复；不得创建第二个 Session、重发上一轮或依赖原始 Message 标识进入前端。
- [ ] OpenCode 按自身原生权限模型直接产生工作区改动；Halo 以任务基线和当前 Git 事实记录关联改动，并保留用户已有修改与人工介入归因。
- [ ] OpenCode 回合结束只进入 `waiting_developer`；只有用户显式“结束并审查”才关闭逻辑会话、固定最新 Diff、摘要、验证结果和运行结论。
- [ ] 显式结束不是 `abort`：它先停止接受新 Prompt，再有序释放 Adapter Session；任务取消仍走工单 11 的原生 abort。
- [ ] 只读审查展示 Diff、摘要、归因、验证结果和证据新鲜度；后续文件变化使相关证据过期但不删除旧版本。
- [ ] 接受或拒绝只记录交付结论，不自动暂存、提交、推送、回滚、删文件、建分支或改写 Git 历史。
- [ ] Halo 交付历史不保存 OpenCode 完整对话、原始工具日志、凭据或远程 Session/Message 标识。

## 验证要求

- 集成测试覆盖同 Session 追问、并发闸门、无重发、无害文件改动、显式结束、证据版本、新鲜度和 Git 不变性。
- 前端端到端测试覆盖追问、等待、结束、只读 Diff、接受/拒绝，并断言没有编辑器写入或自动 Git 操作。
- 随机 canary 必须跨消息、事件、Diff、摘要、日志和历史证明脱敏边界。

## 不在本票

- 不实现跨执行器交接或多执行器选择。
- 不把 OpenCode 自身 idle、TaskDone 或进程退出直接映射为用户接受的交付。
