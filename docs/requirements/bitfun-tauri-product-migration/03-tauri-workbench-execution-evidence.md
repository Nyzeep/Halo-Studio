# 工单 03 执行证据：Halo Tauri 开发工作台

**状态：** READY-FOR-REVIEW（latest 2026-07-30 result; earlier BLOCKED sections are retained below as historical audit trail）

**日期：** 2026-07-30（保留 2026-07-29 的初始阻断记录）

**工作树：** `D:\Halo Studio\.worktrees\issue-03-tauri-workbench`

**分支：** `codex/issue-03-halo-tauri-workbench`

本记录只覆盖工单 03。工单 04 未开始，也没有把 HTTP 页面或旧 PySide/QML 启动结果计为 Tauri 验收。

Latest decision (2026-07-30): Ticket 03 is `ready-for-review`. The audited 03a vendor lock path is in place, VS x64 locked/offline Cargo verification passed, `pnpm run desktop:build` passed after the current-user Tauri WiX cache was verified, and canonical PID-bound/CDP release `halo-studio.exe` native smoke passed the visible/non-empty/interactive, Halo three-column workbench, workspace-open, command-preview, and terminal-navigation checks. Ticket 04 remains out of scope.

## 当前判定

Halo 正式入口、Halo scope、首屏静态结构、范围外路由裁剪、03a vendor lock path、VS x64 locked/offline Cargo validation、packaged `desktop:build`, and canonical PID-bound/CDP real native release-window smoke now all have passing evidence. HTTP smoke remains auxiliary only and is not used as native Tauri acceptance.

## 锁定依赖与缓存

| 来源 | 锁定 rev | Cargo.lock 中的包 | 当前缓存证据 |
| --- | --- | --- | --- |
| `https://github.com/tauri-apps/tao.git` | `c704261c519c58cfdd0bc2d58ba24e06a0b71c92` | `tao 0.35.3`、`tao-macros 0.1.3` | Git DB `C:\Users\Nyzee\.cargo\git\db\tao-acc866d3b4940d67` 的 `git cat-file -e ...^{commit}` 退出码 `0` |
| `https://github.com/tauri-apps/tauri.git` | `ce3860e84b79af0d5ee628b304399499a87328b1` | `tauri-runtime 2.11.3`、`tauri-runtime-wry 2.11.4`、`tauri-utils 2.9.3` | Git DB `C:\Users\Nyzee\.cargo\git\db\tauri-69fbbe4d0942e697` 的 `git cat-file -e ...^{commit}` 退出码 `1`；对应 checkout `C:\Users\Nyzee\.cargo\git\checkouts\tauri-69fbbe4d0942e697\ce3860e` 不存在 |

锁定来源位置：`product/Halo Studio/Cargo.toml` 的 workspace dependency 声明，以及 `product/Halo Studio/Cargo.lock` 的上述 source 行。当前 `Cargo.lock` 没有 `halo-tauri-desktop` 包条目；本轮按约束没有修改或生成锁文件。

## 本轮收口审计（2026-07-29）

- `product/Halo Studio/Cargo.toml` 的 `workspace.members` 已包含 `src/apps/halo-desktop`；未显式设置 `workspace.default-members`，因此默认成员图同样包含 `halo-tauri-desktop`。该包的 manifest 为 `src/apps/halo-desktop/Cargo.toml`，包名为 `halo-tauri-desktop`。
- `cargo metadata --locked --no-deps --format-version 1` 退出码 `0`：`workspace_members` 和 `workspace_default_members` 都含有该包，且 manifest path 位于 `product/Halo Studio/src/apps/halo-desktop/Cargo.toml`。这只验证 workspace 拓扑，不解析完整依赖图。
- 已跟踪的 `Cargo.lock` 包含精确 Git source：`tao` 的 `c704261c519c58cfdd0bc2d58ba24e06a0b71c92`，以及 `tauri-runtime`、`tauri-runtime-wry`、`tauri-utils` 共用的 `ce3860e84b79af0d5ee628b304399499a87328b1`；manifest 未设置 branch 或 tag。锁文件没有 `halo-tauri-desktop` 本地包条目，因此尚未把新桌面包纳入锁定图。
- VS x64 下的完整 `cargo metadata --locked --format-version 1` 和 `cargo tree --locked -p halo-tauri-desktop` 均在输出图之前退出码 `1`：Cargo 尝试更新 `https://github.com/tauri-apps/tauri.git`，随后报 `revision ce3860e84b79af0d5ee628b304399499a87328b1 not found`，根因是 `failed to send request: 无法与服务器建立连接`。这不能归类为代码或 pinned rev 无效。
- 一次受控的 `cargo fetch --locked` 在 VS x64 会话中运行约 `1804` 秒后由执行超时终止（退出码 `124`）。没有获得 Tauri checkout 或该 rev；遗留的两个 `cargo` 进程已显式结束，随后确认没有 `cargo`、`git`、`git-remote-https` 或 `rustc` 后台进程。
- Cargo Git cache 中 `tao` 的精确 rev 可由 `git cat-file` 读取；`tauri` 的精确 rev 不存在，`git/checkouts` 中也只有 `tao` checkout。当前网络条件不能只读确认 Tauri 上游是否仍可获取该 rev，所以工单保持 `blocked`，不会替换版本、删除 pin 或手写修改 `Cargo.lock`。

