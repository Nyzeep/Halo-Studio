# DeepSeek Harness 基座迁移对照审计

记录日期：2026-08-16

本审计以只读方式对照 Halo Studio 的权威产品边界与 DeepSeek Harness（`dsh`）源码结构，为后续“以 `dsh` 为新基座”的迁移提供能力映射、缺口与动作清单。它不是放行证据；任何迁移结论仍需在目标基座上完成工单 14/15 的真实验收与完整复验。

## 参考事实

- dsh 参考检出：`D:\DeepSeek Harness\deepseek-harness`，提交 `47f943859bef60e4160492346772ded9b24f765a`（2026-08-13，`feat/npm-public`），MIT，developer preview（存在破坏性变更承诺）。
- 架构：everything-is-a-plugin，基于 vendored Cordis；插件贡献 service/typed event/reversible effect；profile（命名组合）→ bundle（可安装分层）→ patch 覆盖；能力 seam = Service Definition / Service Provider / Consumer 三角色。
- 包分组（`packages/`）：core（session/system-prompt/tools/agent/agent-loop/scope）、llm、shell、subprocess、terminal、fs、lsp、skill、web、compaction、context、subagent、bundle、workflow、todo、plan、preset、guard、self-modification、hooks、session、identity、settings、credentials、acp、interaction、boot、sdk、examples、support、util，以及 sandbox、goal、jobs、mcp、workspace、code-runtime、schedule、spill、storage、session-query、feedback、runtime-diagnostics、extensions、attachment、api、client、host。
- 其余：`native/landlock-run`（进程沙箱）、`python/`（SDK 与运行时）、`apps/{cli,web}`、`examples/`、`docs/`、`website/`。

## Halo 能力清单（权威来源）

来源：`CONTEXT.md`、`docs/architecture/target-product.md`、ADR-0065～0074、工单 03B～15。P0 核心事实：

- 唯一生产受管执行器：本机 Pi RPC（`pi --mode rpc`，LF JSONL，`prompt`/`follow_up`/`abort`/`get_state`/`get_entries`）。
- Halo Workbench Runtime 位于 Tauri 接缝，持有工作区信任、任务状态、一次性决议、脱敏、交付证据与生命周期权威。
- 两种模式：标准编码模式（Halo Studio 原生会话/工具/历史/Git）与受管交付模式（信任工作区、任务基线、运行轨迹、只读审查、接受/拒绝）。
- 安全边界：系统凭据引用、统一脱敏、中断不重放、接受/拒绝不自动改 Git、第一方 extension 一次性门控。

## 映射表

