# 工单 01 执行证据：可迁移能力基线与仓库卫生

**状态：** READY-FOR-REVIEW（合格非沙箱 Windows 用户会话中的 MSVC、workspace check/build/test、Python/QML、Schema 和凭据清理验证均通过）

**执行日期：** 2026-07-29

**源代码基线：** `origin/main` / `75bd4d294ef230706ba39129124ae350e8dacda0`

**隔离 worktree：** `D:\Halo Studio\.worktrees\issue-01-baseline`

**分支：** `codex/issue-01-baseline`

## 判定摘要

| 工单 01 验收项 | 当前判定 | 证据与原因 |
| --- | --- | --- |
| MSVC workspace 检查、构建、Rust、Python/QML、Schema 命令与结果 | 满足 | `VsDevCmd.bat`、MSVC linker、`cl.exe`、Windows SDK `rc.exe` 和 target 均正确；在合格非沙箱 Windows 用户会话中 `cargo check --workspace`、`cargo build --workspace`、`cargo test --workspace` 均退出码 0，Python 为 `154 passed, 1 skipped`，QML smoke 和 Schema 结构检查也退出码 0。 |
| 旧六票的产品定位 | 满足 | GitHub #9–#14 明确记录为旧产品上的可迁移能力基线，不是目标 Tauri 产品验收或 P0 放行。 |
| 真实 OpenCode 原生 UI 门槛 | 满足 | 目标 Tauri 产品未执行真实 UI 验收；历史 OpenCode 版本阻断和资格验证限制保留在旧基线记录中。 |
| 临时产物与用户资产卫生 | 满足（受保护 cache 保留） | 删除了隔离 worktree 中本轮创建的 pytest 临时目录、Python `__pycache__` 和 Cargo 生成的 `sidecar\\target`；`app\\.pytest_cache` 的精确删除在受限 ACL 下返回拒绝访问，因此保留并记录，不修改权限；主工作区的用户改动和候选缓存未触碰。 |
| 独立审查边界 | 满足，提交待授权 | 代码、测试和状态证据均位于从 `origin/main` 建立的独立 worktree，当前差分未吸收主工作区改动，也未混入工单 02/迁移实现；本轮不提交，独立 Git 提交边界等待用户授权。 |

工单 01 验收条件已满足，可进入 `ready-for-review`；本轮仍不能进入工单 02。

## 可复现命令记录

以下命令均在隔离 worktree 根目录执行。输出只记录可审计的状态和汇总，不记录凭据、Authorization、端口、完整对话或 session/message 标识。

### 1. Workspace 检查

命令：

```powershell
git rev-parse --show-toplevel
git rev-parse --abbrev-ref HEAD
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=all
git diff --check
Test-Path -LiteralPath 'product'
```

结果：

```text
D:/Halo Studio/.worktrees/issue-01-baseline
codex/issue-01-baseline
75bd4d294ef230706ba39129124ae350e8dacda0
git status：仅有本轮工单、证据和基线测试/实现差分；未吸收主工作区改动，当前仍未提交
git diff --check：通过
False（当前 origin/main 尚未包含 product/；工单 01 不提前实施产品迁移）
```

### 2. MSVC 环境与 Rust 验证

用户要求的命令主体是下面这条 `cmd.exe` 子进程命令；它的 `/c` 内容按原样保留，MSVC 环境只应存在于该子进程：

```cmd
cmd.exe /d /s /c """D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cd /d "D:\Halo Studio\.worktrees\issue-01-baseline\sidecar" && echo === TOOLCHAIN === && rustc -vV && cargo -V && echo === LINKER === && where link && where cl && where rc && echo === CHECK === && cargo check --workspace && echo === BUILD === && cargo build --workspace && echo === TEST === && cargo test --workspace"
```

在 PowerShell 中，为保持批处理文件的调用语义，实际传输给 `cmd.exe` 的等价形式显式使用 `call`；未修改持久 PATH、注册表、全局环境变量或 Cargo 配置：

