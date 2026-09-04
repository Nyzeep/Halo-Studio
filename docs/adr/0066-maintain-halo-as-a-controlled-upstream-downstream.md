---
status: superseded by ADR-0079
---

# 将 Halo Studio 维护为受控的 Halo Studio 下游产品

Halo Studio 以 `GCWing/BitFun.git` 为只拉取的上游，并在自己的产品仓库中提交、发布和维护 Halo 变更；不会向 Halo Studio 上游直接提交或推送。上游变更通过显式同步候选进入 Halo，只有在产品裁剪、受管运行时契约、自动化测试和真实 UI 验收通过后才能合并，从而兼顾持续获取上游能力与 Halo 的独立产品边界。
