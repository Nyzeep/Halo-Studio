# 工单 13：候选同步与许可证门槛记录

本记录对应基线 `416dbddb8b2a7f98cbbbb9f676075d8f33746039` 和独立分支
`codex/issue-13-pi-extension-audit`。候选只读检查，不是自动 merge 结果。

## 上游候选

- 初始上游导入：`ca56631e38f36db675583288df2bd44c540d250a`，Halo 初始导入提交为
  `58dd8fcdcf0fe97ee7b367751326000e95bb068d`。
- 候选参考树：`<HALO_BITFUN_REFERENCE_ROOT>`（只读 evidence locator
  `readonly-evidence://bitfun-latest`），`main` 与 `origin/main` 均为
  `1616ccaf73c0dabc50783344e583d304dd77622b`，树为
  `396a5fcd7423aa81e2f28610211f435599c94343`。
- 候选工作树状态干净；候选是 shallow/grafted checkout，`HEAD^` 退出码为
  `128`，因此不声称它与初始 commit 有本地 ancestry。
- 基于初始导入 manifest 的 raw UTF-8 path/mode/blob 比对：5,254 个 base entry、
  5,362 个 candidate entry；4,642 个相同，608 个修改，112 个新增，4 个删除，
  共 724 个 canonical changed entry。完整 724 条路径级记录见
  `issue-13-upstream-sync-diff.json`；候选 JSON 同时保留旧 Git C-quoted 输出的
  4,641/608/113/5/726 计数作为编码异常记录，不能把它当作 canonical diff。

未执行 checkout、merge、cherry-pick、fetch、commit、push 或上游写入。候选差异
尚未进入 `product/Halo Studio/`，因此 release gate 不能因候选记录本身而放行。

## 保留与冲突决策

- 保留 `product/Halo Studio/` Halo 品牌树、产品裁剪、LICENSE 和第三方 notice。
- 保留 Halo Workbench Runtime 的公共 Interface、状态所有权、任务/信任/证据
  边界；候选触及 desktop/runtime 和 `runtime-ports` 的路径只列为人工三方审查
  面，不自动接受。
- 保留 `PiRpcPort`、`pi-rpc-p0` 唯一 P0 身份、严格 LF JSONL、
  `--no-extensions --extension` 的 adapter-owned hash-verified copy、fail-closed
  extension UI 决策和现有 contract tests。
- 未进行 merge，所以没有可声称“已解决”的 merge conflict；724 个 canonical
  候选差异均是 review evidence，任何实际冲突必须在后续独立候选中人工决策。

## Extension inventory 与证据

唯一允许的 Halo P0 extension 是
`halo-workbench-permission-gate` `1.0.0`：

- 源文件：`product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts`
- 来源 commit：`e8c445d6a81d90851ac03d6aac7a4f11b6b749a3`
- 来源 commit tree：`f50918b6bdebc6067f409f248cc9182ff5bcdec3`
- `git hash-object`：`15d6908cc30e45f8812a87c591e58799d2f7ae69`
- SHA-256：`A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B`
- 能力：`tool_call` 前置拦截、`ctx.ui.confirm`，通过 Pi RPC
  `extension_ui_request/response` 请求一次性决议；无自定义 tool。
- 影响：extension 源码无文件、网络、进程、Git、凭据或 Renderer API；它仍继承
  Pi 进程的启动用户权限，不是沙箱。
- 依赖：运行时直接/传递依赖为空；`@earendil-works/pi-coding-agent` 只有
  type-only import，由用户安装的 Pi host 提供，不进入 Halo package/PNPM/Cargo
  lockfile。host closure 不作为 Halo release dependency 推断。
- 加载：adapter-owned temporary copy，固定使用 `--no-extensions --extension`；
  项目 `.pi`、用户全局 extension、npm/Git/网络运行时下载均拒绝。
- Pi 0.83.0 的 `llama.cpp` 是 always-inline built-in；`--no-extensions` 只关闭
  项目/用户发现式路径，不能移除它。它可访问 llama/Hugging Face 网络、读取凭据
  和 token 文件、写入模型状态，因此已单独列入 inventory 并明确排除 release。
