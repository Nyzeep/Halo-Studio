# Halo Studio 构建与测试

## 1. 环境要求

- Windows 10/11（首发验收平台），需 WebView2 运行时；
- Rust 工具链 1.95+（`rustc`/`cargo`），使用仓库内 vendored 源（`product/Halo Studio/.cargo/config.toml`），可离线解析依赖；
- Node.js 22.12+，pnpm 10（仓库 `packageManager` 为 pnpm@10.15.0）；
- 首次 `pnpm install` 需要访问 npm registry；`sherpa-onnx-sys` 构建脚本需要访问 GitHub 下载库文件（受限网络下需提权）。

## 2. 依赖安装

```powershell
cd "product/Halo Studio"
pnpm install          # 安装 workspace 依赖（含 postinstall halo-scope 守卫）
```

若 pnpm 因 workspace 变更要求重建 modules 目录且无 TTY，使用：

```powershell
$env:CI = "true"; pnpm install
```

## 3. 常用命令

在 `product/Halo Studio` 目录执行（任务验收矩阵）：

```powershell
pnpm run check:repo-hygiene
pnpm run product:check
pnpm run product:test
pnpm run type-check:web
pnpm run desktop:build:fast
pnpm run e2e:test:smoke
cargo test --manifest-path Cargo.toml -p halo-pi-rpc-adapter
cargo test --manifest-path Cargo.toml -p halo-tauri-desktop
node --test scripts/halo-scope.test.mjs
git diff --check
```

开发热重载：

```powershell
pnpm run desktop:dev          # Vite HMR + Rust 自动重建/重启
pnpm run desktop:preview:debug  # 前端热更，Rust 不自动重建
```

其他常用：

```powershell
pnpm run lint:web
pnpm run lint:rs:desktop
pnpm run i18n:generate && pnpm run i18n:contract:test && pnpm run i18n:audit
pnpm run theme:color-audit:all
cargo check --workspace
```

## 4. 常见问题

| 现象 | 原因与处理 |
| --- | --- |
| `sherpa-onnx-sys` 构建脚本报 Connection Failed | 构建脚本需下载 sherpa-onnx 库文件；在允许网络的环境重跑 `cargo check/test` |
| `resource path ..\..\mobile-web\dist doesn't exist` | 桌面 crate 需要 mobile-web 产物；先运行 `pnpm --dir src/mobile-web run build` |
| pnpm 报 `Aborted removal of modules directory due to no TTY` | workspace 变更后 pnpm 需要重建符号链接；设 `CI=true` 后重跑 install/run |
| e2e 报 `Webview not available: main` | 通常是沙箱/权限导致 WebView2 无法创建；以完整用户环境（提权）运行 e2e |
| `i18n:audit` 报 sharedTermDuplicates 基线不符 | 该基线为 no-growth 约束；若确为存量债务，需单独评审后再调整基线，不得为过关放宽 |
| 日志目录写入被拒（os error 5） | 应用需要 `%APPDATA%\Halo Studio` 写入权限；确认运行环境为真实用户会话 |

## 5. 验证矩阵记录

更名任务的全量验证结果（pass/fail/not-run 与原因）记录在任务最终汇报与 `docs/verification/`；历史证据内容不因更名篡改，命令名保留历史记录（`scripts/verify-old-six-behavior-equivalence.mjs` 中的命令为历史证据匹配项）。