## 构建阻断重试（2026-07-30）

本轮遵循短反馈回路，先区分锁文件、wrapper 与依赖获取三种假设；没有重复运行无诊断价值的长时间 `cargo check`。

| 假设 | 结论 | 证据 |
| --- | --- | --- |
| H1：新增 `halo-tauri-desktop` 后未刷新 `Cargo.lock` | 确认 | `cargo metadata --locked --no-deps --format-version 1` 退出码 `0`，workspace/default members 都包含 `halo-tauri-desktop`；`rg -n '^name = "halo-tauri-desktop"$' Cargo.lock` 退出码 `1`，且 `git diff -- Cargo.lock` 为空。 |
| H2：workspace 拓扑或 pnpm wrapper 使用错误 manifest/workspace | 排除 | `cargo locate-project --workspace` 指向 `product/Halo Studio/Cargo.toml`；`desktop:dev` 与 `desktop:build` 都经 `scripts/halo-tauri.mjs` 在 `src/apps/halo-desktop` 运行强制的 `tauri.conf.json`。该目录、package `halo-tauri-desktop` 与 bin target `halo-studio` 均与 metadata 一致。 |
| H3：精确 Tauri Git rev 在当前环境无法获取 | 未能证实 pin 无效；依赖获取仍受环境超时阻断 | 公开仓库在一次性授权后可达，但 Cargo 未能在时限内取得精确 revision。H1 尚未修复，故不能把 H3 视为“锁文件正确后的单独结论”。 |

从 `Cargo.toml` 读取、未替换的 Git 源和一次性授权后的只读检查如下。`git ls-remote` 的 `HEAD` 检查只证明仓库可达；裸 SHA 未必是远端广告 ref，因此不能单独证明历史 commit 存在，精确 pin 仍需 Cargo/Git fetch 或本地 `cat-file` 验证。

| Git 源 | manifest pin | `git ls-remote --symref <url> HEAD` | 结论 |
| --- | --- | --- | --- |
| `https://github.com/tauri-apps/tao.git` | `c704261c519c58cfdd0bc2d58ba24e06a0b71c92` | 退出码 `0`；`refs/heads/dev`，HEAD `f8722bd3628f52ff637e043a95de2e53f78cbcd3` | 仓库可达，且本地 Git DB 的精确 pin 可由 `git cat-file -e ...^{commit}` 读取。 |
| `https://github.com/tauri-apps/tauri.git` | `ce3860e84b79af0d5ee628b304399499a87328b1` | 退出码 `0`；`refs/heads/dev`，HEAD `872428fe910efe25eeaa959b56adcd9d9a9a2157` | 仓库可达，但这不证明精确历史 pin 已被取得。 |

- `cargo generate-lockfile --offline` 退出码 `1`，错误为 `revspec 'ce3860e84b79af0d5ee628b304399499a87328b1' not found`；因此不能在本地缓存中正常刷新缺失的 package 条目。
- 在当前子进程设置 `CARGO_NET_GIT_FETCH_WITH_CLI=true` 后，未带 `--locked` 的 `cargo metadata --format-version 1` 用于正常 Cargo 解析，而非编译；304 秒后被受控超时终止（退出码 `124`），没有成功输出、没有锁文件写入。超时后的 `git cat-file -e ce3860e84b79af0d5ee628b304399499a87328b1^{commit}` 仍退出码 `1`。
- 已核对并终止该命令启动的两个 `cargo`、四个 `git` 与一个 `git-remote-https` 进程；随后确认无残留。唯一由本轮留下的 `C:\Users\Nyzee\.cargo\git\db\tauri-69fbbe4d0942e697\objects\pack\tmp_pack_ppN3Ay`（36.9 MB）已在无进程后删除。
- 随后 `git diff -- Cargo.lock` 为空，`Cargo.lock` 仍没有 `halo-tauri-desktop` 条目。因锁文件前置条件仍未满足，本轮没有运行 `cargo fetch --locked`、`cargo check --locked -p halo-tauri-desktop`、`cargo build --locked -p halo-tauri-desktop`，也没有启动 `pnpm run desktop:dev` 或 `pnpm run desktop:build`。
- `node scripts/halo-scope.mjs` 退出码 `0`；`node --test scripts/halo-scope.test.mjs` 的 7 个测试全部通过。对正式 Halo wrapper、Tauri app 与 Halo frontend 扫描 `D:\BitFun-main`、旧 desktop、PySide/QML/Electron 均退出码 `1`（无匹配）。
- 本轮结束时 `git diff --check HEAD -- .` 退出码 `0`。