- 许可证：extension 归 Halo `product/Halo Studio/LICENSE` 的 MIT 与
  `Copyright (c) 2026 CWing`；`product/THIRD_PARTY_NOTICES.md` 已记录 source、
  commit、hash 和 notice。Pi host 的许可证/来源不因包名推断；Pi 未随 P0 分发。
- 更新责任：Halo Studio maintainers；任何 source/loader/dependency/permission
  改动都必须升级固定版本并重新做 provenance/hash/license/contract audit。

机器清单：`docs/architecture/pi-first-party-extension-inventory.json`。

## Release gate

以下是审计 CLI JSON 结果的可读快照；机器判定只以
`node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json` 的
`status`、`findings`、`evidenceLocators` 和 `blockingReasons` 为准。本快照不
独立维护另一套 release 状态。

CLI 结果：`blocked`。

逐项原因：

1. 上游 candidate 尚未人工审查、应用或通过 Halo 产品/Workbench/Pi RPC 验证。
2. 初始导入 base `ca56631e38f36db675583288df2bd44c540d250a` 在 shallow/grafted
   candidate reference tree 中无法解析；`merge-base --is-ancestor` 退出 `128`，因此
   incremental-sync ancestry 保持 `unproven`，不能把 tree diff 当作 ancestry diff。
3. 声明的 initial-import tree `fba189b8b3db23c45a4bfed18c0250018b251387` 与由实际
   file manifest 重算的 `f6a559f45e266945921913f9752eb0e5b4609bdb` 不一致；base tree
   provenance 必须先 reconciliation，不能编造或覆盖 hash。
4. `<PI_REFERENCE_ROOT>` 无 Git metadata，无法为 host package 给出精确 source commit/tag。
5. Pi host package closure 不在 Halo lockfile，也没有作为 Halo release artifact 完整
   审计；不能从包名推断许可证。
6. 尚无精确 desktop distribution artifact 可核对 LICENSE/notice 是否随发行物携带。
7. 审计脚本对缺失 evidence、未固定 hash/version、未审计 `--extension` 路径、
   自动发现、运行时下载和外部绝对路径 fail closed。
8. 基线 `product/Halo Studio/Cargo.toml` 重复登记
   `src/crates/adapters/pi-rpc-adapter` workspace member；本任务按并行边界只报告，
   不修改 Cargo.toml/Cargo.lock，主集成必须先处理该依赖边界问题。
9. Pi inline built-in `llama.cpp` 没有精确 source commit/tag，且其完整能力、
   host 依赖闭包、许可证和发行物证据尚未纳入 Halo release gate。

## 审计入口

```powershell
node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json
node --test "product/Halo Studio/scripts/pi-extension-audit.test.mjs"
```

这些命令只读本地 manifest/source/Git object/许可证和 lockfile，不启动真实 Pi
RPC、不发送模型请求、不读取真实凭据、不下载依赖。

审计 module 的通过值是 `eligible`；任何声明阻断、缺失 provenance、host license、
dependency closure、exact artifact 或 candidate validation 都只能返回 `blocked`。

## 本轮验证矩阵（2026-08-03）

状态定义：`PASS` 表示本轮实际执行且退出码为 `0`；`BLOCKED` 表示命令实际执行
但由已记录的证据或环境门槛阻断；`NOT_RUN` 表示没有把缺失证据解释成失败或通过。

