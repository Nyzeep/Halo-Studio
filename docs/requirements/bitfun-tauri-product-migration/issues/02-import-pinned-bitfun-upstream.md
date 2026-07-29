# 02 - 固定并纳入可审计的 BitFun 上游基线

**What to build:** 维护者可以从一个精确的 BitFun 上游 commit 重建 Halo 产品树，审计来源与许可证，并确认正式构建不依赖仓库外的参考目录。

**Blocked by:** 01 - 固化可迁移能力基线与仓库卫生.

**Status:** ready-for-review

- [x] 从 `GCWing/BitFun.git` 选择并记录精确上游 commit；没有 Git 元数据的本地快照不得作为来源证明。
- [x] 完整 BitFun 源码关系进入受跟踪的 Halo 产品树，导入内容与来源清单可机械核对。
- [x] BitFun MIT 许可证、版权归属和适用的第三方声明随源码保留。
- [x] Halo 的构建、测试和开发入口不引用外部上游参考树的绝对路径。
- [x] 上游远端只作为获取与比较来源；Halo 改动不会被推送或提交到 BitFun 上游仓库。

验证证据：`../02-bitfun-upstream-import-evidence.md`、
`../bitfun-upstream-manifest.json` 和 `npm run bitfun:verify-import`。
