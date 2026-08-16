# 14 - 完成真实 Pi RPC 原生 UI 验收

**What to build:** 发布负责人可以在可删除独立 Git 工作区、系统凭据存储、本机 Pi 和 Halo 原生 Tauri UI 中完成一条真实受管任务主链，证明 Halo 已复用 Pi 的 Provider/模型/Session/Agent 执行和第一方 extension 权限门控，同时留下足以放行但不泄露敏感信息的验收结论。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；12 - 证明旧六票到 Pi RPC Tauri 产品的行为等价迁移；13 - 演练 BitFun 上游同步、Pi extension 依赖与许可证门槛。

**Status:** ready-for-agent

## 实现边界

- 本票是唯一允许真实 Pi 模型请求的真实验收票；必须使用可删除 Git 工作区、明确的系统凭据和交互式非受限 Windows 宿主。
- 验收只观察 Halo 原生 Tauri UI、Runtime 脱敏事件、Git 前后事实和进程清理；不保存原始 Pi RPC/session/extension 数据。
- 任何外部写入必须在验收前单独授权；接受/拒绝不触发 Git 写入，未执行或无法脱敏的步骤直接阻断 P0。
- 上游 UI 参考（流程补充，非验收证据）：可只读使用独立克隆 `D:\Halo Studio\BitFun-latest`（`https://github.com/GCWing/BitFun.git` 的 `main`）作为最新 UI 布局参考；仅借鉴布局与交互，不复制其目录结构、配置、凭据或构建依赖。该目录不参与构建、不作为验收工作区，也不构成放行证据；需要最新参考时先在该克隆中 fetch 并 fast-forward。若采纳整树 UI 同步，须另立跟进工单并重跑 halo-scope 守卫。

## 验收前授权

- 用户必须明确确认可删除验收工作区、真实 `credential_ref`、Provider/模型选择和任何工作区外写入；验收负责人必须确认已安装 Pi 版本和第一方 extension 审计清单。
- 不在聊天、命令回显、截图、日志、证据或文档中记录密钥、完整 Base URL 凭据、完整对话、Authorization、Pi session/entry/toolCall 标识、原始 JSONL 或命令输出。

## 验收清单

- [ ] 从正式 Halo Tauri 入口打开并信任验收 Git 工作区，确认 UI 为已对齐的 BitFun 工作台而非旧 QML、Electron 或静态页面。
- [ ] 在受控环境先执行 `where.exe pi`、`Get-Command pi -All`、`pi --version`，记录真实版本、`pi-rpc-p0` 档案和第一方 extension 版本/hash；不把命令回显中的敏感环境变量保存为证据。
- [ ] UI 显示真实 Pi 版本、Pi RPC 能力结果、Provider/模型就绪状态和失败关闭行为；不暴露 config/session 路径、凭据、原始 ID 或 JSONL。
- [ ] 创建受管任务并完成首轮 `prompt`/回复，收到 `agent_settled` 后状态进入 `waiting_developer`，不会因 Pi idle、`agent_end` 或 prompt response 自动完成。
- [ ] 触发并处理至少一个真实的第一方 extension tool gate；确认 `tool_call` 在工具执行前阻断，allow/deny 只针对当前脱敏 `toolCallId`，超时/deny 不执行工具，匹配 response 后 UI 才移除请求。
- [ ] 在同一 RPC session 中发送 `follow_up`，并让 Pi 在验收工作区产生一项可安全丢弃的无害文件改动。
- [ ] 用户显式结束会话，在只读审查中核对 Diff、摘要、归因、验证结论和证据新鲜度，并自行选择接受或拒绝。
- [ ] 验证接受/拒绝不会自动暂存、提交、推送、回滚、删文件、建分支或改写历史。
- [ ] 另建运行，在进行中关闭应用或 Pi 子进程，重启后确认任务中断、无自动重连、无重发、无请求重放和无重复写入。
- [ ] 退出后确认没有残留 Pi 子进程、RPC 句柄、临时认证材料或受管 session/config；标准会话历史不受影响。