| 命令/检查 | 状态 | 退出码 | 结果摘要 |
| --- | --- | ---: | --- |
| extension SHA-256 | `PASS` | 0 | `A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B` |
| extension source commit/tree/blob | `PASS` | 0 | commit `e8c445d6a81d90851ac03d6aac7a4f11b6b749a3`, tree `f50918b6bdebc6067f409f248cc9182ff5bcdec3`, blob `15d6908cc30e45f8812a87c591e58799d2f7ae69` |
| extension `git hash-object` | `PASS` | 0 | `15d6908cc30e45f8812a87c591e58799d2f7ae69` |
| `cargo tree --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter` | `PASS` | 0 | 依赖树可解析；workspace member 重复仍由审计 gate 单独阻断 |
| `pnpm --dir "product/Halo Studio" run check:repo-hygiene` | `PASS` | 0 | repository hygiene passed |
| `pnpm --dir "product/Halo Studio" run product:check` | `PASS` | 0 | product assembly check passed |
| `pnpm --dir "product/Halo Studio" run product:test` | `PASS` | 0 | 17/17 passed |
| `pnpm --dir "product/Halo Studio" run type-check:web` | `PASS` | 0 | `tsc --noEmit` passed |
| `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter` | `PASS` | 0 | Serial rerun passed 17/17 (exit 0); a parallel invocation transiently reported 16/17 on `unknown_event` as `Transport` instead of `Protocol`, with no source change. |
| `node --check "product/Halo Studio/scripts/pi-extension-audit.mjs"` | `PASS` | 0 | Audit CLI syntax check passed after the fail-closed scan/metadata changes. |
| `$env:HALO_BITFUN_REFERENCE_ROOT='<matching read-only checkout for readonly-evidence://bitfun-latest>'; node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json` | `BLOCKED` | 1 | Matching locator produced 12 findings: `release-gate-declared-blocked`, `upstream-candidate-release-gate-blocked`, `upstream-history-boundary-untrusted`, `upstream-base-commit-unresolved`, `upstream-ancestry-unproven`, `upstream-initial-import-tree-mismatch`, `workspace-member-duplicate`, `built-in-extension-provenance-missing`, `host-source-provenance-missing`, `host-license-evidence-not-release`, `host-dependency-closure-incomplete`, and `release-artifact-evidence-missing`. |
| `Remove-Item Env:HALO_BITFUN_REFERENCE_ROOT; node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json` | `BLOCKED` | 1 | Missing locator is reported as `upstream-reference-tree-unavailable`; it is not interpreted as candidate provenance or a pass. |
| `node --test "product/Halo Studio/scripts/pi-extension-audit.test.mjs"` | `PASS` | 0 | 71/71 audit contract tests passed, including the blocked/eligible release-gate seam, upstream candidate-gate fail-closed behavior, structured exception output, safe evidence locators, declared-blocker fail-closed behavior, absolute-path redaction, external re-export rejection, computed global property/optional-call capability scanning with root aliases, canonical separation of host license paths from extension evidence/distribution/release-artifact paths, recorded `HEAD^`/clean-status evidence, source commit/tree/blob provenance, path-policy/permission validation, and host closure/license text claims. |
| Tauri candidate build against the Halo product tree | `NOT_RUN` | — | Candidate was not applied; automatic merge is prohibited, so no candidate build claim is made. |
| Base-tree Workbench Runtime Rust contracts: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts` | `PASS` | 0 | 16/16 passed on the existing Halo base tree; this is not candidate validation. |
| Base-tree Web UI suite: `pnpm --dir "product/Halo Studio/src/web-ui" run test:run` | `PASS` | 0 | Fresh rerun: 362 files / 2,373 tests passed. This is still base-tree evidence, not candidate validation; no test source was changed. |
| Candidate Workbench Runtime focused checks: `pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/formalPath.contract.test.ts src/infrastructure/workbench-runtime/client.test.ts` | `NOT_RUN` | — | Candidate was not applied to the product tree; no candidate contract claim is made. |
| 工单 07 Pi RPC contract and source-inventory checks | `PASS` | 0 | Fixed extension hash/source inventory checks passed on the Halo base tree; the candidate delta remains unvalidated. The current full adapter crate matrix is also `PASS` above. |
| `pnpm --dir "product/Halo Studio" run desktop:build:fast` | `BLOCKED` | 1 | Vendor checksum failure: first observed `allocator-api2/src/stable/vec/splice.rs` expected SHA-256 `95A460B3A7B4AF60FDC9BA04D3A719B61A0C11786CD2D8823D022E22C397F9C9`, actual `7CE9FA74764C36AB9043F7339548E96B0B68F7D1A16769C9CB066B9A538DCB14`; vendor and system environment were not modified. |
| `git diff --check` | `PASS` | 0 | 本轮 tracked diff 无 whitespace error |
| 精确 desktop distribution artifact 的 LICENSE/notice 内容核对 | `NOT_RUN` | — | 没有可核对的 exact release artifact；因此 release gate 保持 blocked |
| 真实 Pi RPC、真实凭据、真实模型请求、真实 Pi UI 验收 | `NOT_RUN` | — | 按任务禁止项未执行 |

`desktop:build:fast` 的 vendor 校验在当前运行首个报告为 `src/stable/vec/splice.rs`；
vendor checksum 文件仍记录其声明值而实际文件不同。这些 vendor 差异只作为环境阻断
证据记录，未修改 vendor 或系统环境。

## 本轮 owned 审计变化

本轮只修改 `pi-extension-audit.mjs`、其 Node contract test、架构审计说明、release-gate
artifact 和工单 13 说明。CLI 仍是独立只读审计入口，不是已接入 product/package release gate 的
命令；测试通过只证明审计规则本身，不能把 base-tree 测试写成 candidate validation。
脚本新增的 release-gate seam 与静态检查覆盖动态外部 import/require、网络能力、
无扩展名文本输入、根式 Windows 路径脱敏、fail-closed extension metadata，以及
完整 host closure/发行文件 evidence；computed `globalThis`/`window` property access
（含 alias 与 optional computed call）和 extension-owned distribution/release-artifact
路径复用也 fail closed；它们不会执行 Pi、模型、联网安装或凭据读取。

## 本轮补审计与上游重演练（2026-08-13）

本分支 `codex/issue-13-upstream-sync-rehearsal` 基于最新 main `331903c55`，仅完成
工单 13 的剩余部分 A/B/C。release gate 保持 `blocked`，未合并 main。

### A. 上游同步重演练

- 只读来源：`git ls-remote https://github.com/GCWing/BitFun.git HEAD refs/heads/main`
  退出码 `0`，HEAD 与 `refs/heads/main` 均为
  `9b05dd0e0e751c9e6e83fae3e9a0307bcd79b6b6`。
