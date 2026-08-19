---
status: superseded by ADR-0072
related: ADR-0065 deep Workbench Runtime seam; ADR-0072 active Pi RPC P0 adapter
supersedes: 0002-halo-runtime-as-the-single-managed-coding-owner.md execution ownership; 0002-verified-dual-agent-delivery-workflow.md and 0023-use-locally-installed-pi-and-opencode-executors.md for P0 executor scope
---

# 使用 OpenCode Server 作为 P0 唯一受管执行 Adapter（历史决策）

本 ADR 记录曾经选择 OpenCode Server 的 P0 方案。它在文档迁移前曾规定由 Halo Workbench Runtime 启动本机 OpenCode 1.x `serve`，通过回环 HTTP/SSE 复用其 Provider、模型、Session 和 Agent 工具循环；该方案现已由 ADR-0072 取代，不能指导新的 P0 实现。

本段只作为历史比较对象保留：当时的回环端口、认证、HTTP/SSE、Session/Message 脱敏和清理约束不得被移植成新的 P0 传输。新的 P0 约束以 ADR-0072 的 Pi RPC、LF JSONL、extension gate 和临时 session 边界为准。

Halo 从未因本 ADR 获得复制或分叉 OpenCode 内部 Provider、模型注册表、Session 数据库或 Agent 循环源码的授权。`D:\opencode-dev` 和旧 `opencode.rs` 仅保留为历史协议/行为参考；它们不是新的构建依赖、运行时依赖或 P0 生产路径。

Pi 与上游（原 BitFun）内置 Code Agent 当时被排除在 P0 之外；该排除已由 ADR-0072 更新为 Pi RPC 的唯一 P0 生产路径。上游（原 BitFun）内置 Code Agent 和 OpenCode Server 仍不进入当前选择器、配置、测试矩阵或发布门槛。