### 精确 SHA Git probe（2026-07-30）

主线在此探针前未启动新的 Cargo 解析；探针使用从 `Cargo.toml` 的三个 Tauri patch 条目读取并相互校验的真实值：

```text
URL: https://github.com/tauri-apps/tauri.git
rev: ce3860e84b79af0d5ee628b304399499a87328b1
git init --bare <unique-temp>/tauri.git
git -C <unique-temp>/tauri.git fetch --no-tags --depth=1 <URL> <rev>
git -C <unique-temp>/tauri.git cat-file -e <rev>^{commit}
```

第一次实现使用的 PowerShell `Start-Process` 受当前会话同时继承 `Path`/`PATH` 键影响，无法可靠启动或读取子进程退出码；因此先前的 `90.315` 秒 / `124` 不是有效 Git 传输结论，且该临时仓库已经删除。

### 已验证的精确 SHA fetch（2026-07-30）

改用 .NET `ProcessStartInfo` 后，在唯一临时 bare repo 中执行：

```text
git -c protocol.version=2 -C <unique-temp>/tauri.git fetch --no-tags --depth=1 --filter=blob:none <URL> <rev>
git -C <unique-temp>/tauri.git cat-file -e <rev>^{commit}
```

结果：初始化、fetch 和 `cat-file` 的退出码均为 `0`，总耗时 `2.102` 秒；fetch 输出为 `<rev> -> FETCH_HEAD`，`cat-file` 验证该对象是 commit。临时 bare repo 已删除，未遗留 Git/Cargo 进程，也没有手工写入 Cargo cache、`Cargo.lock`、全局 Cargo 配置、PATH、注册表或系统环境变量。精确 pin 因此可从当前网络取得；它没有被替换或修改。

### Cargo cache 传输重试（2026-07-30）

随后在 VS x64 环境运行当前子进程生效的 Git CLI transport：

```text
set CARGO_NET_GIT_FETCH_WITH_CLI=true
cargo metadata --format-version 1
```

`rustc` host 为 `x86_64-pc-windows-msvc`，`where link` 的第一项为 Visual Studio `Hostx64\x64\link.exe`。该正常 Cargo 解析在 `480.492` 秒后受控超时（退出码 `124`），stderr 仅到 `Updating git repository 'https://github.com/tauri-apps/tauri.git'`；没有 Rust 编译、没有 `Cargo.lock` diff，`rg '^name = "halo-tauri-desktop"$' Cargo.lock` 仍无匹配。Cargo/Git 进程树已清理。

此结果是 Cargo/Git cache 的依赖传输阻断，不是 `not our ref`、unreachable object、pin 无效或产品代码失败。由于 lockfile 前置条件仍未满足，本轮没有运行 `cargo metadata --locked`、`cargo tree --locked -p halo-tauri-desktop`、`cargo check --locked -p halo-tauri-desktop`、`pnpm run desktop:build` 或原生 Tauri smoke；工单 03 保持 `blocked`。

并行只读审计曾运行 `cargo update --workspace --dry-run --offline`，这不符合“先完成精确 SHA probe 再做 Cargo 解析”的顺序要求。该命令没有写入 `Cargo.lock` 或 Cargo cache，并立即因当前缺失的 `ce3860e84b79af0d5ee628b304399499a87328b1` checkout 失败；其结果未被用作本轮探针或最终验证依据，后续不再执行 Cargo 命令。

## 工具链与 Cargo 命令

授权后的 VS x64 命令如下，未安装软件、未修改全局配置、PATH、注册表或环境变量：

```text
cmd.exe /d /s /c 'call "D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cd /d "D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio" && rustc -vV && where link && cargo fetch --locked && cargo check --locked -p halo-tauri-desktop && cargo build --locked -p halo-tauri-desktop'
```

### Cargo Git Transport Triage (2026-07-30)

Direct Git has already fetched and verified the pinned Tauri commit: `https://github.com/tauri-apps/tauri.git` at `ce3860e84b79af0d5ee628b304399499a87328b1`, with `git cat-file -e <rev>^{commit}` exiting `0`. Cargo remains the blocked path.

All probes used the VS x64 environment, `cargo metadata --format-version 1 -vv`, and a 120-second process-tree timeout. Output was restricted to the public Git URL, stage, error class, and exit code.

| Transport / cache | Result |
| --- | --- |
| `CARGO_NET_GIT_FETCH_WITH_CLI=true` | Exit `124` after `120.324 s`; timeout at `Updating git repository https://github.com/tauri-apps/tauri.git`. |
| `CARGO_NET_GIT_FETCH_WITH_CLI=false` | Exit `124` after `120.413 s`; timeout at the same Git update stage. |
| CLI transport with `CARGO_HOME=D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\.cargo-transport-probe-20260730-019fb08b` | Exit `124` after `120.296 s`; timeout at the same Git update stage. The temporary directory was removed. |