- 新候选 commit：`9b05dd0e0e751c9e6e83fae3e9a0307bcd79b6b6`（父提交前缀
  `59e06a0`，message `perf(cargo): centralize default feature ownership`，41 个文件
  变更、+326/−258），**不同于初始导入** `ca56631e38f36db675583288df2bd44c540d250a`，
  也新于既往候选 `1616ccaf73c0dabc50783344e583d304dd77622b`。
- 树级候选 diff：**未能在本地重算**。本环境 `git fetch`（POST git-upload-pack）与
  `codeload` tarball（大 GET）通道被阻断，GitHub REST API 被限流（403，core
  remaining 0）；仅 `git ls-remote` 元数据与 commit HTML 页面可达。既往已完整派生的
  路径级 diff 仍为 `1616ccaf`（`issue-13-upstream-sync-diff.json`），本次不覆盖。
- 冲突决策：不自动 merge、不 cherry-pick、不向 BitFun 上游提交或推送；保留 Halo
  品牌、产品裁剪、Workbench Runtime 公共 Interface 与 Pi RPC Adapter seam。新 commit
  主要触碰 Cargo workspace 默认 feature 归属策略（`AGENTS.md`/`AGENTS-CN.md`、
  根 `Cargo.toml`、多个 crate manifest、docs），与 Halo 自有 workspace 及已报告的
  `src/crates/adapters/pi-rpc-adapter` 重复 member 边界缺陷重叠，必须 Halo 专属三方
  审查，不自动应用。

### B. 第一方 extension inventory 逐项补审计（对最新 main）

- 源码 commit/tree/blob 与 SHA-256 重新核验：commit
  `e8c445d6a81d90851ac03d6aac7a4f11b6b749a3`（祖先于 main，`merge-base --is-ancestor`
  退出 `0`）、tree `f50918b6bdebc6067f409f248cc9182ff5bcdec3`、canonical blob
  `15d6908cc30e45f8812a87c591e58799d2f7ae69`、SHA-256
  `A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B`，与清单一致。
