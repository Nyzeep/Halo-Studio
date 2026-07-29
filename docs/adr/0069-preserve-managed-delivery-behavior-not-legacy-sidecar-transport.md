---
status: accepted
---

# 迁移受管交付行为而不兼容旧 Sidecar 传输

Halo Studio 将现有可迁移能力基线中的任务状态、操作请求、证据、审查和中断语义迁入 Halo Workbench Runtime Module，但不要求新的 Tauri command/event 接口兼容旧 `stdio JSONL v1`。旧协议只作为迁移行为对照，并在目标接口和真实桌面路径达到行为等价后随 Python/QML Sidecar 产品入口删除，避免形成双运行时或永久桥接层。