`cargo fetch -vv` was not run after either `metadata -vv` timeout because it would repeat the same already-red pre-resolution Git update path without changing a diagnostic variable. No probe reached dependency resolution, so there was no lockfile change to inspect and no basis to run locked metadata/tree/check, `desktop:build`, or native-window smoke.

Post-probe cleanup: the temporary `CARGO_HOME` does not exist, no visible `cargo` or `git` process remains, `git diff -- Cargo.lock` is empty, `Cargo.lock` still lacks `name = "halo-tauri-desktop"`, and `git diff --check` exits `0`.

Classification: **Cargo Git transport/cache-path blockage**. This is not evidence that the pinned revision is invalid or unreachable, and it is not product-code or native-window success evidence. Ticket 03 remains **BLOCKED**.

### 03a Recommended Path A Execution (2026-07-30)

With one-time authorization for the public sources recorded in Cargo.toml, a single extended resolver attempt ran in the VS x64 environment: rustc -vV, where link, then cargo metadata --format-version 1 -vv.

The child process used only CARGO_NET_GIT_FETCH_WITH_CLI=true, GIT_TERMINAL_PROMPT=0, and CARGO_TERM_COLOR=never; these were process-local and were not written to Cargo configuration, PATH, the registry, or system environment variables. Rust reported host x86_64-pc-windows-msvc, and the first where link result was Visual Studio Hostx64\x64\link.exe.

Cargo still stopped at Updating git repository https://github.com/tauri-apps/tauri.git and reached the one controlled 900 s timeout after 900.415 s (normalized exit 124). It emitted no resolution success, Cargo.lock remained unchanged, halo-tauri-desktop remained absent from the lockfile, and no cargo or git process remained after cleanup. git diff --check exited 0.

This changed only the timeout window from the prior 120-second transport probes; it did not provide a different proxy or network route. It therefore closes recommended path A in the current environment as a repeated Cargo Git transport/cache-path timeout. Do not run the same long resolver again. Tickets 03a and 03 remain **BLOCKED**; B or C requires an explicit new user decision, and Ticket 04 remains out of scope.

### 03a Selected Path B Preflight (2026-07-30)

The user selected the audited vendor strategy. The intended tracked artifacts are `product/Halo Studio/Cargo.lock`, `product/Halo Studio/vendor/cargo/`, `product/Halo Studio/.cargo/config.toml`, and a 03a vendor audit record under `docs/requirements/bitfun-tauri-product-migration/`.

This worktree currently has no `product/Halo Studio/.cargo/config.toml` and no Cargo vendor directory. Because B must be generated by Cargo rather than hand-built, the current machine first needs a successful Cargo lock graph for the existing exact pins. The prior online Cargo path is already red after a 900-second timeout at `Updating git repository https://github.com/tauri-apps/tauri.git`.

A short, no-network cache probe was run from `product/Halo Studio`:

```text
cargo metadata --locked --offline --format-version 1
```

Result: exit `1` after about `1.3 s`. Cargo stopped before validation because the local offline registry/cache is incomplete: `no matching package named objc2-core-foundation found`, required through `arboard v3.6.1` and the locked `bitfun-cli` graph. This confirms the current machine cannot produce the lock/vendor artifacts offline, and the existing online Cargo path cannot produce them either. No source files, `Cargo.lock`, Cargo cache configuration, vendor directory, global Cargo config, PATH, registry, or system environment were modified.

B therefore requires a trusted external environment that can resolve the current `product/Halo Studio` workspace with the existing `tao` and `tauri` pins, run `cargo vendor --locked vendor/cargo`, and return only Cargo-generated artifacts for audit/import. Ticket 03a remains **BLOCKED** until those artifacts are available; Ticket 03 remains **BLOCKED**, and Ticket 04 remains out of scope.

### 03a Selected Path B Verification (2026-07-30)

External Cargo-generated artifacts were returned into the current worktree: `product/Halo Studio/Cargo.lock`, `product/Halo Studio/.cargo/config.toml`, and `product/Halo Studio/vendor/cargo/`. The public summary retained the exact Tauri pin `https://github.com/tauri-apps/tauri.git` at `ce3860e84b79af0d5ee628b304399499a87328b1` and the exact Tao pin `https://github.com/tauri-apps/tao.git` at `c704261c519c58cfdd0bc2d58ba24e06a0b71c92`; no commit, push, lockfile hand-edit, or pin replacement occurred.

Local audit results:

