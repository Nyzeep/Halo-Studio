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

状态：`blocked`。

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

## 本轮验证矩阵（2026-08-03）

状态定义：`PASS` 表示本轮实际执行且退出码为 `0`；`BLOCKED` 表示命令实际执行
但由已记录的证据或环境门槛阻断；`NOT_RUN` 表示没有把缺失证据解释成失败或通过。

| 命令/检查 | 状态 | 退出码 | 结果摘要 |
| --- | --- | ---: | --- |
| extension SHA-256 | `PASS` | 0 | `A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B` |
| extension `git hash-object` | `PASS` | 0 | `15d6908cc30e45f8812a87c591e58799d2f7ae69` |
| `cargo tree --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter` | `PASS` | 0 | 依赖树可解析；workspace member 重复仍由审计 gate 单独阻断 |
| `pnpm --dir "product/Halo Studio" run check:repo-hygiene` | `PASS` | 0 | repository hygiene passed |
| `pnpm --dir "product/Halo Studio" run product:check` | `PASS` | 0 | product assembly check passed |
| `pnpm --dir "product/Halo Studio" run product:test` | `PASS` | 0 | 17/17 passed |
| `pnpm --dir "product/Halo Studio" run type-check:web` | `PASS` | 0 | `tsc --noEmit` passed |
| `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter` | `BLOCKED` | 1 | 16/17；`extension_error_is_a_protocol_failure_and_timeout_decision_is_deny_path` fails with `fake Pi event arrived before the contract timeout`; the isolated focused test passes. No runtime test or source was changed. |
| `HALO_BITFUN_REFERENCE_ROOT=<local read-only checkout>; node "product/Halo Studio/scripts/pi-extension-audit.mjs" --json` | `BLOCKED` | 1 | `blocked`，11 项 finding；包含 base/tree provenance、shallow ancestry、host/license、workspace 与 artifact 缺口 |
| `node --test "product/Halo Studio/scripts/pi-extension-audit.test.mjs"` | `PASS` | 0 | 33/33 audit contract tests passed |
| Tauri candidate build against the Halo product tree | `NOT_RUN` | — | Candidate was not applied; automatic merge is prohibited, so no candidate build claim is made. |
| 工单 04 Workbench Runtime contract checks: `pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/formalPath.contract.test.ts src/infrastructure/workbench-runtime/client.test.ts` | `NOT_RUN` | — | Candidate was not applied to the product tree; no candidate contract claim is made. |
| 工单 07 Pi RPC contract and source-inventory checks | `PASS` | 0 | Adapter crate tests and the fixed extension hash/source inventory checks passed on the Halo base tree; candidate delta remains unvalidated. |
| `pnpm --dir "product/Halo Studio" run desktop:build:fast` | `BLOCKED` | 1 | Existing vendor checksum mismatch：`allocator-api2/src/stable/slice.rs` expected `089263…`/actual `14d6eb…`（the build also previously observed `src/lib.rs` and `LICENSE-APACHE` mismatches）；未修改 vendor 或系统环境 |
| `git diff --check` | `PASS` | 0 | 本轮 tracked diff 无 whitespace error |
| 精确 desktop distribution artifact 的 LICENSE/notice 内容核对 | `NOT_RUN` | — | 没有可核对的 exact release artifact；因此 release gate 保持 blocked |
| 真实 Pi RPC、真实凭据、真实模型请求、真实 Pi UI 验收 | `NOT_RUN` | — | 按任务禁止项未执行 |

`desktop:build:fast` 同时观察到 `allocator-api2/LICENSE-APACHE` 的现有实际 SHA-256
为 `62C7A1E35F56406896D7AA7CA52D0CC0D272AC022B5D2796E7D6905DB8A3636A`，而 vendor
checksum 文件声明 `20fe7b00e904ed690e3b9fd6073784d3fc428141dbd10b81c01fd143d0797f58`；
这些 vendor 差异只作为环境阻断证据记录。
