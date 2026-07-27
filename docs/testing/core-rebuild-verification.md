# Pi/OpenCode 核心重构验证指南

本文定义 R1 核心重构的可复现验证路径。它区分“已有自动化覆盖”和“尚未具备的交付验收”，避免把测试夹具、占位 UI 或成功构建误当作完整桌面产品已发布。

## 前置条件

- 在当前工作树的根目录执行命令。
- Node.js 版本不低于 `20.18`，npm 版本不低于 `10.8`。
- 使用锁定依赖：`npm ci`。
- Windows 烟测必须在 Windows 上运行。
- 正式 `npm run smoke:dev --workspace @halo-studio/desktop` 必须在具有交互式桌面会话、且不受当前 Codex 限制的 Windows 宿主执行。当前受限环境会阻断 Chromium sandboxed Renderer；不得通过 `--no-sandbox` 规避，这会使结果失去安全边界上的代表性。
- 不要在 `用于参考的几个项目的代码/` 中安装依赖、构建、运行测试或生成文件；该目录不是测试输入，也不得进入 Git 暂存区。

## 基础验证命令

```powershell
npm run check:repository
npm run typecheck
npm test
npm run build
npm run verify
```

`npm run verify` 是上述仓库检查、类型检查、测试和构建的组合门槛。除非需要定位失败，不必在同一轮重复执行已被 `verify` 覆盖的命令。

## 开发态会话与烟测

`npm run dev` 会启动实际的桌面开发会话：桌面工作区先构建 Main/Preload 开发入口，再在固定回环地址 `http://127.0.0.1:5173` 启动 Vite Renderer，并由 Electron 加载该开发地址。它适合本地界面开发，不是打包安装器或真实 Pi/OpenCode 运行时的验收。

`better-sqlite3` 是原生模块，宿主 Node 与 Electron 使用不同 ABI。桌面 `build`、`dev` 和 `smoke:dev` 的前置步骤会执行 `scripts/prepare-native-runtime.mjs electron`；根 `npm test`、桌面测试和 `windows-smoke.mjs` 则在执行前使用 `scripts/prepare-native-runtime.mjs node` 恢复当前 Node ABI。脚本在 `.halo-runtime/native-build-cache/` 保存已构建副本，该目录已被 Git 忽略，绝不能暂存或提交。

Windows 上的开发态 Electron 烟测命令为：

```powershell
npm run smoke:dev --workspace @halo-studio/desktop
```

该命令使用临时 Electron 用户数据目录启动同一 Vite + Electron 流程。烟测子进程会使用 `--headless`、`--disable-gpu` 与受捆绑 SwiftShader 支持的 `--use-angle=swiftshader`；普通 `npm run dev` 不会带入这些开关，且这些开关不会改变窗口的 `sandbox`、`contextIsolation` 或 Node 隔离。Main 只有在 `loadURL` 成功后才会写入就绪标记；烟测失败会输出 Vite/Electron 的 stdout/stderr 与标记诊断。当前 Codex 受限环境仍会阻断 Chromium sandboxed Renderer，即使使用上述开关也不能作为正式烟测宿主；请在交互式、非受限 Windows 宿主运行，绝不使用 `--no-sandbox` 作为替代方案。它不替代人工界面验收，不覆盖 Pi/OpenCode 生命周期，也不验证打包产物。

提交前还应执行：

```powershell
git diff --check
```

发布前的依赖复核命令为：

```powershell
npm audit --omit=dev
npm ls --omit=dev --all
```

这两个命令提供审查输入，不自动构成发布批准。任何网络、安装或审计失败都应记录为环境/依赖问题，而不是通过跳过命令来掩盖。

## 聚焦验证命令

以下命令用于定位 R1 关键边界：