- `.cargo/config.toml` uses `directory = "vendor/cargo"` and contains no external absolute path.
- `Cargo.lock` contains `halo-tauri-desktop v0.2.14`.
- `git diff -- product/Halo Studio/Cargo.lock` only adds the `halo-tauri-desktop` package block with dependencies on `tauri` and `tauri-build`.
- `vendor/cargo` contains 1091 crate directories and 1091 `.cargo-checksum.json` files.
- A script verified 56768 vendor files against `.cargo-checksum.json`; missing checksum files, missing listed files, hash mismatches, and extra unlisted files were all `0`.
- The external license/copyright inventory pattern returns 1777 entries: 1762 files and 15 directories. Every vendor crate has either a Cargo `license`/`license-file` field or a top-level license-like file.

The first `cargo check --locked --offline -p halo-tauri-desktop` reached real compilation and failed with exit `101` because Halo's Tauri config did not declare `app.macOSPrivateApi` while the workspace `tauri` dependency enables the `macos-private-api` feature. This was classified as a code/config mismatch, not a vendor or transport failure. The fix was to add `app.macOSPrivateApi: true` to `product/Halo Studio/src/apps/halo-desktop/tauri.conf.json`, matching the existing BitFun desktop Tauri config.

After that fix, the VS x64 offline locked validation passed:

| Command | Result |
| --- | --- |
| `cargo metadata --locked --offline --format-version 1` | exit `0`; `rustc` host `x86_64-pc-windows-msvc`, first `where link` result Visual Studio `Hostx64\x64\link.exe` |
| `cargo tree --locked --offline -p halo-tauri-desktop` | exit `0`; tree includes retained Tauri and Tao Git SHAs |
| `cargo check --locked --offline -p halo-tauri-desktop` | exit `0` |
| `cargo build --locked --offline -p halo-tauri-desktop` | exit `0` |
| `git diff --check HEAD -- .` | exit `0` |

Ticket 03a is now **ready-for-review** and no longer blocks Ticket 03's desktop build and native-window acceptance sequence. Ticket 04 remains out of scope.

### Ticket 03 Desktop Build and Native Smoke Resume (2026-07-30)

After 03a passed, Ticket 03 validation resumed.

`pnpm run desktop:build` was run from `product/Halo Studio` in the VS x64 environment through the Halo wrapper. It used the same Halo scope and Tauri config path as the static wrapper audit:

```text
pnpm run desktop:build
```

Result: exit `1`. The Halo frontend built successfully, and Tauri compiled the release executable:

```text
Built application at: D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\target\release\halo-studio.exe
```

The failure occurred after Rust compilation, during Tauri bundling:

```text
Info Verifying wix package
Downloading https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip
failed to bundle project ... os error 10013
```

Classification: packaging environment/tool dependency blockage. The product Rust build and vendor ingestion are no longer the failing phase; the remaining `desktop:build` failure is Tauri bundler's attempt to obtain WiX in an environment that denies the socket operation. No global software was installed, no PATH/registry/global Cargo config was modified, and the failure cannot be reported as a passing packaged build.

A real native smoke was then run against the compiled release executable as auxiliary native-window evidence:

```text
target\release\halo-studio.exe
```

Result:

- Process launched and stayed alive through interaction.
- Native window title: `Halo Studio - 编码工作台`.
- Window visible: `true`.
- Window size: `1454 x 938`.
- Screenshots saved:
  - `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-before.png`
  - `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-after.png`
- Screenshot color sampling showed a non-empty window (`95` distinct sampled colors before, `99` after).
- Calibrated native clicks/keystrokes changed `75` of `7081` sampled pixels, and the process remained alive, proving the window was responsive to input.
- Visual inspection confirms a real Halo Tauri workbench window with Halo branding, left navigation/file list, and local editor content.

Limitations: this native smoke does not satisfy full Ticket 03 acceptance. In the current desktop capture, the right status sidebar is not visible, apparently because the native WebView/layout is under the responsive threshold at the active display scale. The coordinate-based smoke did not reliably prove the full open-workspace plus command-preview flow. Therefore this is native window evidence, but not complete native acceptance. Ticket 03 remains **BLOCKED**.

### Current-Turn Local Reverification (2026-07-30)

The returned 03a vendor artifacts were re-audited in this turn and still satisfy the Cargo dependency-ingestion acceptance criteria:

- `product/Halo Studio/.cargo/config.toml` exists and uses only `directory = "vendor/cargo"` for `source.vendored-sources`; no external absolute path is referenced.
- `Cargo.lock` contains `halo-tauri-desktop v0.2.14`.
- `git diff -- product/Halo Studio/Cargo.lock` only adds the `halo-tauri-desktop` package block with dependencies on `tauri` and `tauri-build`.
- Tauri remains pinned to `https://github.com/tauri-apps/tauri.git` at `ce3860e84b79af0d5ee628b304399499a87328b1`; Tao remains pinned to `https://github.com/tauri-apps/tao.git` at `c704261c519c58cfdd0bc2d58ba24e06a0b71c92`.
- `vendor/cargo` contains 1091 top-level package directories and 1091 `.cargo-checksum.json` files. Every top-level package directory has a checksum file, all checksum JSON files parse with a `files` map, and a deterministic 20-package SHA-256 sample had zero mismatches.
- A strict local filename recount found 1762 license/copyright files; the returned full audit's broader pattern counts 1777 entries as 1762 files plus 15 directories.

