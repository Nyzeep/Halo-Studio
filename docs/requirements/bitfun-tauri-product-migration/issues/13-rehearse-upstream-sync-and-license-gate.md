# 13 - 演练 BitFun 上游同步与许可证门槛

**What to build:** 维护者可以把一个新的 BitFun 上游 commit 作为独立候选同步到 Halo，审查冲突与产品范围，并通过构建、回归和许可证门槛决定是否接纳；同步不得破坏 Halo Workbench Runtime 的 OpenCode Adapter seam，也不得触碰 OpenCode 上游仓库。

**Blocked by:** 04 - 建立 Halo Workbench Runtime 公共契约.

**Status:** ready-for-agent

## 验收标准

- [ ] 从只读 BitFun 上游参考树获取一个不同于初始导入的精确 commit，并记录前后来源。
- [ ] 同步候选保持为独立、可审计的 Halo 变更，不自动合并，也不向 BitFun 上游提交或推送。
- [ ] 冲突处理保留 Halo 品牌、产品裁剪、Workbench Runtime 公共 Interface 和 OpenCode Adapter seam，并记录所有需要人工判断的差异。
- [ ] 候选通过 Tauri 构建、产品范围检查、工单 04 Runtime Interface 契约和来源清单核对；若执行时工单 07 已完成，还必须重跑届时存在的 OpenCode Adapter 自动化。
- [ ] 源码与发行包包含所需 BitFun MIT 归属和第三方声明，许可证检查失败阻止接纳。
- [ ] 同步不引入 `D:\BitFun-main`、`D:\opencode-dev` 或其他外部绝对路径，不复制/vendor OpenCode 内部源码，也不启用 Pi/Code Agent P0 路径。

## 验证要求

- 记录精确上游 commit、候选 diff、冲突决策、清单变化、许可证 inventory 和全部门槛命令/退出码。
- 失败候选必须可安全丢弃，不污染已验证产品树或上游参考树。