## 验收证据

- 只保存 Halo/Pi 公开版本、兼容性档案、每个清单项的通过/失败/未执行、脱敏截图、Git 前后事实和进程清理结论。
- 任何无法可靠观察或尚未执行的项目必须标为未完成；自动化、HTTP smoke 或受控替身不得替代真实原生 UI 结论。

## 不在本票

- 不测试历史 OpenCode Server、Pi TUI、Unix/CBOR PiServer、BitFun 内置 Code Agent 或未批准的真实外部写入。
- 不提交、推送、关闭 GitHub issue 或更改 Git 历史。

## 精确验证命令与真实验收流程

在交互式、非受限 Windows 宿主执行；本票是唯一允许发送真实 Pi 模型请求的工单，其他工单只能使用替身或凭据盲探测。

```powershell
$ErrorActionPreference = "Stop"
where.exe pi
Get-Command pi -All | Select-Object Name,CommandType,Source,Path
pi --version
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio" run product:check
pnpm --dir "product/Halo Studio" run product:test
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run desktop:build:fast
pnpm --dir "product/Halo Studio" run e2e:test:smoke
git diff --check
```

真实流程必须按以下顺序执行，不能用自动化 smoke、手工启动 `pi --mode rpc` 或 HTTP 检查替代：

1. 在仓库根目录创建独立、可删除的验收工作区，记录其规范化路径和初始 `git status --short`；只向该目录授权写入。不得在 Halo 源码树或用户日常工作区发送真实请求。
2. 通过上面的 preflight 命令记录 Pi 版本、`pi-rpc-p0` 能力档案、第一方 extension ID/version/hash 和验证命令退出码；不要保存命令回显中的环境变量、路径中的用户名或完整进程命令行。
3. 使用 `pnpm --dir "product/Halo Studio" run desktop:dev` 从正式 Tauri 入口启动 Halo；仅在 Halo 原生 UI 中打开并信任验收工作区，确认运行时状态来自 Workbench Runtime。Halo 负责启动受控 `pi --mode rpc`；验收人员不直接启动 Pi RPC。
4. 在 UI 中创建受管任务并发送首轮 prompt，观察 `agent_settled` 后进入 `waiting_developer`；记录脱敏状态和事件摘要，不记录原始 JSONL、session/entry/toolCall ID、凭据或完整回复。
5. 触发一个需要决议的工具调用，确认 Pi 的 `tool_call` 在执行前停住；只对当前脱敏 `toolCallId` 作一次 allow/deny。验证 deny、超时和协议/extension 错误都不执行工具，且只有匹配的 `extension_ui_response` 到达后 UI 请求才消失。
6. 在同一任务中发送一次 `follow_up`，只允许 Pi 在验收工作区产生一个可安全丢弃的无害文件变化；保存 `git diff --stat`、文件路径摘要和脱敏验证结果，不保存命令输出或原始工具参数。
7. 点击“结束并审查”，在只读界面核对 Diff、摘要、归因、验证和证据新鲜度，再选择接受或拒绝；随后用 `git status --short` 证明没有自动暂存、提交、推送、回滚、删除或建分支。
8. 另开一次受管任务，在 prompt、extension 等待或 follow-up 期间关闭 Halo 或其受控 Pi 子进程；重启后确认状态为 `interrupted`，没有自动重连、重发、请求重放或重复写入。只记录由 Halo 绑定的进程清理结果，不记录 PID 或命令行。
9. 通过 Halo 的停止/退出流程清理受管 session/config 和临时 extension 文件；确认绑定的 Pi 子进程已退出、没有 RPC 句柄或临时认证材料，且标准会话历史未被删除。验收工作区在收集 `git status --short` 和脱敏摘要后删除，不把原始工作区内容作为证据保存。

验收记录只能包含 `pass`、`fail` 或 `not-run`、脱敏截图、Git 前后摘要、公开版本/能力档案和清理结论。任何 `not-run`、Pi RPC framing/extension gate 失败、证据脱敏失败、非授权外部写入或残留进程都阻止 P0 放行。
