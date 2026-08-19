---
status: accepted
related: 0050 gate managed delivery acceptance on fresh reviewed evidence; 0072 Pi RPC P0 adapter
---

# 工单 13 release gate 的显式排除与演练-only 上游候选政策

## 背景

工单 13 的 `pi-extension-audit` gate 在 2026-08-15 收尾时仍有 9 个 finding / 16 条 blocking
reasons。其中一部分是「候选/宿主未纳入 Halo 自身发行物」导致的 fail-closed 记录，而不是 Halo
第一方 extension 或发行物的真实缺口。本 ADR 记录维护者裁决：哪些项是**显式排除**（记录但不阻断
Halo release gate），哪些项保持阻断。

## 决策

1. **上游候选 = rehearsal-only（仅演练）**。上游（原 BitFun）候选（当前记录 `9b05dd0e`，主证据由 GitHub
   REST API 核验：tree `74c1ff43…`、直接父 `59e06a0e…`；base `ca56631e` 是其祖先，compare API
   `behind_by=0`）只作为只读同步演练记录，不自动 merge、不应用、不进入 Halo 产品树。Halo 发行以
   当前固定 base 为准。因此上游候选验证类 finding（candidate release gate、history boundary、
   base-commit unresolved、ancestry unproven）在 inventory 声明 `releasePolicy.upstreamCandidate.scope =
   "rehearsal-only"` 且附 reason/policySource 时，标记为 `blocking: false` 的显式排除记录。
2. **Pi host = 显式排除**。`@earendil-works/pi-coding-agent` 由用户本机安装、不随 Halo P0 分发；
   其许可证/依赖闭包不进入 Halo release 证据（工单 13 明文「不能把 … 许可证推断为已审计」）。
   来源 provenance 仍必须固定（v0.83.0 = commit `845d6ff1…`）。host license/closure 类 finding 在
   inventory 声明 `releasePolicy.hostPackage.excludedFromRelease = true` 且附 reason/policySource 时，
   标记为 `blocking: false`。
3. **初始导入 tree 声明不覆盖**。工单明文「不能用新 hash 覆盖旧证据」：`initialImportTree`
   `fba189b8…` 与 `treeBindingStatus: mismatch` 保持原值；主证据（GitHub API 证明真实 tree 为
   `f6a559f4…`）以追加式 reconciliation 记录。该 finding 保持阻断，直到维护者另行裁决或本地参考树
   可证明。
4. **发行物证据保持阻断**。无精确 desktop 发行物；`desktop:build:fast` 在 tauri CLI 路径下被
   vendored source checksum 校验阻断（1,237 个 vendor 文件与 `.cargo-checksum.json` 不一致，首个
   `allocator-api2/Cargo.toml.orig`）。按边界不修改 vendor；该 finding 保持阻断，待可构建/CI 发行
   通道提供精确发行物后放行。

## 后果

- 审计 CLI 输出区分 `blocking: false` 的排除记录与真实阻断；`status: eligible` 只要求不存在真实
  阻断 finding。
- 排除本身 fail closed：`releasePolicy` 缺失、schema 错误、缺 reason/policySource、或 host 未声明
  `excludedFromRelease: true` 时，全部维持原阻断语义并新增 `release-policy-invalid` finding。
- 排除不能豁免真实缺口：host source provenance 缺失、extension 自身 hash/许可证/发行物缺口仍阻断。
- 工单 13 在当前裁决下仍非 eligible（tree 声明与发行物证据两项保持阻断）；工单 14 不启动。
