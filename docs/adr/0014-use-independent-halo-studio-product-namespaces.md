---
status: accepted
---

# 使用独立的 Halo Studio 产品命名空间

Halo Studio 的应用标识、配置目录、系统凭据库服务名、日志命名空间、IPC 与协议名均使用独立的 Halo Studio 命名空间，不复用 Halo Studio 的产品身份或本地数据边界。首期不自动导入 Halo Studio 的配置、会话、模型设置或凭据，以避免将不受 Halo Studio 控制的历史数据、留存策略和凭据引用带入产品；需要迁移时另行设计显式、可预览的迁移流程。
