# 旧 PySide/Sidecar 历史基线

**Status:** archived - non-authoritative

本目录保留旧 PySide/QML、Rust Sidecar 和相关设计研究的原始资料，用于行为等价迁移、审计和复现。归档正文保持迁移时原样，其中的旧路径、状态和架构判断可能已经失效；任何内容都不得指导当前 Halo Studio 实现。

当前替代依据：

- [目标产品架构](../../architecture/target-product.md)
- [BitFun/Tauri 迁移规格](../../requirements/bitfun-tauri-product-migration/00-bitfun-tauri-product-migration-spec.md)
- [可迁移能力基线](../../verification/migratable-capability-baseline/README.md)

## 内容

| 路径 | 历史用途 |
| --- | --- |
| `architecture/` | 旧 PySide/QML + Sidecar 分层说明 |
| `contracts/` | 旧 JSONL IPC 与模块所有权契约 |
| `requirements/` | 旧产品需求、决策地图、规格和 #14 前置工单 |
| `design/` | 旧 IDE、文件系统、Agent 协议和差异化设计研究 |
| `tickets/original-01-10/` | 最初十个本地 tracer-bullet 工单 |

失效链接不在归档正文中修补。需要引用历史事实时，应从当前验证文档链接到具体归档文件，并明确它只证明过去状态。
