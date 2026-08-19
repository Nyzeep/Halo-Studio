# feat: 全面去 BitFun 化——Halo Studio 独立命名、独立构建验证与中文开发文档

## 背景

仓库已独立于 BitFun 开发，但代码与文档仍残留大量 bitfun 命名与品牌表述。本次变更把本项目命名空间内的全部文件/目录/标识符改写为 Halo Studio 命名（`halo_studio` / `halo-*` / `@halo-studio/*`），完成独立构建、测试、运行验证，并交付中文开发文档与带技术徽章的 README。

## 变更内容

1. Rust workspace 与安装器全面更名：全部 `bitfun-*` crate/包/bin/lib/feature → `halo-*`（含 `bitfun-desktop`→`halo-desktop`、`bitfun-pi-rpc-adapter`→`halo-pi-rpc-adapter`、`BitFunError/BitFunResult`→`HaloError/HaloResult`），同步 `use` 导入、依赖键、Cargo.lock、脚本/CI/测试；`BitFun-Installer`→`Halo-Installer`。
2. npm/TS 与前端资源更名：`@bitfun/web-ui`→`@halo-studio/web-ui`，其余包 → `halo-*`；`bitfun-canvas`→`halo-canvas`；主题、图标、宠物、徽标资源同步；锁文件重新生成。
3. 脚本/CI/部署/目录路径更名：`products/bitfun`→`products/halo`、迁移文档目录 → `halo-tauri-product-migration`、ADR/归档文件名 `bitfun`→`upstream` 等；halo-scope、check-repo-hygiene、verify-old-six 等守卫同步更新且未放宽基线。
4. 文档与历史证据去 BitFun 化：CONTEXT、ADR、需求、验证与归档按“历史记录/上游对照（已归档）”标注；历史证据内容不篡改，历史命令名保留原样。
5. 中文开发文档：新增 `docs/development/architecture.md`、`build-and-test.md`、`pi-rpc-adapter.md`、`contribute.md`；重写根 README（技术徽章、目录说明、如实状态）。

## 验证矩阵（更名后最终状态）

| 命令 | 结果 |
| --- | --- |
| `pnpm run check:repo-hygiene` | pass |
| `pnpm run product:check` / `product:test` | pass（17/17） |
| `pnpm run type-check:web` | pass |
| `pnpm run desktop:build:fast` | pass |
| `pnpm run e2e:test:smoke` | pass（6/6，窗口标题 Halo Studio） |
| `cargo test -p halo-pi-rpc-adapter` | pass（11+43） |
| `cargo test -p halo-tauri-desktop` | pass（2/2） |
| `node --test scripts/halo-scope.test.mjs` | pass（17/17） |
| `git diff --check` | pass |
| `desktop:dev` 启动 | pass，无 bitfun 标识泄漏 |

## 豁免清单（独立性扫描剩余命中全部可解释）

- `BitFun-latest/**`（任务硬性豁免）；`product/Halo Studio/vendor/**`（第三方 vendored 源码）。
- `openbitfun.com` 系列真实外部域名/Provider id；`GCWing/BitFun`、`D:\BitFun-main` 上游标识。
- `docs/archive/**`、`docs/verification/**`、迁移 artifacts、`upstream-manifest.json`、`legacy-brand-assets/**`（不可篡改历史证据）。
- `product/THIRD_PARTY_NOTICES.md`（上游 MIT 署名）；`verify-old-six-behavior-equivalence.mjs`（历史证据命令名）。
- 文档中“上游（原 BitFun）”历史标注（已统一标注“历史记录/上游对照（已归档）”）。

## 遗留风险

- 工单 14/15 真实 Pi RPC 原生 UI 验收记录保持 `not-run`，P0 未放行。
- `i18n:audit` 的 sharedTermDuplicates 基线偏差与主题审计 2 项失败为更名前存量问题（以 HEAD 快照复核确认），未放宽基线。
- `halo-pi-rpc-adapter` 契约测试在与其他重型构建并发时出现过 2 例时序性失败，单独运行稳定通过。
- 旧用户配置中的历史主题 id 由运行时兼容映射处理，无需迁移用户数据。

## 合并说明

- 源分支：`codex/halo-studio-debrand-20260818`；目标分支：`main`。
- 合并方式：merge commit（保留 6 个中文提交），不使用 force。
- 合并已由维护者授权；合并后远程功能分支保留，不自动删除。
