---
status: accepted
---

# 结合自动化契约测试与真实受管会话验收

Halo Studio 的自动化测试覆盖前端行为、Halo Workbench Runtime 契约和受控 Pi RPC 替身；真实 Pi RPC 会话则仅在可安全丢弃的受信任验收工作区中由开发者明确触发验证。真实验收结果只保存脱敏的结论和交付证据，不将真实凭据、完整 session JSONL、原始 extension UI、工具输出或外部执行成本带入持续集成环境。Pi TUI 和历史 OpenCode Server 不属于本验收接口。
