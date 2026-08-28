# Halo Studio

> 面向本地开发者的原生开发工作台。Halo Studio 在受信任的 Git 工作区中，将受管任务、工作区变更、验证证据与人工审查组织为可验证的编码交付。

> [!IMPORTANT]
> **开发中 · P0 尚未放行。** Windows 是首发验收目标平台；真实 Pi RPC 原生 UI 验收记录为 `not-run`。当前 P0 受管执行链由 Halo Workbench Runtime 受控启动本机 `pi --mode rpc`，开发者审查变更和验证证据后决定是否接受交付。
>
> [查看验证记录](docs/verification/README.md) · [了解产品架构](docs/development/architecture.md)

## 从这里开始

| 如果你想…… | 建议入口 |
| --- | --- |
| 搭建本地开发环境 | 先阅读下方“开发环境快速开始”，再查看[构建与测试说明](docs/development/build-and-test.md)。 |
| 理解产品边界与术语 | [产品架构](docs/development/architecture.md) · [领域词汇](CONTEXT.md) |
| 查看当前验收事实 | [验证记录](docs/verification/README.md) · [可迁移能力基线（历史）](docs/verification/migratable-capability-baseline/README.md) |
| 为项目贡献 | [贡献指南](docs/development/contribute.md) |

## 核心边界与当前状态

| 主题 | 当前定义 |
| --- | --- |
| 正式产品入口 | `product/Halo Studio` 是唯一正式产品源码树和 Tauri 桌面入口（bin `halo-studio`）。 |
| P0 受管执行 | `Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。 |
| 发布与验收 | P0 尚未放行；真实 Pi RPC 原生 UI 验收记录为 `not-run`。完整验收与验证事实以 [验证记录](docs/verification/README.md) 为准。 |
| 运行时职责 | Pi 是当前唯一的 P0 生产执行 harness；DeepSeek Harness（DSH）仅作迁移参考，不进入 Halo Studio 的生产执行链。 |
| 受管事实 | 生产 assembly 已接入独立的 Halo managed-event-facts 持久化端口与 JSON provider；任务恢复、完整生命周期事实覆盖和交付证据投影仍在后续工单中完善，不能视为已完成发布能力。 |
| 上游与历史资料 | 上游（原 BitFun）源码与历史证据仅作历史/上游对照；`BitFun-latest/` 是豁免目录，不参与构建。 |

## 技术栈

[![Rust](https://img.shields.io/badge/Rust-1.95-orange?style=flat-square)](https://www.rust-lang.org/) [![TypeScript](https://img.shields.io/badge/TypeScript-5.8-blue?style=flat-square)](https://www.typescriptlang.org/) [![React](https://img.shields.io/badge/React-18-61dafb?style=flat-square)](https://react.dev/) [![Tauri](https://img.shields.io/badge/Tauri-2-6b46c1?style=flat-square)](https://tauri.app/) [![pnpm](https://img.shields.io/badge/pnpm-10-f69220?style=flat-square)](https://pnpm.io/) [![Vitest](https://img.shields.io/badge/Vitest-4-729b1b?style=flat-square)](https://vitest.dev/) [![GitHub Actions](https://img.shields.io/badge/GitHub%20Actions-yes-2088ff?style=flat-square)](https://github.com/features/actions) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow?style=flat-square)](product/Halo%20Studio/LICENSE)

## 开发环境快速开始

> 面向贡献者与本地开发。以下命令均在正式产品工作区 `product/Halo Studio` 中执行。

```powershell
cd "product/Halo Studio"
pnpm install
pnpm run check:repo-hygiene
pnpm run product:check
pnpm run desktop:dev
```

完整构建、测试、打包命令与常见问题见 [构建与测试说明](docs/development/build-and-test.md)。

## 仓库地图

| 路径 | 角色 |
| --- | --- |
| `product/Halo Studio/` | 唯一正式产品源码树：Rust workspace、React 前端与 Tauri 桌面入口。 |
| `docs/` | 权威文档：ADR、需求、验证、归档与开发文档。 |
| `docs/development/` | 中文开发文档：架构、构建/测试、Pi RPC 适配与贡献指南。 |
| `scripts/` | 根级归档脚本与仓库守卫。 |
| `.agents/skills/` | 仓库开发工作流技能。 |
| `BitFun-latest/` | 历史/上游对照的豁免目录：不读取、不更名、不参与构建。 |

## 继续阅读

| 主题 | 文档 |
| --- | --- |
| 文档权威顺序与目录角色 | [文档地图](docs/README.md) |
| 产品术语 | [领域词汇](CONTEXT.md) |
| 产品结构与模块边界 | [产品架构](docs/development/architecture.md) |
| Pi RPC 执行适配 | [Pi RPC 适配](docs/development/pi-rpc-adapter.md) |
| 贡献流程 | [贡献指南](docs/development/contribute.md) |
| 已执行的验收与验证事实 | [验证记录](docs/verification/README.md) |

## License

MIT（见 [LICENSE](product/Halo%20Studio/LICENSE)）。上游（原 BitFun）第三方署名与许可证说明见 `product/THIRD_PARTY_NOTICES.md`。