The VS x64 offline Cargo commands were rerun locally:

| Command | Result |
| --- | --- |
| `cargo metadata --locked --offline --format-version 1` | exit `0`; `rustc` host `x86_64-pc-windows-msvc`, first `where link` result Visual Studio `Hostx64\x64\link.exe` |
| `cargo tree --locked --offline -p halo-tauri-desktop` | exit `0` |
| `cargo check --locked --offline -p halo-tauri-desktop` | exit `0` |
| `cargo build --locked --offline -p halo-tauri-desktop` | exit `0` |
| clean temporary `CARGO_HOME` + `cargo metadata --locked --offline --format-version 1` | exit `0`; temporary home removed |
| `git diff --check HEAD -- .` | exit `0` |

Ticket 03a remains **ready-for-review**.

Ticket 03 was resumed in this turn. `pnpm run desktop:build` in VS x64 again built the Halo frontend and compiled `target\release\halo-studio.exe`, then failed in Tauri bundling while verifying/downloading WiX:

```text
Info Verifying wix package
Downloading https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip
failed to bundle project ... os error 10013
```

The normal run exited `1`. A one-time outside-sandbox rerun was attempted to distinguish sandbox/network denial from product failure, but it timed out after `604` seconds without a completed packaged build; the leftover build processes from that attempt were cleaned up. This is classified as a Windows packaging tool/network environment blockage. The release executable exists, but `pnpm run desktop:build` has not passed.

A fresh native smoke was run against the locally compiled release executable as auxiliary evidence:

- Executable: `product/Halo Studio/target/release/halo-studio.exe`
- Native window title: `Halo Studio - 编码工作台`
- Window visible: `true`
- Window size: `1454 x 938`
- Screenshots:
  - `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-local-before.png`
  - `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-local-after.png`
- Distinct sampled colors: `103` before, `102` after.
- Sampled pixel changes after native click/key input: `9` of `6643`.
- The process stayed alive after interaction and was closed after the smoke.
- Visual inspection confirmed a real Halo Tauri workbench window with Halo branding, local coding navigation, a workspace file list, and editor content.

This smoke proves a real visible, non-empty, input-responsive native Halo Tauri window, but it does **not** satisfy full Ticket 03 acceptance because `desktop:build` still fails and the complete existing-workspace, command-preview, and terminal-navigation flow was not reliably proven. Ticket 03 remains **BLOCKED**; Ticket 04 remains out of scope.

### Ticket 03 WiX Cache Unblock, Packaged Build, and Native Acceptance (2026-07-30)

This section supersedes the earlier WiX `os error 10013` and partial native-smoke blockage records while preserving them above for audit history.

#### WiX bundler toolchain diagnosis

- Tauri CLI package used by `pnpm run desktop:build`: `@tauri-apps/cli` `2.10.0`; bundled Rust `tauri-bundler` string: `2.8.0`.
- WiX source URL expected by Tauri bundler: `https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip`.
- WiX SHA-256 expected by Tauri bundler and verified locally: `6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31`.
- Current-user Tauri WiX cache: `C:\Users\Nyzee\AppData\Local\tauri\WixTools314`.
- Required files verified in that cache: `candle.exe`, `candle.exe.config`, `darice.cub`, `light.exe`, `light.exe.config`, `wconsole.dll`, `winterop.dll`, `wix.dll`, `WixUIExtension.dll`, `WixUtilExtension.dll`; `heat.exe` is also present.
- No global software was installed; system PATH, registry, global Cargo config, and system environment variables were not modified.

Because `bundle.targets` remains `"all"`, the successful packaging path also created/used Tauri's current-user NSIS cache at `C:\Users\Nyzee\AppData\Local\tauri\NSIS` while producing the NSIS installer. This was a Tauri bundler current-user tool-cache side effect, not a global install.

#### Packaged desktop build

Command:

```text
cmd.exe /d /s /c """D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"" -arch=x64 -host_arch=x64 && cd /d ""D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio"" && where link && pnpm run desktop:build"
```

Result: exit `0` after about `212` seconds.

Toolchain evidence:

```text
D:\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe
D:\MSYS2\usr\bin\link.exe
```

The first `where link` result is the required Visual Studio Hostx64/x64 linker.

Relevant output:

