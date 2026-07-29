---
status: accepted
---

# 将 Git 远程认证委托给原生凭据提供方

Halo Studio 的克隆、fetch/pull 和远程管理操作委托系统 Git Credential Manager 或原生浏览器认证处理。应用不读取、保存或转发 Git Token，仅接收完成操作所需的脱敏成功、失败和状态结果；Git 网络凭据不进入 Halo Studio 的配置档、IPC、日志、历史或受管任务上下文。
