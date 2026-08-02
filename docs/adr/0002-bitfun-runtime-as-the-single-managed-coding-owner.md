---
status: superseded by ADR-0072 for P0 execution ownership
---

# 由 BitFun Runtime 作为受管编码的单一权威运行时

Halo Studio 以 BitFun Runtime 作为编码会话、工具执行和事件事实的唯一所有者，并将现有 Halo Sidecar 的受管工作区、任务基线、凭据边界、证据审查与人工交接语义迁入该运行时及其产品能力。不会保留并行的权威 Sidecar 会话或只靠界面桥接的双状态实现；这保证会话、权限、文件归属和审查证据各有唯一事实来源，但后续维护必须完成语义迁移。

ADR-0072 调整了 P0 所有权：Pi 拥有 Provider、模型、原生 Session 与 Agent 工具循环；Halo Workbench Runtime 拥有工作区信任、受管任务状态、决议、证据和生命周期投影。仍不允许旧 Sidecar、ACP、Pi TUI 或前端形成并行权威。