```powershell
$cmdLine = 'call "D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cd /d "D:\Halo Studio\.worktrees\issue-01-baseline\sidecar" && echo === TOOLCHAIN === && rustc -vV && cargo -V && echo === LINKER === && where link && where cl && where rc && echo === CHECK === && cargo check --workspace && echo === BUILD === && cargo build --workspace && echo === TEST === && cargo test --workspace'
& cmd.exe /d /s /c $cmdLine
$code = $LASTEXITCODE
Write-Output "OVERALL_CMD_EXIT=$code"
```

结果：

```text
Visual Studio 2022 Developer Command Prompt v17.14.37
rustc 1.95.0 (59807616e 2026-04-14)
host: x86_64-pc-windows-msvc
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
D:\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe
D:\MSYS2\usr\bin\link.exe
D:\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\cl.exe
C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\rc.exe
=== CHECK ===
=== BUILD ===
=== TEST ===
...
OVERALL_CMD_EXIT=0
```

工具链判定：通过。`rustc host` 满足 `x86_64-pc-windows-msvc`；`where link` 第一项是 Visual Studio `Hostx64\x64\link.exe`，MSYS2 linker 仅为第二项；`where cl` 和 `where rc` 也指向 Visual Studio/Windows SDK。没有修改持久 PATH、注册表、全局环境变量或 Cargo 配置。

阶段结果：

组合命令完成了 `CHECK`、`BUILD` 和 `TEST` 三个阶段，整体退出码为 `0`。`cargo test --workspace` 的所有 workspace 测试二进制均通过：包括 `halo-config` 24、`halo-core` 51、集成测试 40、`halo-protocol` 契约 38、`halo-runtime` 42、`halo-sidecar` 91、`halo-store` 22 和 `halo-testkit` 20，共 328 个测试通过、0 个失败。生命周期、测试锁生命周期和断言类型等既有基线问题已在本隔离 worktree 中以最小改动修复。

| 阶段 | 精确阶段命令 | 退出码 | 结果 |
| --- | --- | ---: | --- |
| Check | `cargo check --workspace` | 0 | 通过。 |
| Build | `cargo build --workspace` | 0 | 通过。 |
| Test | `cargo test --workspace` | 0 | 328 个 workspace 测试通过、0 个失败；5 个 `happy_opencode`、1 个 `credential_canary`、12 个 `runtime_failures` 用例均通过。 |

失败归类：

- 既有基线代码/测试问题：`origin/main` 中的 `fake_opencode.rs:778` 生命周期错误、两个测试中的临时 `MutexGuard` 借用错误、一个集成断言类型错误，以及 `runtime_failures.rs:99` 对畸形版本的错误成功预期。本隔离 worktree 只做了局部修复；没有修改生产 runtime interface。`fake_opencode.rs` 的生命周期错误来自提交 `46eec301`，畸形版本测试来自提交 `20019dc4`，而三段 semver 解析契约已存在于提交 `118e4ec4`。
- 历史测试环境问题：受限沙箱中的同一主命令曾以退出码 `101` 结束，5 个 `happy_opencode` 用例在当前 `support/mod.rs:124` 的 `require_test_credential()` 处报告 Windows Credential Manager 不可用。该失败是沙箱/受限 Windows 用户会话资格问题；没有把正向测试改成 skip、ignore、普通文件回退或内存替身。获授权的非沙箱 Windows 用户会话中同一主命令以退出码 `0` 通过，故不归类为 keyring 后端选择错误或产品代码回归。
- 工具链问题：无。`rustc host` 和 linker 顺序满足验收条件。

`runtime_failures.rs:99` 的首次复现（测试期望修正前）在编译成功后以 `RUNTIME_99_CMD_EXIT=101` 失败，错误摘要为 `RUNTIME_PROBE_FAILED` / 版本输出格式不受支持；原因是 fake runtime 的 `malformed_version` 输出 `1.18`，而生产解析器只接受三段数字版本。该失败是既有测试语义不一致，不是 MSVC、路径或 Credential Manager 问题。将该用例改为断言 `RUNTIME_PROBE_FAILED` 后，以下同一 VsDevCmd 子进程命令以退出码 0 通过：