| Halo 能力 | dsh seam / 包 | 置信度 | dsh 证据 | 动作 |
| --- | --- | --- | --- | --- |
| 受管任务 turn：prompt/reply/waiting_developer/follow_up | `core/agent-loop` + `core/session` 的 turn/step 驱动与 inbox | 直接 | `packages/core/agent-loop`、`packages/core/session` | 用 dsh turn 语义表达受管回合；waiting_developer = turn 关闭后 inbox 等待下一条消息 |
| Pi RPC 执行适配 | `llm` 能力 seam / 自定义驱动 | 缺口 | `packages/llm`（Streaming 词汇 + adapter seam） | 自研 `dsh-pi-rpc` 插件：Provider/Consumer 角色承载 `pi --mode rpc`，复用 Halo 现有契约测试与兼容档案 |
| 会话持久化与中断不重放 | `core/session` 追加式 `SessionEvent` 日志 + `agent/*` 事件 | 直接 | `packages/core/session`、`packages/session` | 中断状态由 session 日志推导；Halo 的 interruption-history 作为投影插件 |
| 运行轨迹（结构化过程视图） | `session/event` 派生投影 | 直接 | `packages/session`、`packages/session-query` | 保留 Halo 脱敏轨迹投影 |
| 一次性 allow/deny 与工具执行前门控 | `interaction`（approval/permission/commands/ask-user）+ `tools/*` pre-execute 瀑布 | 直接 | `packages/interaction`、`packages/core/tools` | Halo 第一方 extension gate 实现为 interaction provider + tools 事件监听 |
| 标准编码模式工具集 | shell/subprocess/terminal/fs/lsp/skill/web/workflow/todo/plan/subagent/mcp/code-runtime | 直接 | 对应 `packages/*` | 用 dsh profile 组合；Halo Studio 旧 UI 行为改为 dsh consumer |
| 标准/受管双模式隔离 | `preset`（每会话 agent 组合）+ `core/scope`（按 agent 作用域注册） | 直接 | `packages/preset`、`packages/core/scope` | managed/standard 各一个 preset；受管策略不扩散 |
| 系统凭据引用（Windows Credential Manager、一次性录入） | `credentials`（credential-reference + env/.env provider） | 部分 | `packages/credentials` | 新增 OS Credential Manager provider，沿用 Halo 的 `credential_ref` 与 one-shot 语义 |
| Provider/model/baseUrl/thinking 配置权威 | `settings` + `llm` 配置 + `preset` | 部分 | `packages/settings`、`packages/llm`、`packages/preset` | Halo 配置服务映射为插件 config；保留 write-only Base URL 与 provider 绑定校验 |
| 工作区信任（canonical path、显式信任） | `workspace` + `fs` + `sandbox` | 部分 | `packages/workspace`、`packages/fs`、`packages/sandbox` | Halo 信任策略插件：路径校验与信任状态为 Halo 权威 |
| 文件写入租约与人工介入 | `fs` 能力事件 + `interaction` | 缺口 | `packages/fs`、`packages/interaction` | 自研 lease 策略插件（独占写、冲突等待、人工介入暂停） |
| Git 基线/归因/只读审查/接受拒绝不变性 | dsh 无独立 git 包 | 缺口 | `packages/fs`、`packages/subprocess`（间接） | 自研 `dsh-git-delivery` 插件：基线快照、归因、证据新鲜度、禁止自动暂存/提交/推送 |
| 交付证据与脱敏 | `session` 投影 + `runtime-diagnostics` | 部分 | `packages/session`、`packages/runtime-diagnostics` | 自研 redaction 插件：在 session/event 投影边界统一脱敏与限长 |
| 原生 Tauri 工作台 | `apps/web` + `packages/client` + `sdk` JSON-RPC | 部分 | `packages/client`、`packages/sdk`、`apps/web` | Halo Tauri 壳内嵌 dsh web/client 或经 SDK 驱动 `ctx.agents`；保留 halo-scope 导航与隔离守卫 |
| 进程沙箱与子进程生命周期 | `subprocess` + `sandbox`（landlock-run） | 直接 | `packages/subprocess`、`packages/sandbox`、`native/landlock-run` | Windows 首期评估 sandbox provider 覆盖；Pi 子进程由 Halo 受控启动不变 |
| 配置事务与回滚 | `settings` + `storage` | 部分 | `packages/settings`、`packages/storage` | 保留 Halo 配置事务（diff 预览、冲突、原子写、回滚）语义 |
| 第三方/项目扩展隔离 | `bundle` + `settings` + `credentials` + `guard` | 部分 | `packages/bundle`、`packages/guard` | 受管模式保持 `--no-extensions` 等价的 fail-closed 组合 |
| ACP / hooks 桥 | `acp`、`hooks`（Claude Code/Codex hook 桥） | 排除 | `packages/acp`、`packages/hooks` | 按 Halo P0 边界不作为执行接口；仅作为历史比较对象 |

## 关键缺口（需自研为 dsh 插件）

1. `dsh-pi-rpc`：Pi RPC LF JSONL 驱动与兼容档案检查（对接 Halo 现有 pi-rpc-adapter 契约测试）。
2. `dsh-credential-manager`：Windows Credential Manager provider（沿用 `halo-pi-credential-v1-` 引用与 provider 绑定校验）。
3. `dsh-git-delivery`：任务基线、归因、证据新鲜度、只读审查、接受/拒绝不变性。
4. `dsh-file-leases`：文件写入租约与人工介入。
5. `dsh-redaction`：session/event 投影的统一脱敏与限长。
6. `dsh-trust`：工作区信任与 canonical path 策略。
7. `dsh-halo-extension-gate`：第一方 permission-gate 的一次性 interaction provider。
8. Tauri 宿主适配：Halo 壳 ↔ `dsh` client/SDK/事件流。

## 迁移建议顺序

1. 在独立迁移分支建立 `dsh` 作为 vendored/依赖基座，冻结版本并记录兼容档案（dsh 为 developer preview，先锁定本次审计提交）。
2. 先实现插件 1、2（Pi RPC 执行 + 凭据库），复用 Halo 现有 pi-rpc-adapter/配置契约测试做行为等价。
3. 再实现 3、4、5（交付/租约/脱敏），以工单 12 行为等价矩阵为输入。
4. 接入 Tauri 宿主与 halo-scope 守卫，执行工单 14 真实 UI 验收与工单 15 完整复验。
5. 随迁移执行 ADR-0074 的 OpenCode 清理清单与新基座最终扫描。

## 风险

- dsh 仍处 developer preview，存在兼容性破坏与快速迭代；迁移过程中需以提交级锁定并记录升级路径。
- 当前 Halo 的 Git/证据/租约/脱敏语义没有 dsh 现成等价物，自研插件是主要工作量。
- Windows 首期要求（路径、凭据、进程沙箱、WebView2/Tauri）需在目标基座重新验收，不能由现有自动化直接放行。
- 本审计基于 2026-08-13 的 dsh 检出；后续采纳前应重新核对上游提交与文档。
