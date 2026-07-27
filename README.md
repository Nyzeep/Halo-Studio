# Halo Studio

Halo Studio 是面向本地开发者的原生开发工作台：在单一受信任 Git 工作区中受管 **Pi** 与 **OpenCode** 两个编码应用，提供任务基线、结构化运行轨迹、只读交付审查与手动交接的**可验证编码交付**闭环。

- 需求依据：[requirements-alignment/](requirements-alignment/)（01 基础对齐、02 任务拆分）
- 领域词汇表：[CONTEXT.md](CONTEXT.md)
- 首发平台：Windows（路径 / 进程 / 凭据 / IPC 边界保持可移植）

## 架构总览

```
┌────────────────────────────┐   stdio JSONL (版本化契约 v1)   ┌─────────────────────────────┐
│  app/  PySide6 + QML 原生UI │ ◄────────────────────────────► │  sidecar/  Rust Runtime      │
│  - ipc/        Sidecar客户端│                                │  - halo-protocol  消息契约   │
│  - viewmodels/ 视图模型     │                                │  - halo-core      领域状态机 │
│  - qml/        原生界面     │                                │  - halo-config    启动配置/凭据│
└────────────────────────────┘                                │  - halo-store     SQLite持久化│
                                                              │  - halo-runtime   Pi/OC 运行时│
        UI 永不接触凭据明文、                                   │  - halo-sidecar   stdio 入口 │
        不直接触碰工作区 Git                                    │  - halo-testkit   受控假进程 │
                                                              └──────────┬──────────────────┘
                                                                         │ 原生协议受管启动/停止
                                                              ┌──────────▼──────────────────┐
                                                              │  Pi (RPC/stdio)  OpenCode(回环)│
                                                              │  按各自原生权限模型写工作区    │
                                                              └─────────────────────────────┘
```

详见 [docs/architecture.md](docs/architecture.md)。

## 目录结构

| 路径 | 说明 |
| --- | --- |
| `requirements-alignment/` | 已确认的需求对齐记录（只读，不修改） |
| `docs/` | 架构、IPC 契约、模块契约、需求追踪 |
| `protocol/v1/` | JSONL 契约的 JSON Schema（UI 与 Sidecar 共同依据） |
| `sidecar/` | Rust cargo workspace（全部后台与安全敏感能力） |
| `app/` | PySide6/QML 原生应用 |
| `scripts/` | 构建、测试、Windows 烟测脚本 |

## 构建与测试

前置：Rust（MSVC 工具链）、Python 3.13、Git。Python 依赖安装在 `.venv`。

```powershell
# Rust：构建 + 全部单元/契约/集成测试
cd sidecar; cargo build --workspace; cargo test --workspace

# Python：应用测试（使用符合契约的测试 Sidecar）
.\.venv\Scripts\python.exe -m pytest app/tests

# 启动应用（先构建 sidecar）
.\scripts\dev.ps1

# Windows 烟测（验证无 Electron/浏览器依赖、Sidecar 状态可见）
.\scripts\smoke-windows.ps1
```

## 安全与边界（不可回退项）

- 凭据明文只在 Sidecar 启动受管应用时短暂读取；不进入 UI、IPC、日志、Diff、备份或 SQLite。凭据录入走 `halo-sidecar cred set <ref>`（stdin），UI 只处理凭据引用。
- 生产路径无 Mock Agent、无模拟在线状态、无 Electron/WebView 入口；测试替身只存在于 `halo-testkit` 与 `app/tests/`。
- 接受/拒绝交付只记录结论：不提交、不推送、不建分支、不回滚、不删除文件。
- Halo Studio 不自行执行任意验证命令；验证结果只来自受管应用原生运行时或用户显式标记“未执行”。