```powershell
$cmdLine = 'call "D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cd /d "D:\Halo Studio\.worktrees\issue-01-baseline\sidecar" && cargo test -p halo-integration-tests --test runtime_failures opencode_probe_only_accepts_the_known_stable_1x_profile -- --exact --nocapture'
& cmd.exe /d /s /c $cmdLine
Write-Output "RUNTIME_99_CMD_EXIT=$LASTEXITCODE"
```

结果：`1 passed; 0 failed; 11 filtered out`，`RUNTIME_99_CMD_EXIT=0`。合格非沙箱环境中的完整 `runtime_failures` 测试为 `12 passed; 0 failed`；畸形版本仍按生产三段 semver 解析契约返回 `RUNTIME_PROBE_FAILED`。

本轮没有直接执行 `scripts\test-all.ps1` 或 `scripts\smoke-windows.ps1`：它们不会替本轮命令提供明确的 `VsDevCmd.bat` 子进程入口，且当前阶段已按要求分别执行 Cargo 命令。两者的 2026-07-27 结果仍只保留在历史基线记录中，没有作为本轮通过结果复报。

### 2.1 Credential Manager 正向资格与清理

受限沙箱中的首次主命令复现了 `cargo test --workspace` 退出码 `101`，5 个 `happy_opencode` 用例在 `support/mod.rs:124` 报告 `Windows 凭据管理器不可用`；同一环境的 `credential_canary` 只验证了不可用时 CLI 失败关闭，不能证明正向资格。该结果归类为沙箱/受限 Windows 用户会话限制。

在获授权的非沙箱 Windows 用户会话中，主验证命令成功运行；其中 `credential_canary_never_leaks_across_full_chain` 通过真实 `WindowsCredentialStore` 和 Sidecar 完成合成引用的写入、读取/存在性确认、子进程环境注入与删除守卫。为提供独立复现，执行了下面的 VsDevCmd 子进程命令：

```powershell
$cmdLine = 'call "D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cd /d "D:\Halo Studio\.worktrees\issue-01-baseline\sidecar" && rustc -vV && where link && cargo build --workspace && cargo test -p halo-integration-tests --test credential_canary credential_canary_never_leaks_across_full_chain -- --exact --nocapture'
& cmd.exe /d /s /c $cmdLine
$code = $LASTEXITCODE
Write-Output "CREDENTIAL_CANARY_WITH_BUILD_CMD_EXIT=$code"
```

结果：`credential_canary_never_leaks_across_full_chain` 为 `1 passed; 0 failed`，`CREDENTIAL_CANARY_WITH_BUILD_CMD_EXIT=0`。此前在清理 `sidecar\target` 后直接运行定向用例曾因缺少 `halo-sidecar.exe` 在 `support/mod.rs:137` 退出 `101`；补充 `cargo build --workspace` 后重跑通过，该次是测试前置二进制缺失，不是工具链、Credential Manager 或产品失败。

测试结束后仅输出授权前缀的匹配数量和测试锁状态，没有输出任何凭据值或其他 Credential Manager 条目：

```powershell
$tempPath = [IO.Path]::GetTempPath()
$lockPath = Join-Path $tempPath 'halo-studio-credential-manager-integration.lock'
$lines = @(cmdkey.exe /list 2>$null)
$matches = @($lines | Select-String -SimpleMatch 'halo/integration/opencode-')
Write-Output "TEST_LOCK_EXISTS=$(Test-Path -LiteralPath $lockPath)"
Write-Output "SYNTHETIC_CREDENTIAL_MATCH_COUNT=$($matches.Count)"
```

