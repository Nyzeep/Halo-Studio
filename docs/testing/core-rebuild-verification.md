# Pi RPC 核心重构验证指南

本文定义 Halo Studio 当前 P0 Pi RPC 迁移的可复现验证路径。它区分自动化契约、受控进程测试、Tauri 桌面 smoke 和唯一允许真实模型请求的工单 14 验收。历史 OpenCode Server、`opencode serve`、HTTP/SSE、旧 Sidecar JSONL 和 Electron 检查只能作为迁移比较材料，不属于当前 P0 验收门槛。

## 命令约定

- 以下命令从仓库根目录 `D:\Halo Studio` 执行；产品脚本显式指向 `product/Halo Studio`，避免调用根目录旧工程脚本。
- Node.js 使用产品 `package.json` 声明的 `>=22.12.0`；包管理器使用锁定的 `pnpm`，不使用 npm workspace 或 `npx` 临时下载工具。
- 自动化测试不得读取真实凭据、登录 Provider 或发送模型请求；真实请求只允许在工单 14 的交互式、非受限 Windows 验收中执行。
- `D:\pi-main` 只读；不在该目录安装依赖、构建、运行测试或生成文件，也不把它加入 Git 输入。

## 基础验证

```powershell
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio" run product:check
pnpm --dir "product/Halo Studio" run product:test
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/store.test.ts src/infrastructure/workbench-runtime/selectors.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-agent-runtime --test workbench_runtime_contracts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-desktop --test halo_workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run desktop:build:fast
git diff --check
```

Cargo 在本工作树有已知的 vendor checksum 漂移时，必须保留完整失败输出并标为环境阻断；不得修改 `vendor` 或通过删除锁定文件掩盖失败。

## Pi RPC 资格验证

以下三条只做本机可执行文件和版本探测，不启动 `pi --mode rpc`，不发送 prompt，不读取真实凭据：

```powershell
where.exe pi
Get-Command pi -All | Select-Object Name,CommandType,Source,Path
pi --version
```

Adapter 自动化覆盖：

| 验证目标 | 主要位置 | 可证明的行为 |
| --- | --- | --- |
| Pi executable/probe | `src/crates/adapters/pi-rpc-adapter/src/lib.rs` | 本机 `pi`/Windows `pi.cmd` 解析、版本盲探测、未安装失败关闭。 |
| LF JSONL framing | `src/crates/adapters/pi-rpc-adapter/src/framing.rs` | 单个 LF record、可剥离尾部 CR、嵌入 LF/畸形 JSON 拒绝。 |
| RPC command/event seam | `src/crates/adapters/pi-rpc-adapter/src/lib.rs`、`src/crates/contracts/runtime-ports/src/halo_workbench.rs` | `prompt`、`follow_up`、`abort`、`get_state`、`get_entries`、response 关联、`message_update`、`tool_execution_*` 和 `agent_settled` 的脱敏投影。 |
| First-party extension gate | `src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts`、Adapter tests | `tool_call` 前置阻断、`extension_ui_request/response`、单任务/单 toolCallId 决议、deny/超时/协议/extension 错误 fail closed。 |
| Workbench Runtime | `src/apps/desktop/tests/halo_workbench_runtime_contracts.rs`、`src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs` | 工作区信任、状态迁移、事件顺序、取消、迟到事件、清理和敏感字段剥离。 |
| WebView boundary | `src/web-ui/src/infrastructure/workbench-runtime/*.test.ts` | Renderer 只消费 Halo snapshot/event，不能获得 Pi 原始 session/entry/toolCall、凭据、Authorization、命令输出或原始 JSONL。 |

受控 Pi 子进程只验证进程和协议 seam，不证明用户机器的 Provider、模型、凭据、图形驱动或真实 extension 发布许可。

## Tauri 与真实验收边界

自动化 Tauri smoke 使用真实产品入口，但不替代工单 14：

```powershell
pnpm --dir "product/Halo Studio" run e2e:test:smoke
```

工单 14 在交互式、非受限 Windows 宿主中执行 `pnpm --dir "product/Halo Studio" run desktop:dev`，再从 Halo 原生 UI 完成工作区信任、首轮 `prompt`、extension tool gate、同一 session 的 `follow_up`、显式结束/只读审查和中断不重放。验收人员不能手工启动 Pi RPC、使用 Pi TUI、使用 HTTP/SSE 或以自动化替身代替真实 UI。

证据只能包含公开版本/能力档案、`pass`/`fail`/`not-run`、脱敏截图、Git 前后摘要和清理结论；不能保存完整 session、原始 JSONL、原始工具输出、凭据、Authorization、命令行或原始标识。

## OpenCode 历史比较

| 历史检查 | 当前含义 |
| --- | --- |
| OpenCode Server、`opencode serve`、回环 HTTP/SSE、Basic Auth、SSE heartbeat/dispose | 仅用于解释 ADR-0071 和迁移前行为，当前 P0 明确不执行、不维护、不作为 release gate。 |
| 旧 `sidecar/crates/halo-runtime/src/opencode.rs`、旧 JSONL 和 `agent-opencode` 测试夹具 | 已于工单 15 收缩移除；历史行为对比见工单 12 记录与归档文档，不得作为 Pi RPC 协议或生产 Adapter 输入。 |
| Pi TUI、Unix/CBOR PiServer、ACP 或任意新传输 | 范围外；必须先有独立 ADR 和 Windows 可用性证明，不能由当前测试矩阵默认为兼容。 |

任何活动规格中的 OpenCode 字样都必须能归入上述历史、比较或 superseded 分类；未标记的 OpenCode 生产命令、健康检查、认证或 Adapter 绑定均为扫描失败。

## 交付记录模板

每次合并或发布前记录：

```text
工作树/提交：
Node / pnpm：
pnpm --dir "product/Halo Studio" run check:repo-hygiene：通过 / 失败
pnpm --dir "product/Halo Studio" run type-check:web：通过 / 失败
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter：通过 / 失败
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-desktop --test halo_workbench_runtime_contracts：通过 / 失败
pnpm --dir "product/Halo Studio" run desktop:build:fast：通过 / 失败
pnpm --dir "product/Halo Studio" run e2e:test:smoke：通过 / 失败 / 未执行
工单 14 真实 Pi RPC UI 验收：通过 / 失败 / 未执行
Pi extension source/version/hash/license inventory：完整 / 阻断
git diff --check：通过 / 失败
未覆盖项及批准理由：
```
