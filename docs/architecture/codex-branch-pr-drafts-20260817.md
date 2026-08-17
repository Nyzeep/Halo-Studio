# Codex 分支合并 PR 草稿（中文）

记录日期：2026-08-17。以下三个分支均已推送到远端、尚未并入 `main`；按用户要求提供中文 PR 标题与正文草稿，供创建 PR 时直接使用。

## PR 1：工单 14

### 标题

工单 14：Halo 隔离与 Pi 配置加固（真实 UI 验收记录保持 not-run）

### 正文

**分支**：`codex/issue-14-real-pi-rpc-native-ui-acceptance-20260816` → `main`

**摘要**：收口工单 14 的实现部分：

- Halo 启动不再初始化 BitFun SSH/远程工作区状态，状态槽 fail closed；
- write-only Base URL 未填写时保留既有 endpoint；update 前校验 credential_ref 的 provider 归属；
- 一次性密钥改为非受控输入并即时清空；切换 Provider 必须重录凭据；
- 工作台导航、旧静态入口退役与 smoke 对齐正式 Halo 产物；BitFun-latest 仅作只读 UI 布局参考；
- 上游 UI 参考与验收边界已写入工单规范与验收记录。

**验证**：前端聚焦 28/28、halo-scope 17/17、pi-rpc-adapter 11/11、halo-tauri-desktop 2/2、type-check、desktop:build:fast 通过；e2e smoke 4/6（既有选择器/窗口桥不匹配）。

**已知未完成（如实记录）**：工单 14 的真实 Pi RPC 原生 UI 验收清单保持 `not-run`，P0 不放行；验收工作区写入边界因实际使用目录与预设验收工作区不一致，不能判定为 pass。

## PR 2：工单 15

### 标题

工单 15：收缩旧产品实现并记录 15b 延后决策（ADR-0074）

### 正文

**分支**：`codex/issue-15-contract-legacy-product-and-revalidate-20260816` → `main`

**摘要**：工单 15a 独立收缩：

- 删除 `app/`、`sidecar/`、`apps/desktop/`、`protocol/v1/` 与 `packages/`（旧根 TS workspace），约 331 个跟踪文件、-72k 行；
- 删除仅服务旧运行时的根脚本；修复 `assert-repository`（maxBuffer、测试 blob 源、互为排除）与 `verify-bitfun-import` 扫描根；
- README、.gitignore、目标产品架构、核心重建验证文档改为历史口径；Gemini 注释移除旧 packages/core 引用；根 package-lock 重生成；
- 15b（产品树 OpenCode 全量移除）按 ADR-0074 延后至 DeepSeek Harness 基座迁移。

**验证**：repo-hygiene、product:check、product:test 17/17、type-check、halo-scope 17/17、assert-repository 4/4 通过。

**已知未完成（如实记录）**：删除后的完整复验（构建/打包、契约、extension 审计、e2e、行为等价、真实 UI 主链）保持 `not-run`；真实 Pi RPC 原生 UI 验收保持 `not-run`，P0 不放行。

## PR 3：迁移审计

### 标题

迁移审计：Halo 能力与 DeepSeek Harness（dsh）基座对照及插件 01/02 规格

### 正文

**分支**：`codex/deepseek-harness-migration-audit-20260816` → `main`

**摘要**：

- `docs/architecture/deepseek-harness-migration-audit.md`：以 dsh `47f94385` 为参考，映射 Halo P0 能力与 dsh 模块（session/agent-loop/interaction/preset/scope/sandbox 等），列出 8 项需自研插件与迁移顺序；
- `docs/requirements/deepseek-harness-migration/01-pi-rpc-and-credential-provider-spec.md`：插件 01/02（Pi RPC 执行 + 系统凭据 Provider）规格，`ready-for-agent`；
- seams 设计待用户最终确认后发布；GitHub issue 创建需要登录态。

**风险**：dsh 为 developer preview，存在兼容性破坏；采纳前需重新核对上游提交。
