# 13 - 演练 BitFun 上游同步与许可证门槛

**What to build:** 维护者可以把一个新的 BitFun 上游 commit 作为独立候选同步到 Halo，审查冲突与产品范围，并通过构建、回归和许可证门槛决定是否接纳，而不会触碰上游仓库。

**Blocked by:** 04 - 建立 Halo Workbench Runtime 公共契约.

**Status:** ready-for-agent

- [ ] 演练从只读上游参考树获取一个不同于初始导入的精确 commit，并记录前后来源。
- [ ] 同步候选保持为独立、可审计的 Halo 变更，不自动合并，也不向 BitFun 上游提交或推送。
- [ ] 冲突处理保留 Halo 品牌、产品裁剪和 Workbench Runtime 公共契约，并记录需人工判断的差异。
- [ ] 候选通过 Tauri 构建、产品范围检查、关键自动化回归和来源清单核对。
- [ ] 源码与发行包均包含所需 MIT 归属和第三方声明，许可证检查失败会阻止接纳。