```powershell
npm test --workspace @halo-studio/desktop -- workspace-runtime.integration.test.ts
npm test --workspace @halo-studio/desktop -- credential-boundary.integration.test.ts
npm test --workspace @halo-studio/desktop -- piLaunchResolver.test.ts
npx vitest run tests/sessionCoordinator.test.ts src/renderer/components/SessionWorkbench.test.tsx --config apps/desktop/vitest.config.ts
node scripts/windows-smoke.mjs
npm run smoke:dev --workspace @halo-studio/desktop
```

`windows-smoke.mjs` 会先恢复当前 Node ABI，再在 Windows 上运行第一条桌面集成测试。它会使用受控的 Node 子进程模拟 Pi 与 OpenCode 的真实协议过程，并覆盖 Pi 就绪、OpenCode 健康/版本、优雅停止和临时目录清理；它不启动已打包的图形界面。

## 自动化覆盖矩阵

| 验证目标 | 主要位置 | 当前可证明的行为 |
| --- | --- | --- |
| 仓库边界 | `scripts/assert-repository.mjs` | 阻止参考资料目录和已废弃工程根路径被 Git 跟踪。 |
| 工作区与信任 | `packages/core/src/workspace.test.ts`、桌面服务测试 | 真实路径、目录可用性、信任状态和单一活动工作区语义。 |
| Pi transport | `packages/agent-pi/src/*.test.ts` | JSONL 分帧、检测、`get_state` 就绪、停止和协议故障语义。 |
| OpenCode transport | `packages/agent-opencode/src/*.test.ts` | 锁定工件解析、回环健康检查、认证、版本检查、SSE 与停止语义。 |
| Main 服务组合 | `apps/desktop/tests/workspace-runtime.integration.test.ts` | 受控子进程下的信任门槛、Pi/OpenCode 真实生命周期、含空格/CJK 的临时路径与清理。 |
| 结构化会话 | `apps/desktop/tests/workspace-runtime.integration.test.ts`、`tests/sessionCoordinator.test.ts` | 未受信任和未启动运行时不读取会话；Pi 当前会话、OpenCode 会话、发送/中止、原生命令目录、事件脱敏投影与请求去重。 |
| 凭据边界 | `apps/desktop/tests/credential-boundary.integration.test.ts`、`piLaunchResolver.test.ts` | Provider canary 不出现在 IPC、运行时快照、SQLite 或凭据密文；Main-only 启动配置对缺值失败关闭。 |
| 开发态桌面启动 | `scripts/desktop-dev.mjs`、`scripts/electron-dev-smoke.mjs` | 在交互式、非受限 Windows 宿主以真实 Vite + Electron 进程确认回环 Renderer 已被加载；保留 Chromium Renderer sandbox，不覆盖受管运行时或打包流程。 |
| Preload 与窗口安全 | `apps/desktop/tests/security.test.ts`、`apps/desktop/tests/ipc.test.ts` | 固定 IPC、输入/输出校验、隔离窗口首选项、禁止 Renderer 直连运行时。 |
| Renderer 行为 | `apps/desktop/src/renderer/App.test.tsx`、`components/SessionWorkbench.test.tsx` | 工作区/信任/运行时的 UI 状态在模拟 Preload API 下可见；受信任工作区的 Pi 固定启动、停止和崩溃/不可用后重试操作，以及会话读取、发送和原生命令目录都会使用固定 IPC。 |
| 存储与配置基础 | `packages/storage/src/*.test.ts`、`packages/config/src/*.test.ts` | 迁移恢复、凭据库、配置预览/备份/回滚的底层安全性质。 |

受控进程夹具只用于测试 `createDesktopServices` 这一 Main 服务组合接缝。它们不是生产回退路径，也不能证明用户机器上的 Pi 安装、OpenCode 外部服务、图形驱动或打包安装器正常。

## R1 人工核对清单

在自动化检查通过后，至少核对以下结果：

