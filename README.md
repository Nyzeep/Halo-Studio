# Halo Studio

Halo Studio 是面向本地开发者的原生开发工作台。产品正在从已验证的 PySide/QML + Rust Sidecar 能力基线迁移到 Halo 品牌的 BitFun/Tauri 工作台；新功能只能面向目标产品实现，旧运行时仅用于行为等价核对。

## 当前状态

- **当前可运行基线：** `app/` 与 `sidecar/` 仍可用于旧六票的自动化复验，但不是最终产品入口。
- **唯一目标产品：** 受跟踪的 `product/` 产品树、Tauri 桌面入口和 Tauri seam 上的 Halo Workbench Runtime Module；BitFun 仍是产品基座，不是第二个 P0 执行权威。
- **发布状态：** BitFun/Tauri 迁移尚未完成，真实 Pi RPC 原生 UI 验收尚未执行，P0 未放行。
- **P0 执行链：** `Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。
- **外部上游参考：** `D:\BitFun-main` 只用于获取和检查 BitFun 上游，不是构建依赖或 Halo 提交位置。
- **Pi 协议参考：** `D:\pi-main` 只读用于核对 Pi RPC、extension、模型和 session 行为，不复制源码、不建立依赖、不修改该目录。

## 文档入口

- [文档地图](docs/README.md)
- [领域词汇](CONTEXT.md)
- [目标产品架构](docs/architecture/target-product.md)
- [BitFun/Tauri 迁移规格与工单](docs/requirements/bitfun-tauri-product-migration/README.md)
- [可迁移能力基线](docs/verification/migratable-capability-baseline/README.md)
- [历史 PySide/Sidecar 基线](docs/archive/legacy-pyside-sidecar-baseline/README.md)

## 基线复验

以下命令只验证迁移输入，不代表目标 Tauri 产品通过验收。Rust 命令必须在 Visual Studio Build Tools 开发环境中运行。

```powershell
cd sidecar
cargo check --workspace
cargo test --workspace

cd ..
.\.venv\Scripts\python.exe -m pytest app/tests
.\scripts\smoke-windows.ps1
```

目标产品的构建和启动命令将在迁移工单建立正式 Tauri 入口后加入。不得把旧 `scripts/dev.ps1` 启动结果作为目标 UI 验收。