结果：`TEST_LOCK_EXISTS=False`、`SYNTHETIC_CREDENTIAL_MATCH_COUNT=0`。因此未遗留 `halo/integration/opencode-*` 合成凭据；生产 `WindowsCredentialStore::available()` 的写入再删除探测条件未被降低，keyring `3.6.3` 的 `windows-native` 后端也未被替换。

### 3. Python/QML 验证

命令：

```powershell
$env:PYTHONIOENCODING = 'utf-8'
Push-Location app
try {
    & 'D:\Halo Studio\.venv\Scripts\python.exe' -m pytest tests -q --basetemp ..\.scratch\issue-01-pytest
}
finally {
    Pop-Location
}
```

结果：`154 passed, 1 skipped, 2 warnings in 5.41s`，退出码 `0`。QML 静态检查包含在 `tests` 集合中；一个警告来自 `pytestqt` 的 QApplication 类型提示，另一个是当前隔离 worktree 的 `.pytest_cache` 写入权限提示，均不影响退出码。

测试临时目录清理命令：

```powershell
$tmp = Join-Path $PWD '.scratch\issue-01-pytest'
$files = @(Get-ChildItem -LiteralPath $tmp -Recurse -File -Force)
Remove-Item -LiteralPath $tmp -Recurse -Force
Test-Path -LiteralPath $tmp
```

结果：目录删除后 `PYTEST_TEMP_EXISTS_AFTER=False`。该路径只由本轮验证创建，删除范围已精确解析到隔离 worktree 内。

Cargo 生成目录清理：验证完成后确认绝对路径为 `D:\Halo Studio\.worktrees\issue-01-baseline\sidecar\target`，且 `git ls-files -- sidecar/target` 无输出；删除后 `TARGET_EXISTS_AFTER=False`。本轮生成的 Python `__pycache__` 也已按精确路径删除，没有删除任何 Git 跟踪文件。

`app\\.pytest_cache` 当前仍是 ignored 目录；对该精确路径的删除在默认和非沙箱 PowerShell 中均返回 `Access denied`，且没有修改 ACL。它不在 Git 差分中，也没有把该权限问题扩大为系统或主工作区改动。

补充 smoke 命令：

```powershell
Remove-Item Env:HALO_SIDECAR_EXE -ErrorAction SilentlyContinue
Push-Location app
try { & 'D:\Halo Studio\.venv\Scripts\python.exe' -m halo_studio.main --smoke }
finally { Pop-Location }
```

结果：`SMOKE-OK`，退出码 `0`。本次走的是 Sidecar 不可用但界面如实报告的 smoke 路径，没有伪造 Sidecar 可用状态。

### 4. Schema 验证

本轮同时执行了 JSON Schema 结构检查，作为协议文件层面的独立验证：

```powershell
$schema = Get-Content -LiteralPath 'protocol/v1/envelope.schema.json' -Encoding UTF8 -Raw | ConvertFrom-Json
$requestDef = $schema.'$defs'.request
$responseDef = $schema.'$defs'.response
$eventDef = $schema.'$defs'.event
$errorDef = $schema.'$defs'.error
if ($schema.'$schema' -ne 'https://json-schema.org/draft/2020-12/schema') { throw 'unexpected $schema' }
if ($schema.oneOf.Count -ne 3) { throw 'oneOf must have 3 entries' }
if ((@($schema.'$defs'.PSObject.Properties.Name) -join ',') -ne 'request,response,event,error') { throw 'unexpected defs' }
if ([bool]$requestDef.additionalProperties -or [bool]$responseDef.additionalProperties -or [bool]$eventDef.additionalProperties -or [bool]$errorDef.additionalProperties) { throw 'additionalProperties must be false' }
if ($requestDef.properties.v.const -ne 1 -or $requestDef.properties.kind.const -ne 'request') { throw 'request constants invalid' }
if ($responseDef.properties.v.const -ne 1 -or $responseDef.properties.kind.const -ne 'response') { throw 'response constants invalid' }
if ($eventDef.properties.v.const -ne 1 -or $eventDef.properties.kind.const -ne 'event') { throw 'event constants invalid' }
if ($errorDef.properties.code.enum.Count -ne 42) { throw 'error code enum count changed' }
"SCHEMA_STRUCTURE=PASS defs=$(@($schema.'$defs'.PSObject.Properties.Name).Count) oneOf=$($schema.oneOf.Count) error_codes=$($errorDef.properties.code.enum.Count)"
```

