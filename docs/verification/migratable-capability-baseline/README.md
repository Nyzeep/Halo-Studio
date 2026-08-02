# 可迁移能力基线

**Status:** historical baseline - non-authoritative

本目录记录旧 PySide/QML 与 Rust Sidecar 产品中已经实现和自动化验证的行为。它是 BitFun/Tauri 迁移的输入，不是目标产品验收或 P0 放行证明。

旧 GitHub #9–#14（旧六票）的实现和自动化结果在本目录中只作为“可迁移能力基线”保存；它们不代表目标 Tauri 产品已验收，也不代表 P0 已放行。旧产品的内部 JSONL 传输同样不是目标产品的兼容承诺。

- [旧十票验收与 TDD 基线](original-ten-task-acceptance-and-tdd-baseline.md)
- [旧实现追踪矩阵](traceability.md)
- [迁移工单 01 执行证据](issue-01-freeze-evidence.md)

迁移前的真实 OpenCode 原生 UI 记录只属于历史比较材料，从未成为目标 Tauri 的有效 P0 门槛。目标产品的行为等价结论由迁移工单 12 建立，当前 P0 的真实 Pi RPC 原生 UI 验收由工单 14 和 15 完成。
