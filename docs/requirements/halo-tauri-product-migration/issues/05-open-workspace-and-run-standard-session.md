# 05 - 打开工作区并运行 Pi 标准编码会话

**What to build:** 本地开发者可以在 Halo 工作台打开 Git 工作区，并通过已经就绪的 Pi RPC Adapter 创建、发送和重开标准编码会话；Pi 负责 Provider、模型、Session 和 Agent 工具循环，Halo 只投影工作台需要的状态。标准会话不会被误记为受管交付。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；04 - 建立 Halo Workbench Runtime 公共契约；06 - 管理 Pi Provider、模型与系统凭据；07 - 在 Tauri 运行时探测并启动 Pi RPC.

**Status:** ready-for-agent

## 实现边界

- Halo Workbench Runtime 负责工作区生命周期、标准/受管模式隔离和公开事件；Pi RPC Adapter 私有持有原始 session/entry 标识。
- 标准模式只使用可持久 Pi session 和有界历史，不创建受管任务、基线、决议或交付证据。
- Renderer 只使用固定 IPC；不接收 Pi 子进程 stdin/stdout、模型认证、原始工具输出或 session 文件路径。

## 验收标准

- [ ] 用户可以选择、打开、关闭和切换本地 Git 工作区，并看见规范路径、分支与工作树状态；标准模式不要求先授予受管工作区信任。
- [ ] 标准会话通过 Pi RPC 使用真实 `prompt`、`get_state`、`get_entries` 和 `message_update`/tool 事件语义，原生载荷不泄漏到前端。
- [ ] Provider/模型选择来自工单 06 的受管配置与 Pi 能力结果；缺失模型、凭据或能力时显示稳定错误和修复建议。
- [ ] Pi 原生回复、工具阶段和错误被规范化为工单 04 的有序事件；完整原始工具日志不进入 Halo 诊断。
- [ ] 标准会话历史保存在 Halo 管理的标准 Pi session 目录中，可以按当前 Git 工作区重新打开；不得与受管任务的临时会话或交付历史混用。
- [ ] 标准会话不生成任务基线、受管任务状态、交付证据、证据新鲜度或接受/拒绝结论。
- [ ] 标准模式保留 Pi 原生工作区改动和 Halo Studio 工作台 Git 能力，不因受管交付策略被全局禁用。

## 验证要求

- Adapter 契约与 Tauri 测试覆盖：打开/切换工作区、创建 RPC session、首条 prompt、message/tool 事件、错误、重开有界历史和关闭清理。
- 前端测试覆盖：工作区状态、发送中闸门、回复投影、重开会话，以及不出现受管证据控件。
- 受控 Pi RPC 替身用于自动化；真实本机 Pi 标准会话属于工单 14 授权范围，不在本票发送真实模型请求。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/store.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-agent-runtime --test workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run desktop:build:fast
git diff --check
```

## 不在本票

- 不创建受管任务、任务基线、一次性决议或交付证据；这些从工单 08 开始。
- 不引入历史 OpenCode Server、HTTP/SSE、Halo Studio 内置 Code Agent 或前端直连 Pi。
