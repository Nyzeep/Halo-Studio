# 15 - 收缩旧产品实现并完成 Pi RPC 全量复验

**What to build:** 在 Pi RPC-backed Tauri 产品通过全部迁移门槛后，维护者通过独立变更删除旧 QML、旧 Sidecar、历史 OpenCode Adapter 和旧启动入口，使 Halo 只剩一个正式桌面产品与一个权威 Workbench Runtime，并在最终仓库状态重新获得发布证据。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；14 - 完成真实 Pi RPC 原生 UI 验收.

**Status:** ready-for-agent

## 实现边界

- 收缩只删除经过引用审计、行为等价和 14 验收证明已替代的旧入口与实现；不删除用户数据、凭据、分支、提交或远端。
- Pi RPC Adapter、第一方 extension、Tauri 产品树和验证证据必须在删除旧实现后继续被正式构建和测试引用。
- 删除是独立变更；若任一门槛失败，停止收缩并保留现状，不用 destructive git 操作掩盖缺口。
- 决策记录：产品树 OpenCode 全量移除（15b）延后至 DeepSeek Harness 基座迁移，见 `docs/adr/0074-defer-opencode-removal-until-deepseek-harness-migration.md`；过渡期冻结新增 OpenCode 功能与引用，迁移完成后在新基座执行最终扫描。

## 验收标准

- [ ] 删除范围由引用审计和工单 12 行为等价矩阵确定，只移除已被新产品替代的旧入口、Adapter、传输、测试和文档。
- [ ] 旧 `sidecar/crates/halo-runtime/src/opencode.rs` 和 `packages/agent-opencode` 中仍需保留的行为语义和测试夹具已迁入新的 Pi RPC Adapter 后再删除；不保留历史 OpenCode HTTP/SSE、旧 JSONL 或双运行时桥接。
- [ ] 仓库、脚本、权威文档和发布配置只指向 `product/Halo Studio` Tauri 产品，不再提供旧 PySide/QML、Python 或 Sidecar 产品入口。
- [ ] 新 Halo Workbench Runtime、Pi RPC Adapter、第一方 extension、受控替身和行为等价证据不得被误判为“旧 Sidecar”删除。
- [ ] 删除后重跑完整 Tauri 构建/打包、Rust/前端契约、Pi RPC Adapter 集成、extension 审计、桌面端到端、行为等价矩阵、同步演练和许可证检查。
- [ ] 删除后再次完成真实 Pi RPC 原生 UI 主链与中断验收；任何失败或未执行项阻止 P0 放行。
- [ ] 最终扫描不存在旧入口、外部 `D:\BitFun-main`/`D:\pi-main`/`D:\opencode-dev` 依赖、历史 OpenCode/Code Agent P0 选择器、Pi 未审计 extension 或 Pi/OpenCode 内部源码副本。

## 安全边界

- 本票不自动删除用户工作区、应用数据、系统凭据、Git 分支、提交或远端。
- 删除清单必须先提交审查；任何来源不明或仍被正式构建引用的文件不得删除。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio" run product:check
pnpm --dir "product/Halo Studio" run product:test
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run desktop:build:fast
pnpm --dir "product/Halo Studio" run e2e:test:smoke
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts
git diff --check
```

删除后还必须按工单 12、13、14 的精确命令重跑；任何活动规格中的 `opencode serve`、HTTP/SSE、OpenCode Server Adapter 或旧 OpenCode 认证/健康检查残留都必须被归档、删除或明确标为历史比较对象。