- 加载参数与 include_str!：`lib.rs` 含 `include_str!("halo_permission_gate.ts")`、
  `HALO_PI_EXTENSION_ID=halo-workbench-permission-gate`、
  `HALO_PI_EXTENSION_VERSION=1.0.0`、`--no-approve --no-extensions --extension`。
- 依赖边界：`@earendil-works/pi-coding-agent` 仅出现在 extension 源码第 1 行的
  `import type` 与 `lib.rs` 注释，`rg` 在 package.json/pnpm-lock/package-lock/
  Cargo.toml/Cargo.lock 中无命中（退出 1）；`cargo tree -p bitfun-pi-rpc-adapter`
  无可疑运行时依赖。
- 审计脚本 stale 修复：halo-scope.mjs 的 allowlist SHA-256 已从
  `894652f9373a70b878e24a36dd8a787610def01a03d158fc89a9b21d64e0374f` 更新为
  `d530b0bb45bbddbcae46d54db8e49cb960acb146c9cddf7a09cdf41d2f571bd7`（工单 05
  修复 createSession.taskId 后该文件变更，仍是负向守卫字面量，非真实路径输入）；
  静态检查 `adapter-runtime-load-path-unproven` 的 start-flow 断言已改为当前
  `create_session` 流程（`install_first_party_extension` -> `Some(extension)` ->
  `spawn_session_process` 内 `extension.as_ref().map(|e| e.path.as_path())` ->
  `pi_rpc_args(extension_path, ...)`），不再误报。
- 审计 CLI 当前结论仍为 `blocked`，9 项 finding：
  `release-gate-declared-blocked`、`upstream-candidate-release-gate-blocked`、
  `upstream-reference-tree-unavailable`、`workspace-member-duplicate`、
  `built-in-extension-provenance-missing`、`host-source-provenance-missing`、
  `host-license-evidence-not-release`、`host-dependency-closure-incomplete`、
  `release-artifact-evidence-missing`。

## 2026-08-15 gate-closure 会话（worktree issue-13-gate-closure-20260815，基于 origin/main fea500ca270aaaf487b6f2da52457d27928c2fd4）

本会话按工单 13 收尾授权在独立 D: worktree 完成；根工作树只读。审计 CLI 仍为唯一机器判定入口：
`node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json`（`HALO_BITFUN_REFERENCE_ROOT=<matching read-only checkout for readonly-evidence://bitfun-latest>`）。

### 修复前 vs 修复后（同一命令、同一环境变量）

| 状态 | findings | blocking reasons |
| --- | ---: | ---: |
| 修复前（main fea500ca 原样） | 14 | 23 |
| 修复后（本会话变更后） | 9 | 16 |

修复前 14 项 finding：`release-gate-declared-blocked`、`upstream-candidate-release-gate-blocked`、
`upstream-history-boundary-untrusted`、`upstream-base-commit-unresolved`、`upstream-ancestry-unproven`、
`upstream-initial-import-tree-mismatch`、`workspace-member-duplicate`、
`built-in-extension-provenance-missing`、`host-source-provenance-missing`、
`host-license-evidence-not-release`、`host-dependency-closure-incomplete`、
`license-lockfile-hash-mismatch`、`license-lockfile-size-mismatch`、`release-artifact-evidence-missing`。
（2026-08-15 早前 handoff 记录的 11/20 是 issue-12 合并前状态；Cargo.lock 变更后新增 2 项 lockfile finding。）

### 仓库内可修复项（本会话已修复）

1. `workspace-member-duplicate` — 删除 `product/Halo Studio/Cargo.toml` `[workspace].members` 中重复的
   `"src/crates/adapters/pi-rpc-adapter"`（保留 1 条）。`cargo metadata --no-deps` 退出 0。
2. `license-lockfile-hash-mismatch` / `license-lockfile-size-mismatch` — 刷新
   `docs/architecture/pi-first-party-extension-inventory.json` 中 Cargo.lock 的 SHA-256/size
   （当前 `F46652CB71E3346585E20B191D1C296976C1279E437F92068E5099F200A3CCC0` / 301136）；pnpm-lock 与
   package-lock 记录仍与现文件一致，未改。
