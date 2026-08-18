---
status: accepted
---

# 将工单 15b 的 OpenCode 全量移除延后至 DeepSeek Harness 基座迁移

工单 15a 已通过独立收缩变更移除根级旧产品实现（PySide/QML、Sidecar、Electron、根 TS workspace）。产品树内 `halo-opencode-adapter` 仍经 `halo-core` 的 `product-full` 无条件编译，并接入 external-sources、external-hooks、plugin-runtime、ai-adapters 订阅认证与 ACP 内置客户端五条链路；完整移除将触及这些模块及其大量测试，属于跨产品的大重构。

用户计划以 DeepSeek Harness（`dsh`，MIT，everything-is-a-plugin/Cordis 架构，developer preview，TypeScript/pnpm workspace）作为后续新基座。为避免在即将被替换的 Halo Studio 派生树上投入沉没成本，本决策将 OpenCode 全量移除延后至基座迁移：过渡期 `opencode-adapter` 保持 workspace member 与 `product-full` 编译，冻结新增 OpenCode 功能与引用；迁移完成后在新基座上按工单 15 最终扫描删除 OpenCode 相关实现与引用。迁移时复用以下审计清单。

## 迁移清单（15b）

1. 移除 workspace member `src/crates/adapters/opencode-adapter` 与 `halo-core` 的 `dep:halo-opencode-adapter`（`product-full`）。
2. `assembly/core/external_sources.rs`：移除 `halo_opencode_adapter` 导入与 OpenCode commands/subagents/MCP/tools 生态路径（含相关测试）。
3. `assembly/core/external_hooks.rs`：移除 `OpenCodeHookProvider`。
4. `assembly/core/plugin_runtime.rs`：移除 `load_opencode_package_adapter` 与 `.opencode/plugins` 路径。
5. `ai-adapters/subscription_auth/opencode.rs` 与 `mod.rs`：移除 `SubscriptionProvider::Opencode` 设备登录与刷新。
6. `interfaces/acp/builtin_clients.rs`：移除内置 `opencode` 客户端。
7. 清理 `external_mcp_import.rs`、`external_tools.rs`、`external_subagents.rs`、`skills/registry.rs` 中的 OpenCode 生态引用与测试。
8. 执行工单 15 完整复验（构建/打包、Rust/前端契约、Pi RPC Adapter 集成、extension 审计、e2e、行为等价矩阵、真实 UI 主链）。
