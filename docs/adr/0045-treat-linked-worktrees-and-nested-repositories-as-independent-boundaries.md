---
status: accepted
---

# 将 linked worktree 与嵌套仓库视为独立边界

Halo Studio 首期将普通 Git 仓库和 linked worktree 都作为可打开的一等工作区。子模块和其他嵌套仓库拥有独立的工作区、任务范围和信任状态：信任父仓库不会递归授予其执行权限，从而支持常见的多工作树开发流程而不扩大受管执行的安全边界。
