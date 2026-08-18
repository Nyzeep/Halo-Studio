---
status: superseded by ADR-0066
---

# 将 Halo Studio 一次性导入为可审计的 Halo 自有源码树

Halo Studio 将从确认的 Halo Studio 快照一次性、可审计地导入必要源码，形成独立维护的 Halo 自有产品源码树。`D:\Halo Studio\BitFun-main` 在迁移期间保留为只读对照和许可证依据；正式构建不依赖、桥接或持续修改该目录，从而避免将产品架构变成隐式 fork 或运行时包装层。
