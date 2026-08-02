# 09 - 通过第一方 Pi extension 处理一次性操作请求

**What to build:** 当 Pi 受管 session 在工具执行前通过 Halo 第一方 extension 发出决议请求时，本地开发者可以在 Halo 原生 UI 对准确的当前工具请求作出一次性 allow/deny；Halo 只在 Pi extension 收到匹配 response 后允许工具继续。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；08 - 完成首轮 Pi RPC 受管任务会话.

**Status:** ready-for-agent

## 实现边界

- 第一方 Pi extension 是唯一 P0 工具执行前 gate；Pi 默认行为、项目 trust、TUI 或原生 settings 不构成 Halo 授权。
- Runtime 只保存 task-scoped 的脱敏 request/toolCall 关联和一次性决议事实；不保存原始参数、答案、extension UI payload 或永久授权。
- extension 与 Runtime 必须把所有异常收敛到阻止工具和可解释状态，不能以 UI 超时或 transport 成功推断 allow。

## 验收标准

- [ ] Halo 第一方 extension 使用 `tool_call` 在 Pi 工具执行前拦截，读取工具名、参数和当前任务上下文；普通 Pi 默认行为和项目 trust 不得替代该门控。
- [ ] 需要决议的请求通过 `extension_ui_request` 发出，Halo 只展示脱敏摘要；响应必须是匹配 request ID 的 `extension_ui_response`。
- [ ] 每个请求绑定一个 Halo task、一次 turn 和一个脱敏 `toolCallId`；决议只能“本次允许”或“拒绝”，不得形成永久、会话级或跨任务授权。
- [ ] P0 Pi RPC extension 只支持当前工具请求的一次性 allow/deny；文本澄清、跨请求回答和永久授权不属于本票的生产能力，不能从通用 Runtime `Question` 类型推断为已实现。
- [ ] 浏览器或 Computer Use 对工作区外产生写入、提交、上传、下载、剪贴板写入、进程或系统控制影响时，必须进入高风险决议；P0 不因 Pi 原生工具名而自动放行。
- [ ] 请求在决议提交期间不可重复响应；过期、错任务、错 turn、错 toolCallId、错种类、重复或跨任务复用均 fail closed，不得批准另一请求。
- [ ] UI 请求只在 Pi extension 接收并返回匹配 response 后消失；IPC/JSONL 成功但无 extension 确认不能伪造已解决。
- [ ] deny、超时、协议错误、无效 response、`extension_error`、extension 崩溃、应用关闭、任务取消或 RPC 断开均阻止工具并按事实收口，不在重启后自动重放或自动批准。
- [ ] extension 安装使用 `--no-extensions` 加显式 `-e`，只加载工单 13 审计通过的 Halo 第一方版本；项目/全局 extension、Pi package 和 Provider extension 不得参与 P0。

## 验证要求

- 自动化覆盖 allow、deny、超时、无效 response、ID 不匹配、过期、重复、错关联、extension_error、迟到确认、取消竞态和重启不重放；文本回答保持明确的 P0 范围外结论。
- 脱敏测试使用包含伪 Authorization、路径、原始 session/entry ID、原始 toolCallId 和答案的载荷，断言公开 Interface 与持久化均不泄漏。
- 前端只能呈现 P0 支持的一次性 allow/deny 控件，不显示“始终允许”、永久授权、文本回答或通用批准对话框。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run check:repo-hygiene
git diff --check
```

## 不在本票

- 不修改 Pi 全局权限、项目 trust、`auth.json` 或 extension discovery 配置，不建立 Halo 永久授权规则。
- 不自动替用户批准任何工作区外写入；不声称 Pi 或 extension 天然安全，extension 仍继承启动用户权限。
