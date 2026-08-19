# 08 - 完成首轮 Pi RPC 受管任务会话

**What to build:** 本地开发者可以在受信任 Git 工作区显式创建受管任务，并通过 P0 唯一生产执行 Adapter——Pi RPC——发送首轮任务消息。Pi 真实回复并发出 `agent_settled` 后任务进入“等待开发者”，同时记录任务基线和脱敏运行轨迹。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；05 - 打开工作区并运行 Pi 标准编码会话.

**Status:** ready-for-agent

## 实现边界

- 受管任务必须绑定受信任 Git 工作区、任务基线和单一 Pi RPC session/config；不得复用标准 session。
- Runtime 只把 Pi command response、message/tool/extension/settled 事件转换为脱敏 Halo 事件；原始 session、entry、toolCall 和命令输出留在 Adapter 内。
- `agent_settled` 是首轮完成门槛；本票不处理决议、追问、显式结束或中断恢复。

## 验收标准

- [ ] 受管交付模式必须由用户显式选择，并要求用户确认规范化真实路径对应的受信任 Git 工作区。
- [ ] 创建任务时记录 HEAD、工作树和已有改动基线，区分任务前与任务期间改动，不要求工作树预先干净。
- [ ] P0 直接使用 `pi-rpc-p0`；UI 不显示未实现的 Code Agent、历史 OpenCode 或多执行器选择器，也不接受任意执行器标识。
- [ ] Halo Runtime 创建隔离的 Pi 受管 session/config，并通过 `prompt` 发送首轮；原始 session/entry 标识只保留在 Adapter 私有实现中。
- [ ] `message_update`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`extension_ui_request`、错误和 `agent_settled` 被规范化为 Halo 有序事件；无关任务事件被隔离。
- [ ] 首轮 Agent 回复和 `agent_settled` 确认后任务进入 `waiting_developer` 而非自动完成；`agent_end`、Pi idle 或 prompt response 本身不得被误判为交付完成。
- [ ] 界面只展示用户消息和经脱敏、限长整理的 Agent 回复与结构化运行轨迹，不展示完整原始工具日志、命令输出或 session JSONL。
- [ ] 凭据、Authorization、Provider 环境、原始 toolCallId、entry ID、完整对话和原始 JSONL 不进入状态、日志、证据或持久化。

## 验证要求

- Adapter/Runtime 契约测试覆盖创建 session、首轮 `prompt`、LF JSONL 事件顺序、`agent_settled`、等待状态、错误、迟到/无关事件和凭据不泄漏。
- Tauri/前端测试覆盖信任门槛、任务创建、发送中闸门、等待开发者和真实错误展示。
- 自动化使用受控 Pi RPC 替身；真实首轮只在工单 14 获得凭据与验收工作区授权后执行。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/store.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-agent-runtime --test workbench_runtime_contracts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-desktop --test halo_workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run check:repo-hygiene
git diff --check
```

## 不在本票

- 不实现历史 OpenCode Server、Halo Studio 内置 Code Agent、多执行器选择或自动交接。
- 不处理操作请求决议、追问、显式结束或交付审查；分别由工单 09–10 完成。