3. `upstream-initial-import-tree-mismatch` — 基于主证据对账（追加式 reconciliation，不覆盖旧证据）：GitHub REST API
   （`GET /repos/GCWing/BitFun/git/commits/ca56631e…`）返回初始导入 base `ca56631e` 的真实 tree 为
   `f6a559f45e266945921913f9752eb0e5b4609bdb`，与 manifest 条目用审计算法重算一致；按工单规则「不能用新 hash 覆盖
   旧证据」，`issue-13-upstream-sync-candidate.json` 的 `base.initialImportTree`（`fba189b8`）与
   `treeBindingStatus`（`mismatch`）保持原值，核验结果以新增 `base.treeReconciliation`、
   `base.resolution.primarySourceVerification` 和 `rehearsal20260815` 追加记录；finding 仍按工单要求保持 blocked。
4. `built-in-extension-provenance-missing` / `host-source-provenance-missing` — Pi host
   `@earendil-works/pi-coding-agent` 0.83.0 来源现可精确固定：GitHub REST API 返回 tag `v0.83.0`（lightweight）
   直接指向 commit `845d6ff1f6643aba440341cce877ce1c43ebbc39`（tree `1ff6b68b…`，parent `44b26c9b…`，
   message "Release v0.83.0"）；该 commit 的 `packages/coding-agent/package.json` 声明 0.83.0。inventory 已为
   host 与 llama.cpp built-in 记录 sourceCommit/sourceTag（仅作 provenance，不把 Pi 许可证/closure 推断为 Halo release）。

### 需要外部只读证据（已尽力取得；网络通道限制如实记录）

- BitFun 上游候选 `9b05dd0e0e751c9e6e83fae3e9a0307bcd79b6b6`：
  - `git ls-remote`：2026-08-15 上游 `HEAD`/`refs/heads/main` 已前移到 `142d7e38729b3d646ae305c162e6848d0d44fff9`（tree `b4365d5a…`），9b05dd0e 不再是 HEAD。
  - commit API：9b05dd0e tree `74c1ff43695ee7273383df113759cce66365b280`，**直接父提交是
    `59e06a0e544f55b8152e62440be1a476e7633572`（merge of 33a0d094 + dc34e5fe），不是 ca56631e**；
    message `perf(cargo): centralize default feature ownership`，41 files，+326/−258，unsigned。
  - compare API `ca56631e...9b05dd0e`：status `ahead`、`behind_by=0`、`ahead_by=584`、`total_commits=584`，
    证明 **ca56631e 是 9b05dd0e 的祖先**（584 个提交之隔），但直接父不是它。
  - 路径级 diff（与 1616ccaf 同法：初始 manifest 条目 vs GitHub recursive trees API 的候选 tree）：
    base 5,254 / candidate blobs 6,005；identical 3,042、modified 1,827、added 1,136、removed 385、changed 3,348。
    完整记录新增 `issue-13-upstream-sync-diff-9b05dd0e.json`（不覆盖 1616ccaf 的既有 diff）。
  - 本地 `git fetch`（POST git-upload-pack）挂起、codeload tarball 仅 ~2.1MB/120s 后超时：本地 reference tree
    仍无法解析 9b05dd0e/ca56631e，因此 `upstream-history-boundary-untrusted`、`upstream-base-commit-unresolved`、
    `upstream-ancestry-unproven` 三个 finding 保持阻断（以本地只读树为准），但主证据已记录。
- Pi host 来源：如上，tag/commit/tree/版本全部由 GitHub REST API 核验（无真实 Pi RPC、无凭据读取、无模型请求）。

### 需要发布政策裁决（本会话不擅自改动 audit fail-closed 语义）

1. `release-gate-declared-blocked` — inventory 仍声明 blocked；只有全部证据 fresh 且维护者放行后才可置 passed。
2. `upstream-candidate-release-gate-blocked` — 候选未应用/未构建验证；工单禁止自动 merge。需裁决：候选继续保留为
   只读演练记录（release 以当前 base 为准），或授权一次独立候选应用+验证。
