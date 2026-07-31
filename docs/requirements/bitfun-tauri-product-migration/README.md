# BitFun/Tauri 产品迁移

**Status:** ready-for-agent

本迁移把 Halo Studio 建立为 BitFun 的受控下游产品，并将已验证的受管交付行为迁入 Halo 品牌的 Tauri 工作台。

- [迁移规格](00-bitfun-tauri-product-migration-spec.md)
- [实施工单](issues/)

## 当前检查点

- 工单 01–03A1 已建立可迁移能力基线、受跟踪 BitFun 产品树、Halo Tauri 入口和正式 BitFun Web UI；UI 对齐不等于模型或 Agent 执行链已经接入。
- 下一项是工单 04：在 Tauri seam 建立深的 Halo Workbench Runtime Module。
- P0 只实现本机 OpenCode 1.x 的生产执行 Adapter。Halo 复用 OpenCode Server 的 Provider、模型、Session 与 Agent 循环，不复制 `D:\opencode-dev` 内部源码。

## 执行依赖

`04 → 06 → 07 → 05 → 08 → 09 → 10 → 11 → 12 → 14 → 15`

工单 13 在工单 04 完成后可以与 06–12 并行，但工单 14 同时受 12 和 13 阻断。编号表达需求来源，`Blocked by` 才是实际执行顺序。

## 旧六票策略

GitHub #9–#14 保持原始需求、状态和历史验收证据，不因 OpenCode P0 决策改写、重开或关闭。它们只作为可迁移能力基线；工单 12 负责把这些行为前向映射到新的 OpenCode-backed Tauri 产品，工单 14/15 才负责真实 UI 与最终发布验收。

任何阻断未完成时不得跳到真实 UI 验收或旧产品收缩。
