# 05 - 打开工作区并运行 OpenCode 标准编码会话

**What to build:** 本地开发者可以在 Halo 工作台打开 Git 工作区，并通过已经就绪的 OpenCode Server Adapter 创建、发送和重开标准编码会话；OpenCode 负责 Provider、模型、Session 和 Agent 工具循环，Halo 只投影工作台需要的状态。标准会话不会被误记为受管交付。

**Blocked by:** 07 - 在 Tauri 运行时探测并启动 OpenCode 1.x.

**Status:** ready-for-agent

## 验收标准

- [ ] 用户可以选择、打开、关闭和切换本地 Git 工作区，并看见规范路径、分支与工作树状态；标准模式不要求先授予受管工作区信任。
- [ ] 标准会话通过 OpenCode Adapter 使用真实 `/session`、`/session/:id/prompt_async` 与 `/event` 语义，原生路径和载荷不泄漏到前端。
- [ ] Provider/模型选择来自工单 06 的受管配置与 OpenCode 能力结果；缺失模型、凭据或能力时显示稳定错误和修复建议。
- [ ] OpenCode 原生回复、工具阶段和错误被规范化为工单 04 的有序事件；完整原始工具日志不进入 Halo 诊断。
- [ ] 标准会话历史保存在 Halo 管理的标准 OpenCode profile 中，可以按当前 Git 工作区重新打开；不得与受管任务的临时会话或交付历史混用。
- [ ] 标准会话不生成任务基线、受管任务状态、交付证据、证据新鲜度或接受/拒绝结论。
- [ ] 标准模式保留 OpenCode 原生工作区改动和 BitFun 工作台 Git 能力，不因受管交付策略被全局禁用。

## 验证要求

- Adapter 契约与 Tauri 测试覆盖：打开/切换工作区、创建 Session、首条 Prompt、SSE 回复、错误、重开历史和关闭清理。
- 前端测试覆盖：工作区状态、发送中闸门、回复投影、重开会话，以及不出现受管证据控件。
- 受控 OpenCode 替身用于自动化；至少保留一条真实本机 OpenCode 标准会话烟测作为执行证据，但不得记录完整对话或远程标识。

## 不在本票

- 不创建受管任务、任务基线、一次性决议或交付证据；这些从工单 08 开始。
- 不引入 Pi、BitFun 内置 Code Agent 或前端直连 OpenCode。