```text
[halo-tauri] build src\halo-workbench\index.html -> src\apps\halo-desktop\Cargo.toml
[halo-workbench] built D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\src\halo-workbench\dist
Finished `release` profile [optimized] target(s) in 1m 37s
Built application at: D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\target\release\halo-studio.exe
Running candle for "D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\target\release\wix\x64\main.wxs"
Running light to produce D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\target\release\bundle\msi\Halo Studio_0.1.0_x64_en-US.msi
Running makensis to produce D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\target\release\bundle\nsis\Halo Studio_0.1.0_x64-setup.exe
Finished 2 bundles
```

Generated artifacts:

- `product/Halo Studio/target/release/halo-studio.exe`
- `product/Halo Studio/target/release/bundle/msi/Halo Studio_0.1.0_x64_en-US.msi`
- `product/Halo Studio/target/release/bundle/msi/Halo Studio_0.1.0_x64_en-US.wixpdb`
- `product/Halo Studio/target/release/bundle/nsis/Halo Studio_0.1.0_x64-setup.exe`

#### Real native release-window smoke

Smoke command: launch `product/Halo Studio/target/release/halo-studio.exe` as a real Windows process with a current-process-only WebView2 user-data directory and remote-debugging port, bind strictly to the launched PID/path/window handle, then drive the native Tauri WebView DOM through CDP and close the process after capture. This does not use the HTTP smoke script and does not count an HTTP page as native acceptance.

Result: exit `0`.

Checks:

| Check | Result |
| --- | --- |
| real native window visible | `true`; launched PID `9064`, process path exactly `D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio\target\release\halo-studio.exe`, window handle `0x6C01F0`, foreground handle `0x6C01F0`, title `Halo Studio - 编码工作台`, size `1454x938` |
| non-empty window | `true`; native before/after screenshots and CDP page screenshot are non-empty and show the Halo local coding workbench |
| Halo scope/product | `true`; CDP summary reports `scope=local-coding`, `product=halo-studio`, `url=http://tauri.localhost/` |
| Halo three-column workbench | `true`; CDP summary reports `.shell`, `.sidebar.sidebar--left`, `.main-panel`, and `.sidebar.sidebar--right` all present |
| workspace opened | `true`; CDP interaction changed the workspace state to `halo-workspace` / `workspaceOpened=true` |
| terminal navigation | `true`; CDP interaction changed active nav to `terminal` |
| command preview | `true`; CDP interaction entered `pnpm run desktop:build` and terminal output contains that command (`terminalHasCommand=true`) |

Screenshots:

- `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-pid-bound-20260730-182708-before.png`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-pid-bound-20260730-182708-after.png`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-pid-bound-20260730-182708-cdp-after.png`

Structured evidence:

- `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-pid-bound-20260730-182708.json`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-pid-bound-20260730-182708-cdp-summary.json`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03-native-smoke-pid-bound-20260730-182708-cdp.json`

Earlier same-title/native capture confusion from an unrelated foreground game/window is superseded by this PID-bound run. The canonical evidence binds to the launched release executable path, confirms the foreground window handle, and verifies the Halo DOM through the launched WebView2 instance.

Additional re-sample after the unrelated game window was closed: `03-native-smoke-pid-bound-game-closed-settled.png` shows the PID-bound native Halo three-column workbench, and `03-native-smoke-cdp-after-interaction.png` shows the CDP-driven interaction state with `scope=local-coding`, `product=halo-studio`, workspace `halo-workspace`, active terminal navigation, and terminal output `$ echo halo-smoke`.

The earlier HTTP smoke remains auxiliary only (`tauriWindow:false`) and is not used as native acceptance evidence.

#### Final hygiene

- `git diff --check` passed after the packaged build evidence was captured.
- No files were committed or pushed.
- `D:\BitFun-main`, the main workspace, system PATH, registry, global Cargo config, and system environment variables were not modified.
- Ticket 04 was not entered.

Ticket 03 is now **ready-for-review**.

结果：退出码 `124`，约 `1204` 秒后超时。`rustc` 的 host 为 `x86_64-pc-windows-msvc`；`where link` 第一项为 `D:\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe`。阻断归类为公开 Git/Cargo 依赖获取的网络/环境超时，不是构建通过。

另一次此前的在线 `cargo check --locked -p halo-tauri-desktop` 运行约 `604` 秒后退出码 `124`，同样未形成构建证据。

本轮的精确解析结果：

| 命令 | 退出码 | 结果与归类 |
| --- | ---: | --- |
| `cargo metadata --locked --format-version 1 --no-deps` | `0` | 只证明 workspace manifest 可解析，不证明 Git 依赖或 crate 源码可用 |
| `cargo metadata --locked --format-version 1` | `124` | 124 秒超时，完整依赖解析仍等待网络获取 |
| `cargo metadata --locked --offline --format-version 1` | `1` | `failed to load source for dependency tauri-runtime`；offline 无法 checkout Tauri rev |
| `cargo check --locked --offline -p halo-tauri-desktop` | `1` | 同上；`can't checkout from 'https://github.com/tauri-apps/tauri.git': you are in the offline mode (--offline)` |
| `rg -n 'name = "halo-tauri-desktop"' Cargo.lock` | `1`（无匹配） | 当前锁文件没有新桌面包条目；按约束未私自更新 |