1. 仓库中没有被暂存或跟踪的参考资料目录内容。
2. 工作区打开后初始为未受信任，未受信任状态下 Pi 和 OpenCode 启动请求都被拒绝。
3. 受信任工作区中，Pi 面板只能用固定 IPC 启动、停止或重试，且不提供模型、thinking、Provider 或凭据输入；Pi 只有在 Main 侧选择器和受保护凭据均可用时才会到达 `ready`，公开状态不显示这些值。
4. OpenCode 只有在回环健康检查及版本握手成功后才报告 `healthy`；公开状态不显示认证信息或端口。
5. 切换工作区、撤销信任和应用退出会请求停止对应运行时；停止失败时不能把运行时伪装为安全已清理。
6. 含空格和 CJK 字符的 Windows 工作区路径通过集成测试。
7. `npm run smoke:dev --workspace @halo-studio/desktop` 在交互式、非受限 Windows 宿主完成，且没有使用 `--no-sandbox`。
8. `npm run verify`、`node scripts/windows-smoke.mjs`、`npm run smoke:dev --workspace @halo-studio/desktop` 和 `git diff --check` 的实际退出码被记录。

## 当前未覆盖或未交付的验收

以下事项不能因现有测试成功而视为已完成：

- 已打包桌面应用的启动、安装、升级和卸载流程；仓库尚无打包发布脚本。
- 受限 Codex 宿主中的 Chromium sandboxed Renderer 烟测；这类环境不能用 `--no-sandbox` 伪造通过结果，正式开发态烟测须改在交互式、非受限 Windows 宿主执行。
- 开发态 Electron 会话的完整人工验收，以及已打包桌面应用的验收；`npm run dev` 与 `npm run smoke:dev --workspace @halo-studio/desktop` 只覆盖开发态的 Vite + Electron 启动，不覆盖安装器、升级或已打包产物。
- 用户界面的 Pi 启动配置、凭据输入/管理和配置写入流程；当前 Pi 界面只提供固定启动、停止和重试，配置 IPC 在 Main 服务中失败关闭。
- 文件编辑、完整对话、命令执行或嵌入式终端的功能与安全验收。
- macOS/Linux 的发布验收。
- 真实用户环境中外部 Pi 可执行文件、网络 Provider 或实际 OpenCode 服务的端到端验收。

这些项目需要在其实现和验收脚本同时落地后，才可以从本节移入 R1 的自动化或人工核对清单。

## 本次验收记录（2026-07-24）

- 工作树：`D:\Halo Studio\.worktrees\develop`；Node `v24.14.1`；npm `11.14.1`。
- `npm run verify`：通过，退出码 `0`（仓库检查、类型检查、全部工作区测试和构建均完成）。
- `node scripts/windows-smoke.mjs`：通过，退出码 `0`（Pi readiness、OpenCode health/version、优雅停止和临时目录清理）。
- `npm ls --omit=dev --all`：通过；其他平台的 OpenCode 二进制仅作为预期的 optional dependency 未安装。
- `npm audit --omit=dev`：退出码 `1`，仅发现 `diff@7.0.0` 的 1 项低危拒绝服务漏洞（GHSA-73rr-hh4g-fpgx）；上游当前没有可用修复，已记录为发布前风险。
- `npm run smoke:dev --workspace @halo-studio/desktop`：未执行。当前 Codex 受限宿主无法代表 sandboxed Renderer 的正式验收，且未使用 `--no-sandbox` 绕过该限制；需在交互式、非受限 Windows 宿主补跑。

## 记录模板

每次准备合并或发布时，建议在对应的 GitHub Issue 或合并说明中记录：

```text
工作树/提交：
Node / npm：
npm run verify：通过 / 失败（附退出码与摘要）
node scripts/windows-smoke.mjs：通过 / 失败（Windows 版本）
npm run smoke:dev --workspace @halo-studio/desktop：通过 / 失败（Windows 版本、交互式/非受限宿主、未使用 --no-sandbox）
npm audit --omit=dev：结果摘要
git diff --check：通过 / 失败
未覆盖项及批准理由：
```

文档中的命令是待执行的验证计划，不表示当前工作树已经通过它们。以命令实际输出和保留的证据为准。
