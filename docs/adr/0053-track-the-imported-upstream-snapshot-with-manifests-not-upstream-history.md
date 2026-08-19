---
status: superseded by ADR-0066
---

# 通过导入清单而非上游历史追溯 Halo Studio 快照

Halo Studio 的自有源码树记录 Halo Studio 的精确上游提交、一次性导入清单和许可证证明，但不导入完整上游 Git 历史，也不保留 submodule 依赖。此记录提供许可证审计与安全修复比对所需的可追溯性，同时使 Halo 在导入后拥有清晰、独立的版本历史和维护节奏。
