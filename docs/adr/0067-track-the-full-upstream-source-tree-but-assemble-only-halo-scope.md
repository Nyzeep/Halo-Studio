---
status: superseded by ADR-0079
---

# 跟踪完整 Halo Studio 源码树但只装配 Halo 发布范围

Halo Studio 在自己的下游产品仓库中保留完整 Halo Studio 上游源码关系，以降低持续同步时的重复冲突和遗漏风险；首期构建、路由、导航和后台初始化仍只装配 Halo 的本地桌面编码主链。范围外源码的存在不代表产品能力可用，办公协作、Mini App、远程、Relay、移动端等模块继续受 ADR-0019 的运行路径排除约束。
