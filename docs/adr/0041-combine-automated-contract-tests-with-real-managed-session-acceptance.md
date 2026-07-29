---
status: accepted
---

# 结合自动化契约测试与真实受管会话验收

Halo Studio 的 CI 覆盖前端行为、BitFun Runtime 契约和模拟受管执行器；真实 Code Agent、Pi 或 OpenCode 会话则仅在可安全丢弃的受信任验收工作区中由开发者明确触发验证。真实验收结果只保存脱敏的结论和交付证据，不将真实凭据、完整会话或外部执行成本带入持续集成环境。
