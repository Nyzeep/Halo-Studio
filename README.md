# Halo Studio

Halo Studio 是面向本地开发者的原生开发工作台。唯一正式产品入口是受跟踪的 `product/Halo Studio` Tauri 桌面产品，P0 执行链为 `Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。历史 PySide/QML 与 Rust Sidecar 基线已按工单 15 的独立收缩变更移除，仅保留归档文档作为历史比较对象；旧入口不再可运行，也不构成验收证据。

## 当前状态

- **唯一正式产品：** `product/Halo Studio`（Tauri 桌面入口 + Tauri seam 上的 Halo Workbench Runtime Module）；BitFun 仍是产品基座，不是第二个 P0 执行权威。
- **发布状态：** 工单 14 的真实 Pi RPC 原生 UI 验收记录为 `not-run`，P0 未放行；工单 15 收缩后的完整复验以 `docs/verification/` 记录为准。
- **P0 执行链：** `Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。
- **外部上游参考：** `D:\BitFun-main` 只用于获取和检查 BitFun 上游，不是构建依赖或 Halo 提交位置；`D:\pi-main` 只读用于核对 Pi RPC 行为，不复制源码、不建立依赖。

## 文档入口

- [文档地图](docs/README.md)
- [领域词汇](CONTEXT.md)
- [目标产品架构](docs/architecture/target-product.md)
- [BitFun/Tauri 迁移规格与工单](docs/requirements/bitfun-tauri-product-migration/README.md)
- [可迁移能力基线（历史）](docs/verification/migratable-capability-baseline/README.md)
- [历史 PySide/Sidecar 基线（归档）](docs/archive/legacy-pyside-sidecar-baseline/README.md)

## 验证

正式构建、测试与验收命令见 `product/Halo Studio/package.json` 和工单 15 的精确验证清单；根目录不再提供旧基线命令。
