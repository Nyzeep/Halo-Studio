---
status: ready-for-agent
blocked-by: 14（真实 Pi RPC 原生 UI 验收，not-run）
---

# 01 - 采纳 dsh 基座并实现 Pi RPC 执行与系统凭据 Provider

## Problem Statement

Halo 当前 P0 执行链（Halo Workbench Runtime → 受控 Pi 子进程 → `pi --mode rpc` → LF JSONL）构建在 Halo Studio 派生产品树上；该树即将被 DeepSeek Harness（`dsh`，developer preview）替换为新基座。开发者需要在换基座后仍能：

- 在受信 Git 工作区中创建受管任务并发送首轮 prompt，Pi RPC 作为唯一受管执行器完成回合；
- 在同一会话内追问、中止、读取状态，并保持 `等待开发者` 语义；
- 通过系统凭据库一次性录入 Provider 密钥，产品只保存凭据引用；
- 标准编码模式与受管交付模式共享同一份非敏感配置权威。

当前实现无法直接搬到 dsh：Pi RPC 是进程协议而非 LLM 流式接口，Windows 凭据库也不是 dsh `credentials` 的默认 env/.env provider。

## Solution

以 dsh 为执行与能力基座，Halo 继续作为产品外壳持有信任、任务、权限、证据与生命周期权威。本规格交付三件事：

1. 锁定并采纳一个 dsh 基线提交，建立 Halo 产品插件层（profile/bundle）。
2. 实现 `dsh-pi-rpc` 插件：在 dsh 能力 seam 上承载 Pi RPC 的启动、探测、回合、追问、中止与状态读取，并把工具门控接到 dsh 的 interaction/tools 事件。
3. 实现 `dsh-credential-manager` 插件：在 dsh `credentials` seam 上新增 Windows Credential Manager Provider，沿用 `halo-pi-credential-v1-` 引用与 provider 绑定校验。

## User Stories

1. 作为本地开发者，我希望 dsh 基座以固定提交纳入产品依赖，以便迁移可复现、可升级。
2. 作为本地开发者，我希望 Halo 以 dsh profile/bundle 层挂载产品插件，以便不修改 dsh 内核即可扩展。
3. 作为本地开发者，我希望在受信工作区创建受管任务后由 dsh 插件启动受控 Pi RPC 子进程，以便执行链仍由 Halo 持有生命周期。
4. 作为本地开发者，我希望首次配置 Provider/模型/思考级别后插件执行能力检查并显示就绪状态，以便失败在启动前关闭。
5. 作为本地开发者，我希望在 UI 一次性录入密钥并写入 Windows Credential Manager，以便产品只保存凭据引用。
6. 作为本地开发者，我希望已有配置的 Base URL 在留空编辑时保持不变，以便 write-only 端点不被误清除。
7. 作为本地开发者，我希望切换 Provider 时要求重新录入凭据，以便不会复用跨 Provider 的引用。
8. 作为本地开发者，我希望首轮 prompt 后状态停在 `等待开发者`，以便不会因 Pi idle 自动完成。
9. 作为本地开发者，我希望在同一任务内发送 follow_up，以便在同一 RPC 会话中继续。
10. 作为本地开发者，我希望工具调用在 `tools/pre-execute` 前进入一次性决议，以便 deny/超时不执行工具。
11. 作为本地开发者，我希望中止任务后 dsh 记录中断事实且不自动重连、重发或重放，以便符合 Halo 中断语义。
12. 作为本地开发者，我希望标准与受管模式共享同一配置权威但使用各自 preset，以便设置不漂移。
13. 作为审计者，我希望日志、事件与证据不含密钥或完整凭据，以便满足 ADR-0008/0009。
14. 作为发布负责人，我希望现有 pi-rpc 契约测试能作为行为基线迁移到 dsh 插件，以便行为等价可证明。

## Implementation Decisions

### Seams（待确认）

- **Pi RPC 能力 seam**：新增 `pi-rpc` 能力 seam，三角色齐全：
  - Service Definition：`probe`/`start`/`prompt`/`follow_up`/`abort`/`get_state`/`get_entries` 与规范化事件流；
  - Provider：spawn 本机 `pi --mode rpc`，严格 LF JSONL 分帧，凭据仅在子进程边界注入；
  - Consumer：接入 `core/agent-loop` 的 turn/step 与 `tools/*` 事件，把 `tool_call` 门控转发给 `interaction` 能力。
- **凭据能力 seam**：复用 dsh `credentials` seam（Service Definition / Provider / Consumer），新增 OS Credential Manager Provider 角色；Halo 配置服务作为 Consumer。
- **测试 seam**：优先在 `pi-rpc` 能力 seam 做契约测试（fake Pi fixture），组装层做 keyless snapshot，避免测试散落。

### 基座采纳

- 锁定 dsh 审计提交 `47f943859bef60e4160492346772ded9b24f765a` 为初始基线；记录升级流程（dsh 为 developer preview，兼容性破坏须走新兼容档案）。
- dsh 以依赖/锁定方式引入，不 fork 内核；Halo 产品差异全部落在 bundle/插件层。
- 受管模式沿用 `--no-extensions` 等价的 fail-closed 组合，第一方 extension gate 由后续插件接入 `interaction`。

### 配置与凭据

- 配置权威保存 Provider、模型、write-only Base URL、思考级别、允许的启动选项与凭据引用；不保存明文。
- 凭据引用格式沿用 `halo-pi-credential-v1-<provider-binding>-<uuid>`；读取/删除均校验 provider 归属，缺失或失配 fail-closed。
- 密钥经隔离命令一次性写入系统凭据存储并返回引用；前端状态、payload、日志与证据不含明文。
- 更新配置时未填 Base URL 视为保留既有端点；切换 Provider 必须提供新凭据。

## Testing Decisions

- 好测试只断言外部行为：协议分帧、状态迁移、门控决策、凭据归属与脱敏，不测插件内部实现。
- 测试模块：`pi-rpc` 能力契约（fake Pi fixture 驱动 probe/prompt/follow_up/abort/事件顺序）、凭据 Provider（内存假 OS vault + 失败注入）、配置事务（保留/轮换/回滚）。
- 先例：现有 `pi-rpc-adapter` 契约测试、`pi_configuration_contract`、dsh `test-support` 与 snapshot 体系。
- 组装层：一个 keyless snapshot 覆盖“首轮 prompt → 等待开发者 → follow_up → 工具门控 deny/allow → abort”，用 fake Pi 重放。
- 真实 Pi/模型请求不属于本规格自动化范围；仍需工单 14 授权的交互式宿主，验收前保持 `not-run`。

## Out of Scope

- Git 基线/归因/交付证据、文件写入租约、脱敏投影、工作区信任策略、第一方 extension gate、Tauri 宿主适配：后续独立插件规格。
- OpenCode 清理：按 ADR-0074 随基座迁移执行，不在本规格。
- 真实模型请求与真实 UI 验收：属工单 14，未授权前不执行。
- 非 Windows 平台：首期保持 Windows。

## Further Notes

- 本规格按 `to-spec` 起草，待 seams 与用户确认后发布；发布目标为仓库 issue tracker（GitHub Issues），当前环境未登录 `gh`，需用户提供发布通道或确认以本地文档为准。
- 任何采纳前需重新核对 dsh 上游提交与文档（developer preview 快速迭代）。
