# 12 - 证明旧六票到 Pi RPC Tauri 产品的行为等价迁移

**What to build:** 发布负责人可以通过一份逐项矩阵确认旧 GitHub #9–#14 的可迁移能力已经在 Pi RPC-backed Halo Tauri 产品中获得等价的用户可观察证据，同时保留旧 issue 的历史状态，不把旧内部协议、历史 OpenCode HTTP/SSE 或多执行器设想当作兼容目标。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；11 - 保持 Pi RPC 中断如实化与不重放语义.

**Status:** ready-for-agent

## 实现边界

- 矩阵只比较用户可观察行为和安全事实，不比较旧 Sidecar JSONL、历史 OpenCode HTTP/SSE、Pi 内部源码或原始标识。
- GitHub #9–#14 只读；本票新增前向证据和缺口分类，不修改旧 issue 内容、状态或历史附件。
- 每个结论必须绑定精确命令或脱敏 artifact；缺失、失败和未执行都阻断发布，不用替身或静态页面推断通过。

## 验收标准

- [ ] 矩阵逐项记录：旧 GitHub issue/行为、旧证据、新 Halo Runtime Interface、新 Pi RPC Adapter 证据、新原生桌面路径和当前结论。
- [ ] 工作区、配置与凭据、Pi 可执行文件/RPC 兼容启动、首轮 session、一次性 extension tool gate、追问、显式结束、只读审查和中断均有新产品证据。
- [ ] 旧 #9–#14 不改写标题、需求、状态或历史验收；新矩阵是前向迁移证据，不是对旧 issue 的重新实现记录。
- [ ] Runtime/Adapter 契约、第一方 extension 测试、前端测试、Tauri 桌面烟测和必要端到端测试从 Halo 正式入口执行并通过。
- [ ] 旧 Sidecar JSONL、Python/QML、私有 `opencode.rs` 布局、Pi 内部源码和原始 session/entry 标识不得成为等价断言。
- [ ] 旧基线中 OpenCode、Pi TUI、Unix/CBOR PiServer 或多执行器相关行为明确归类为历史/范围外，而不是虚构成 Pi RPC 已覆盖；只有当前 P0 必需的 Pi 行为进入发布结论。
- [ ] 缺失、失败或尚未人工执行的项目明确标为阻断，不得推断或伪造通过。

## 验证要求

- 每个矩阵结论必须链接到精确自动化命令或脱敏 artifact；仅代码存在、HTTP 静态页面、历史 OpenCode smoke 或受控替身不能替代 Pi 原生真实验收。
- 加入一致性检查，确保所有旧六票条目恰好映射一次、所有 P0 工单 04–11 均有覆盖、所有失败均有分类。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-agent-runtime --test workbench_runtime_contracts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-desktop --test halo_workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run desktop:build:fast
rg -n 'GitHub #9|GitHub #10|GitHub #11|GitHub #12|GitHub #13|GitHub #14' docs/requirements/halo-tauri-product-migration docs/verification
git diff --check
```

## 不在本票

- 不修改或关闭旧 GitHub #9–#14。
- 不执行真实外部凭据会话；工单 14 负责真实 Pi RPC 原生 UI 验收。
