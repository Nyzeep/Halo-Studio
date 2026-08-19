---
status: superseded by ADR-0054 and ADR-0069
---

# 首期不启动旧 Halo 运行时

Halo Studio 首期以纳入 Halo Studio 快照后的 Tauri 桌面应用及 Halo Studio Runtime 作为唯一可启动产品。现有的 PySide/QML、Rust Sidecar 和未完成 Electron/React 代码保留为功能规则、交互意图和迁移参考，但不随新应用启动、不作为辅助运行时，也不通过桥接形成并行会话权威；只有经过需求核对的 Halo 能力才会被选择性迁入新的产品基座。
