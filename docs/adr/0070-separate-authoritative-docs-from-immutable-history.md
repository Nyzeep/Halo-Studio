---
status: accepted
---

# 分离权威文档、不可变历史与临时工作空间

Halo Studio 将当前词汇、ADR、目标架构、需求和验证集中在受跟踪的 `docs/` 权威目录中，把旧 PySide/Sidecar 资料作为不可变历史基线隔离，并把 `.scratch/` 与 Git 工作树视为可重建的本地工作空间。该选择保留迁移审计证据，同时防止旧实现资料和临时产物继续指导 Halo Studio/Tauri 产品开发。
