---
status: accepted
---

# 在 Tauri seam 中建立深的 Halo Workbench Runtime Module

Halo Studio 前端通过 BitFun Desktop/Tauri seam 中的 Halo Workbench Runtime Module 获取工作区快照、标准会话、受管任务、Git 操作、配置就绪状态和结构化事件流，而不是分别直连大量 BitFun `api/*` 命令或旧 Halo Sidecar。该 Module 的 Interface 对前端保持小而稳定；P0 OpenCode Server Adapter、文件租约、信任、证据、凭据与策略协调封装在其实现内，维持 Halo 运行事实的单一权威和高测试杠杆。P0 执行器范围由 ADR-0071 收窄。
