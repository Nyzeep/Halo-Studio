# PR 草稿：Halo Studio 全面去 BitFun 化（去品牌化）并完成独立运行验证

> 说明：`gh` 未登录（`gh auth status` 失败），按任务约定将中文 PR 草稿写入本文件；环境可用 `gh` 后可据此创建真实 PR。

## 标题

feat: Halo Studio 全面去 BitFun 化（去品牌化）并完成独立运行验证与中文开发文档

## 分支

- 源分支：`codex/halo-studio-debrand-20260818`
- 目标分支：`main`
- 不自动合并；合并由维护者决定。

## 背景

仓库已独立于 BitFun 开发，但代码与文档仍残留大量 bitfun 命名与品牌表述。本次变更把本项目命名空间内的全部文件/目录/标识符改写为 Halo Studio 命名（`halo_studio` / `halo-*` / `@halo-studio/*`），完成独立构建/测试/运行验证，并交付中文开发文档。

## 变更内容

1. Rust workspace 与安装器全面更名：`bitfun-*` crate/包/bin/lib/feature → `halo-*`（含 `bitfun-desktop`→`halo-desktop`、`bitfun-pi-rpc-adapter`→`halo-pi-rpc-adapter`、`BitFunError/BitFunResult`→`HaloError/HaloResult`），同步 `use` 导入、依赖键、Cargo.lock、脚本/CI/测试。
2. npm/TS 与前端资源更名：`@bitfun/web-ui`→`@halo-studio/web-ui`，`bitfun-installer`/`bitfun-mobile-web`/`bitfun-miniapp-market-web`/`bitfun-e2e-tests`→`halo-*`；`bitfun-canvas`→`halo-canvas`；主题、图标、宠物、徽标资源同步；锁文件重新生成。
3. 脚本/CI/部署/目录路径更名：`BitFun-Installer`→`Halo-Installer`、`products/bitfun`→`products/halo`、`docs/requirements/bitfun-tauri-product-migration`→`halo-tauri-product-migration`、ADR/归档文件名 `bitfun`→`upstream` 等；halo-scope、check-repo-hygiene、verify-old-six 等守卫同步更新且未放宽基线。
4. 文档与历史证据去 BitFun 化：CONTEXT、ADR、需求、验证与归档按“历史记录/上游对照（已归档）”标注；历史证据内容不篡改，命令名保留历史记录。
5. 中文开发文档：新增 `docs/development/architecture.md`、`build-and-test.md`、`pi-rpc-adapter.md`、`contribute.md`；重写根 README（技术徽章、目录说明、如实状态）。
6. 独立运行验证：`check:repo-hygiene`、`product:check/test`、`type-check:web`、`desktop:build:fast`、e2e smoke、`cargo test -p halo-pi-rpc-adapter / halo-tauri-desktop`、`halo-scope.test`、`git diff --check` 全部通过；`desktop:dev` 启动确认窗口可启动且无 bitfun 标识泄漏。

## 豁免清单（独立性扫描剩余命中全部可解释）

- `BitFun-latest/**`：任务硬性豁免。
- `product/Halo Studio/vendor/**`：第三方 vendored 源码，不改写语义（当前无 bitfun 命中）。
- `openbitfun.com` 系列真实外部域名/Provider id：保留并标注外部服务。
- `GCWing/BitFun`、`D:\BitFun-main`、`BitFun-latest`：上游仓库/对照目录标识。
- `docs/archive/**`、`docs/verification/**`、`docs/requirements/halo-tauri-product-migration/artifacts/**`、`upstream-manifest.json`、`docs/archive/legacy-brand-assets/**`：不可篡改历史证据。
- `product/THIRD_PARTY_NOTICES.md`：上游 MIT 署名，保留事实。
- `scripts/verify-old-six-behavior-equivalence.mjs`：历史证据命令名（`bitfun-*` crate）为证据匹配项。
- 文档中的“上游（原 BitFun）”历史标注：按任务要求保留语义并标注“历史记录/上游对照（已归档）”。

## 遗留风险与状态

- 工单 14/15 真实 Pi RPC 原生 UI 验收记录保持 `not-run`，P0 未放行。
- `i18n:audit` 的 sharedTermDuplicates 基线偏差与主题审计 2 项失败在更名前已存在（以 HEAD 快照复核确认），未随本变更恶化，基线未放宽。
- `halo-pi-rpc-adapter` 契约测试在与其他重型构建并发时出现过 2 例时序性失败，单独运行稳定通过（CI 串行场景不受影响）。
- 旧用户配置中的历史主题 id（如 `bitfun-light`）由运行时兼容映射到 `halo-*`，无需迁移用户数据。
