# 03 - 启动 Halo 品牌的 Tauri 开发工作台

**2026-07-30 Cargo Git transport triage:** Direct Git can fetch and verify the pinned Tauri commit, while Cargo `metadata -vv` times out at `Updating git repository https://github.com/tauri-apps/tauri.git` with both CLI and built-in Git transports. A worktree-local temporary `CARGO_HOME` reproduces the same timeout and was removed. `Cargo.lock` remains unchanged and lacks `halo-tauri-desktop`; Ticket 03 remains `blocked` pending a Cargo Git transport/cache-path remedy or an environment access change. See the execution evidence for the controlled commands and exit codes.

**Resolved blocker:** [03a - Resolve Tauri Cargo Git Dependency Ingestion](03a-resolve-tauri-cargo-git-dependency-ingestion.md) is `ready-for-review` and provides the audited Cargo lock/vendor path used by this ticket.

**2026-07-30 03a recommended-path A result:** One authorized 900-second VS x64 Cargo resolver attempt with the existing pins, CARGO_NET_GIT_FETCH_WITH_CLI=true, and non-interactive Git still timed out at Updating git repository https://github.com/tauri-apps/tauri.git (900.415 s, exit 124). The toolchain was correct, but Cargo.lock stayed unchanged and no Cargo/Git process remained. Do not run locked validation, desktop build, or native-window smoke until 03a is actually unblocked; Ticket 03 remains blocked.

**2026-07-30 03a selected-path B preflight:** The audited vendor strategy is selected, but this machine cannot generate the required Cargo artifacts through Cargo's normal flow: online Cargo still times out at the Tauri Git update stage, and a short offline locked metadata probe exits `1` because the local cache lacks `objc2-core-foundation`. No vendor directory, `.cargo/config.toml`, or `Cargo.lock` change was created. Ticket 03 remains blocked pending externally generated, auditable Cargo lock/vendor artifacts for the existing pins.

**2026-07-30 03a vendor result and 03 resume:** 03a is now `ready-for-review`: the returned Cargo-generated vendor artifacts were audited, offline locked metadata/tree/check/build passed in VS x64, and `Cargo.lock` only adds `halo-tauri-desktop`. Ticket 03 resumed, but remains `blocked`: `pnpm run desktop:build` compiled `target/release/halo-studio.exe` and then failed while Tauri bundler tried to download WiX (`wix314-binaries.zip`, os error 10013). A real native `halo-studio.exe` smoke produced a visible, non-empty, interactive Halo Tauri window, but current native evidence does not prove the full three-column acceptance or command/workspace flow; see the execution evidence and screenshots.

**2026-07-30 current-turn recheck:** Local VS x64 offline Cargo `metadata`, `tree`, `check`, and `build` still pass against the returned vendor artifacts, including a clean temporary `CARGO_HOME` metadata probe. `pnpm run desktop:build` still fails at the Windows WiX download/verification bundling step; a one-time outside-sandbox rerun timed out after 604 seconds and was cleaned up. Fresh native release-exe smoke again showed a real visible, non-empty, input-responsive Halo Tauri window, but Ticket 03 remains `blocked` until packaged `desktop:build` and the full native interaction acceptance pass.

**2026-07-30 WiX cache unblock and 03 acceptance:** The Windows packaging blocker is resolved for this worktree. Tauri bundler's WiX requirement was confirmed as `wix314-binaries.zip` from `https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip`, SHA-256 `6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31`, extracted under the current-user Tauri cache `C:\Users\Nyzee\AppData\Local\tauri\WixTools314`. `pnpm run desktop:build` now exits `0` in the VS x64 environment and emits both MSI and NSIS bundles. A canonical PID-bound release `halo-studio.exe` native smoke (`03-native-smoke-pid-bound-20260730-182708-*`) confirms a visible, non-empty, interactive Tauri window, Halo three-column workbench, workspace-open state, command preview, and terminal navigation. Ticket 03 is now `ready-for-review`; Ticket 04 remains out of scope.

**What to build:** 本地开发者从仓库的正式桌面入口启动后，直接进入 Halo 品牌的 BitFun 三栏开发工作台，并且首期范围外能力不会出现在导航或后台初始化中。

**Blocked by:** 02 - 固定并纳入可审计的 BitFun 上游基线.

**Status:** ready-for-review（2026-07-30：03a vendor lock/cargo path passed; VS x64 `cargo metadata/tree/check/build --locked --offline` passed; `pnpm run desktop:build` passed after the current-user Tauri WiX cache was verified; canonical PID-bound/CDP native `halo-studio.exe` smoke passed for visible/non-empty/interactive window, Halo three-column workbench, workspace open, command preview, and terminal navigation. See [工单 03 执行证据](../03-tauri-workbench-execution-evidence.md).）

**2026-07-30 构建阻断重试：** H1 已确认：`halo-tauri-desktop` 是 workspace member，但尚未进入 `Cargo.lock`。H2 已排除：pnpm/Tauri wrapper、Cargo workspace、Halo package 与 target 一致。一次性授权后的真实 Tauri/Tao 仓库可达性检查均成功，但 Cargo 对保留的精确 Tauri pin 进行正常解析时在 304 秒后超时，未写入锁文件；超时子进程及本轮产生的临时 Git pack 均已清理。没有启动 `cargo check`、`cargo build`、`desktop:dev`、`desktop:build` 或原生窗口验收，工单继续保持 `blocked`；详见[工单 03 执行证据](../03-tauri-workbench-execution-evidence.md)。

**2026-07-30 精确 SHA 与 Cargo cache 重试：** 从 `Cargo.toml` 读取的 `https://github.com/tauri-apps/tauri.git` / `ce3860e84b79af0d5ee628b304399499a87328b1` 已在唯一临时 bare repo 中以 `--no-tags --depth=1 --filter=blob:none` 成功 fetch（退出码 `0`，2.102 秒），且 `git cat-file -e <rev>^{commit}` 退出码 `0`；临时 repo 已删除且没有遗留 Git/Cargo 进程。随后在 VS x64 与仅当前子进程生效的 `CARGO_NET_GIT_FETCH_WITH_CLI=true` 下运行正常 `cargo metadata --format-version 1`，但 Cargo 获取自己的 Git cache 在 `480.492` 秒后受控超时（退出码 `124`），停在 `Updating git repository`，没有写入 `Cargo.lock`。因此精确 pin 可取得，当前阻断是 Cargo/Git cache 的依赖传输，不是 pin 或产品代码失败；工单继续保持 `blocked`。

- [x] 开发与打包入口均启动同一个 Halo Tauri 产品，而不是旧 PySide/QML 应用或外部参考树。
- [x] 首屏保留适合本地编码的 BitFun 三栏工作台、关键导航和高密度交互骨架。
- [x] 产品名称、图标、视觉令牌和简体中文核心文案使用 Halo 品牌。
- [x] 办公协作、Mini App、远程、Relay、移动端等范围外模块不进入构建路由、导航、配置入口或启动初始化。
- [x] 桌面烟测证明窗口可见、非空、可交互，开发构建与安装包使用同一产品裁剪规则。
