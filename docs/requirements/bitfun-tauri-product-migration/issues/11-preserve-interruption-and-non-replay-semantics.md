# 11 - 保持 Pi RPC 中断如实化与不重放语义

**What to build:** Halo 工作台、Workbench Runtime、Pi 子进程或 JSONL 通道意外退出后，未结束任务如实显示为中断；重启不会自动重连旧 session、重发 prompt、重放操作请求或重复工作区写入。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；10 - 追问、显式结束并审查 Pi RPC 交付.

**Status:** ready-for-agent

## 实现边界

- Runtime 负责 Pi `abort`、宽限期、stdin 关闭、子进程回收和临时 session/config 清理；Pi 退出事实不能被包装为成功。
- 中断状态是不可自动重放的终态事实；重启只读取 Halo 脱敏历史，不恢复原生 session 或 extension request。
- 已产生的工作区修改和证据保留，敏感运行态清理；本票不引入自动重试、恢复或无限重连。

## 验收标准

- [ ] 应用关闭、Runtime 崩溃、Pi 子进程退出、JSONL EOF/解析失败和事件流永久失联都会把非终态受管任务归类为 `interrupted`，不得伪装为完成或等待开发者。
- [ ] 用户主动取消先向当前 Pi RPC session 发送 `abort`，再在宽限期内等待 `agent_settled`/进程退出；超时才关闭 stdin 并强制回收子进程，记录 native/forced 取消方式。
- [ ] 取消、关闭或中断后的进程级清理关闭 stdin、回收 Pi 子进程并删除受管临时 session/config；任何迟到事件不得追加到已终态证据。
- [ ] 重启后不自动恢复原生 session，不重发首轮或追问，不重放 extension UI 决议，也不重复文件写入。
- [ ] 已产生的工作区改动、任务基线和可审查证据按事实保留；受管 Pi 临时 profile、活动消息、认证材料和敏感运行态按隐私边界清理。
- [ ] 用户可以从原生 UI 看见脱敏中断原因和明确的“重新开始新运行/保持现状/进入审查”可选处置；任何新运行产生新证据版本。
- [ ] 标准 Pi 会话历史与受管中断隔离，受管清理不得删除标准模式历史。

## 验证要求

- 集成测试在首轮前、Prompt 进行中、等待操作请求、等待开发者、追问和结束取证期间分别强杀应用或 Pi 子进程，验证状态、进程、Git 和不重放。
- 测试断言重启零自动 Pi RPC 请求、零重复写入、零旧认证复用和零原始远程标识持久化。
- Windows 进程测试证明无残留 `pi` 子进程、RPC stdin/stdout 句柄和受管临时 profile。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/store.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run check:repo-hygiene
git diff --check
```

## 不在本票

- 不自动续跑、自动重试、自动恢复或静默创建替代 session。
- 不把 JSONL 瞬断的无限重连策略引入 P0；任何有限恢复必须保持不重发语义并由 Pi RPC 兼容性档案覆盖。