结果：`SCHEMA_STRUCTURE=PASS defs=4 oneOf=3 error_codes=42`，退出码 `0`。这只能证明 Schema 文件可解析且关键结构符合预期；`halo-protocol` Rust 契约测试也已作为 workspace 全量测试的一部分通过，但 Schema 检查本身不替代该 Rust 契约测试。

### 5. 附加检查

在同一 VsDevCmd 子进程中执行 `cargo fmt --all -- --check`，退出码为 `1`。输出显示 `origin/main` 中大量未改动 Rust 文件存在既有格式化差异；该检查没有报告编译或行为错误。为避免把全仓格式化产生的无关改动带入工单 01，没有执行 `cargo fmt` 写回。最终 `git diff --check` 单独退出码为 `0`。

## 历史失败与未完成门槛

- 旧基线记录中的任务 05 曾将本机 `opencode --version` 为 `1.18.5`、而实现锁定 `0.4.2` 的不匹配归类为 `BLOCKED`；该失败归类和复现信息保留在 `original-ten-task-acceptance-and-tdd-baseline.md` 与 `traceability.md` 中，不得改写成目标 Tauri 产品验收结论。
- 受限沙箱中的 `cargo test --workspace` 退出码 `101` 已保留为环境复现证据；在合格非沙箱 Windows 用户会话中同一主命令退出码为 `0`，因此不能把该历史 101 继续写成当前阻断。
- 真实 OpenCode 原生 UI 验收本轮没有执行。旧基线中的受控 fake runtime 测试不能替代目标 Tauri UI；真实 UI 仍由工单 14 负责，且受工单 12、13 阻断。
- Pi 真实安装版资格和完整 Sidecar 二进制端到端资格继续按旧基线记录保持为发布前门槛；本轮仅验证了授权范围内的合成凭据正向链，不使用真实用户密钥。

## 最终差分范围

- 状态与证据：工单 01 状态、基线 README 链接和本执行证据。
- 生产实现：`halo-config` 的 Windows Credential Manager 探测引用唯一化；`halo-sidecar` 的 OpenCode `TaskDone` 生命周期门控；其余 Sidecar/runtime 改动均为既有借用生命周期修复。
- 测试与测试替身：Credential Manager 跨进程测试锁、合成凭据引用与清理、事件游标过滤语义、OpenCode 脱敏/停止/一次性决议断言、畸形版本失败断言、fake OpenCode SSE/生命周期修复。
- 未包含：`product/` 迁移实现、真实 Tauri UI 验收、工单 14/15 操作、主工作区用户改动、`D:\BitFun-main` 内容或任何提交元数据。

## 主工作区保护记录

本轮没有在 `D:\Halo Studio` 主工作区执行 reset、checkout、clean、stash、删除、覆盖、提交或静默搬运。主工作区的源码、配置、`dist`、`.halo-runtime`、`node_modules`、`sidecar\target` 和未跟踪文件均未纳入本 worktree，也未作为迁移实现使用。`D:\BitFun-main` 未访问。

## 进入工单 02 的判定

**工单 01 可进入 `ready-for-review`，但不可进入工单 02。** 合格非沙箱 VsDevCmd 子进程中的 `rustc host`、linker 顺序、`cargo check --workspace`、`cargo build --workspace`、`cargo test --workspace`、Python/QML、Schema、凭据清理和 `git diff --check` 均满足验收；受限沙箱的 101 已完成归因并保留为历史环境复现。真实 OpenCode 原生 UI 验收和旧产品删除仍未执行，独立 Git 提交仍等待用户明确授权。
