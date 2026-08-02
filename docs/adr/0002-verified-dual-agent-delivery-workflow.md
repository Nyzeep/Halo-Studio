---
status: superseded by ADR-0072 for P0 executor scope
---

# 可验证双 Agent 编码交付工作流

在核心安全边界完成后，本文曾以 Pi/OpenCode 双执行器设想描述首个用户可用纵切。该记录保留为历史产品工作流背景；当前 P0 的唯一执行器、权限门控和交付链以 ADR-0072 的 Pi RPC 决策为准。

## Considered Options

- 扩展为 BitFun 式通用 AI 应用平台，覆盖办公、Mini App、远程与多端协作。
- 在 Halo 内实现自动委派、自动重试和跨 Agent 故障转移。
- 将 Agent 文件写入、Git 提交和验证命令统一代理到 Halo。

这些选项都会扩大权限和兼容性边界，并掩盖 Pi/OpenCode 的原生语义，因此不进入首个交付工作流。

ADR-0072 进一步取消 P0 双 Agent 范围：当前首个纵切只通过 Pi RPC Adapter 交付；历史 OpenCode Server、BitFun 内置 Code Agent 和跨执行器交接留待独立决策。
