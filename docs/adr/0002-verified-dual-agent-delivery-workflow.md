---
status: superseded by ADR-0071 for P0
---

# 可验证双 Agent 编码交付工作流

在核心安全边界完成后，Halo Studio 将面向个人本地开发者，以受信任 Git 工作区中的 Pi/OpenCode 显式任务、Git 增量证据、用户审查和手动交接作为首个用户可用纵切。该选择优先提供可解释、可恢复的编码交付，而不复制通用 AI 平台的自动编排、任意 Shell、Git 自动写入、远程协作或办公生态；它保留两种受管应用的原生权限与写入语义，并让接受结论和 Git/发布操作保持分离。

## Considered Options

- 扩展为 BitFun 式通用 AI 应用平台，覆盖办公、Mini App、远程与多端协作。
- 在 Halo 内实现自动委派、自动重试和跨 Agent 故障转移。
- 将 Agent 文件写入、Git 提交和验证命令统一代理到 Halo。

这些选项都会扩大权限和兼容性边界，并掩盖 Pi/OpenCode 的原生语义，因此不进入首个交付工作流。

ADR-0071 取消 P0 双 Agent 范围：当前首个纵切只通过 OpenCode Server Adapter 交付；Pi 与执行器交接留待 P0 之后重新决策。
