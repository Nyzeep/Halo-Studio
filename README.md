# Halo Studio

Halo Studio 是面向本地开发者的原生开发工作台：在受信任 Git 工作区中，由 Halo Workbench Runtime 通过本机 Pi Agent（`pi --mode rpc`）执行受管编码任务，并围绕任务基线、文件写入租约、交付证据与人工审查组织可验证的编码交付。

## 当前状态

- **唯一正式产品入口**：`product/Halo Studio`（Tauri 桌面应用，bin `halo-studio`）。
- **P0 执行链**：`Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。
- **发布状态**：工单 14/15 的真实 Pi RPC 原生 UI 验收记录为 `not-run`，P0 未放行；完整验收与验证事实以 `docs/verification/` 为准。
- **上游对照**：上游（原 BitFun）源码与历史证据仅作历史/上游对照，统一标注“历史记录/上游对照（已归档）”；`BitFun-latest/` 为豁免目录，不参与构建。

## 技术栈

[![Rust](https://img.shields.io/badge/Rust-1.95-orange?style=flat-square)](https://www.rust-lang.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.8-blue?style=flat-square)](https://www.typescriptlang.org/)
[![React](https://img.shields.io/badge/React-18-61dafb?style=flat-square)](https://react.dev/)
[![Tauri](https://img.shields.io/badge/Tauri-2-6b46c1?style=flat-square)](https://tauri.app/)
[![pnpm](https://img.shields.io/badge/pnpm-10-f69220?style=flat-square)](https://pnpm.io/)
[![Vitest](https://img.shields.io/badge/Vitest-4-729b1b?style=flat-square)](https://vitest.dev/)
[![GitHub Actions](https://img.shields.io/badge/GitHub%20Actions-yes-2088ff?style=flat-square)](https://github.com/features/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow?style=flat-square)](product/Halo%20Studio/LICENSE)

## 快速开始

```powershell
cd "product/Halo Studio"
pnpm install
pnpm run check:repo-hygiene
pnpm run product:check
pnpm run desktop:dev
```

完整构建/测试/打包命令与常见问题见 [docs/development/build-and-test.md](docs/development/build-and-test.md)。

## 目录说明

| 路径 | 角色 |
| --- | --- |
| `product/Halo Studio/` | 唯一正式产品源码树（Rust workspace + React 前端 + Tauri 桌面入口） |
| `docs/` | 权威文档：ADR、需求、验证、归档与开发文档 |
| `docs/development/` | 中文开发文档（架构、构建/测试、Pi RPC 适配、贡献指南） |
| `scripts/` | 根级归档脚本与仓库守卫 |
| `.agents/skills/` | 仓库开发工作流技能（to-spec、to-tickets、implement、tdd、code-review 等） |
| `BitFun-latest/` | 豁免目录：不读取、不更名、不参与构建 |

## 文档入口

- [文档地图](docs/README.md)
- [领域词汇](CONTEXT.md)
- [产品架构](docs/development/architecture.md)
- [Pi RPC 适配](docs/development/pi-rpc-adapter.md)
- [贡献指南](docs/development/contribute.md)
- [可迁移能力基线（历史）](docs/verification/migratable-capability-baseline/README.md)

## License

MIT（见 [LICENSE](product/Halo%20Studio/LICENSE)）。上游（原 BitFun）第三方署名与许可证说明见 `product/THIRD_PARTY_NOTICES.md`。
