# BitFun/Tauri 产品迁移

**Status:** ready-for-agent

本迁移把 Halo Studio 建立为 BitFun 的受控下游产品，并将已验证的受管交付行为迁入 Halo 品牌的 Tauri 工作台。

- [迁移规格](00-bitfun-tauri-product-migration-spec.md)
- [实施工单](issues/)

## 当前检查点

- 工单 01–03A1 已建立可迁移能力基线、受跟踪 BitFun 产品树、Halo Tauri 入口和正式 BitFun Web UI；UI 对齐不等于模型或 Agent 执行链已经接入。
- 新增工单 03B：将 P0 受管执行器从历史 OpenCode Server 决策切换为 Pi RPC，并固定 P0 目标链路。
- 下一项是工单 04：在 Tauri seam 建立深的 Halo Workbench Runtime Module；04 及后续工单均阻塞于 03B。
- P0 只实现本机已安装 Pi 的生产执行 Adapter：Halo Workbench Runtime 受控启动 `pi --mode rpc`，通过 stdin/stdout 严格 LF JSONL 使用 Pi 的 Provider、模型、Session 与 Agent 工具循环。
- `D:\pi-main` 只作为只读协议与行为参考，不复制源码、不建立依赖、不修改该目录；Pi TUI、Unix/CBOR PiServer、HTTP/SSE 与历史 OpenCode Server 不属于 Windows P0 生产路径。

## 执行依赖

`03A1 → 03B → 04 → 06 → 07 → 05 → 08 → 09 → 10 → 11 → 12 → 14 → 15`

工单 13 依赖 03B 和 04，并可与 06–12 的实现并行；工单 14 同时受 12 和 13 阻断。编号表达需求来源，`Blocked by` 才是实际执行顺序。04 是新的实现起点；未完成 03B 时不得开始 04。

## 旧六票策略

GitHub #9–#14 保持原始需求、状态和历史验收证据，不因执行器切换改写、重开或关闭。它们只作为可迁移能力基线；工单 12 负责把这些行为前向映射到新的 Pi RPC-backed Tauri 产品，工单 14/15 才负责真实 UI 与最终发布验收。

工单 07–15 中出现的 OpenCode Server、`opencode serve`、HTTP/SSE、Basic Auth 和 OpenCode Server Adapter 只可作为历史比较对象或已废弃决策出现，不能作为 P0 生产路径。当前 issue-04 worktree 中未提交的 OpenCode 实现不改变本规格；后续实现者须按 03B 将可迁移语义移植到 Pi RPC，或在经过审计后废弃，不得直接合并为 P0。Pi extension 的安装、版本、依赖、权限和许可证核对由 13 负责，工具请求的第一方阻断由 09 负责。

任何阻断未完成时不得跳到真实 UI 验收或旧产品收缩。