3. `host-license-evidence-not-release` / `host-dependency-closure-incomplete` — 工单 13 明确规定 Pi host 许可证/
   closure 不得推断为 Halo release 证据、Pi 二进制不随 Halo P0 分发；audit 对这两项 fail-closed。需裁决是否将
   “host excluded” 作为显式排除（inventory 已记录 provenance，但 release 证据仍为空）。
4. `release-artifact-evidence-missing` — 无精确 desktop 发行物；`desktop:build:fast` 被 vendor checksum 差异环境阻断
   （`src/stable/vec/splice.rs` 等），按边界不修改 vendor。需在可构建环境产出发行物并核对 LICENSE/notice 后放行。

### 本会话验证命令（均在 worktree 内执行）

| 命令/检查 | 退出码 | 结果 |
| --- | ---: | --- |
| `node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json`（env 指向 BitFun-latest） | 1 | blocked，8 findings / 14 reasons（修复后） |
| `node --test "product/Halo Studio/scripts/pi-extension-audit.test.mjs"` | 0 | 92/92 |
| `Get-FileHash -Algorithm SHA256 halo_permission_gate.ts` | 0 | `A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B` |
| `git hash-object -- halo_permission_gate.ts` | 0 | `15d6908cc30e45f8812a87c591e58799d2f7ae69` |
| source commit/tree/blob 核验 | 0 | e8c445d6 / f50918b6 / 15d6908c，`merge-base --is-ancestor` 退出 0 |
| `rg` lib.rs 加载/边界 tokens | 0 | include_str!、HALO_PI_EXTENSION_ID/VERSION/PERMISSIONS、--no-extensions、--extension 均在 |
| `rg -F '@earendil-works/pi-coding-agent'` 四个 lockfile/manifest | 1 | 无命中（期望） |
| `cargo metadata --no-deps` / `cargo tree -p bitfun-pi-rpc-adapter`（--offline） | 0 | workspace 解析正常，member 唯一 |
| `cargo test -p bitfun-pi-rpc-adapter extension_decision_is_redacted_one_shot_and_duplicate_request_fails_closed` | 0 | 1 passed |
| `pnpm --dir "product/Halo Studio" run check:repo-hygiene` | 0 | passed；issue-12 矩阵 passed（release blocked） |
| `pnpm --dir "product/Halo Studio" run product:check` | 0 | ok |
| `pnpm --dir "product/Halo Studio" run product:test` | 0 | 17/17 |
| `pnpm --dir "product/Halo Studio" run type-check:web` | 0 | tsc --noEmit passed |
| `pnpm --dir "product/Halo Studio" run desktop:build:fast` | 1 | Web 构建完成（vite built in 33.59s）；cargo 在 tauri CLI 路径下校验 vendored source checksum 失败：`allocator-api2/Cargo.toml.orig` expected `c1688fbd…` actual `64ee4a15…`；vendor 未修改 |
| `git diff --check` | 0 | passed |


vendored source 复核（2026-08-15）：对 `product/Halo Studio/vendor/cargo/*/.cargo-checksum.json` 全量比对，
1,237 个文件与其声明 SHA-256 不一致（首个：`allocator-api2/src/stable/vec/splice.rs` declared
`95a460b3…` actual `7ce9fa74…`）。观测：直接 `cargo build --offline -p halo-tauri-desktop` 与 `-p allocator-api2`
均 exit 0（cargo 未校验 directory-source checksum），但工单命令 `desktop:build:fast`（tauri CLI
`tauri build --features custom-protocol`）在 cargo 读取 vendored source 时触发 checksum 校验并失败，exit 1。
按边界未修改 vendor，`release-artifact-evidence-missing` 保持环境阻断。

### 结论

gate 仍为 `blocked`（9 findings / 16 blocking reasons），剩余项全部属于「外部只读证据/环境」或「发布政策裁决」；
本会话未启动真实 Pi RPC、未发送真实模型请求、未读取凭据、未修改 vendor/lockfile/i18n baseline/历史证据/根工作树。
工单 14 未启动。