## Historical formal-entry and scope evidence (pre-03a/WiX unblock)

静态入口映射：

- 开发入口：`pnpm run desktop:dev` -> `node scripts/halo-tauri.mjs dev`。
- 打包入口：`pnpm run desktop:build` -> `node scripts/halo-tauri.mjs build`。
- 调试预览入口：`pnpm run desktop:preview:debug` -> `node scripts/halo-workbench-preview.mjs`；它只启动现有的 `halo-studio` debug 二进制和 Halo dev server，不会运行 `tauri dev` 或自动重编译 Rust。仅显式传入 `--force-rebuild` 时才会调用 `cargo build --locked -p halo-tauri-desktop`。
- 两者都强制读取同一个 `halo-scope.json` 和 `src/apps/halo-desktop/tauri.conf.json`，Tauri frontend dist 指向 `src/halo-workbench/dist`。
- Halo scope 纳入 `local-workspaces`、`coding-sessions`、`file-explorer`、`editor`、`git`、`terminal`；排除 `office-collaboration`、`mini-app`、`remote-workspace`、`relay`、`mobile-client`，并关闭对应 runtime policy。

通过的静态命令：

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `node scripts/halo-scope.mjs` | `0` | Halo product/config/frontend scope 通过 |
| `node --test scripts/halo-scope.test.mjs` | `0` | 7 个测试通过，包含 debug preview 不得退回 `tauri dev` 的约束 |
| `node --check scripts/halo-tauri.mjs` 等 4 个 Halo 脚本 | `0` | 语法通过 |
| `node scripts/halo-workbench-smoke.mjs` | `0` | HTTP 200；输出 `tauriWindow:false`，仅为辅助证据 |
| `rg` 对正式 Halo wrapper、Tauri app、Halo frontend 扫描 `D:\BitFun-main`、旧 desktop、PySide/QML/Electron | `1`（无匹配） | 正式运行入口没有这些引用 |
| `rg -n -F 'D:\BitFun-main' product/Halo Studio --glob '!target/**' --glob '!node_modules/**'` | `1`（无匹配） | 产品树没有该绝对路径字面量；scope 检查使用分段拒绝令牌 |
| `git diff --check HEAD -- .` | `0` | 已跟踪改动无空白错误 |
| 未跟踪文本文件尾随空白扫描 | `0` | 未发现尾随空白 |

完整 `package.json` 扫描仍会看到旧的 `copy-icons` 辅助脚本引用 `src/apps/desktop/icons/Logo-ICON.png`；它不在 `dev`、`build`、`desktop:dev` 或 `desktop:build` 调用链中，Halo 正式入口扫描未命中该路径。旧源码、QML、Python 和 Sidecar 均未删除。

## Historical native Tauri smoke (pre-03a/WiX unblock; superseded)

未通过，且没有伪造通过：

- `target` 下没有由本轮构建产生的 `halo-studio.exe`。
- 没有可记录的 Halo Tauri 进程、非零原生窗口句柄、窗口标题 `Halo Studio - 编码工作台`、可见性或交互证据。
- `pnpm run desktop:dev` 与 `pnpm run desktop:build` 的启动尝试未完成到可验收退出；前者会启动长驻的 Halo frontend dev server，随后在 Cargo/Tauri 构建阶段受同一依赖阻断。`desktop:preview:debug` 的入口语义已静态验证，但因没有 `halo-studio.exe` 尚不能启动。为避免无锁构建继续挂起和遗留后台进程，本轮停止了探测，未将其计为通过。
- `node scripts/halo-workbench-smoke.mjs` 的输出明确为 `tauriWindow:false`，不能替代原生窗口验收。

因此，开发入口和安装包入口虽然静态地指向同一 Halo 产品裁剪规则，但本轮没有 `desktop:build` 成功产物，也没有原生窗口 smoke，工单 03 保持 `blocked`。

## Historical boundary note (pre-03a/WiX unblock; superseded where noted)

本轮没有修改 `D:\BitFun-main`、主工作区、`Cargo.lock` 或系统配置，没有提交或推送，没有关闭 GitHub Issue，也没有进入工单 04 的 Runtime 契约、真实 OpenCode 会话或工单 14 UI 验收。

## Final boundary/status after WiX unblock (2026-07-30)

- Ticket 03 is `ready-for-review`.
- Ticket 03a remains `ready-for-review`.
- Ticket 04 was not entered.
- No commit or push was performed.
- `D:\BitFun-main`, the main workspace, system PATH, registry, global Cargo config, and system environment variables were not modified.
- Current worktree changes include the Ticket 03/03a source, docs, `Cargo.lock`, workspace-local `.cargo/config.toml`, and `vendor/cargo` artifacts described above.
