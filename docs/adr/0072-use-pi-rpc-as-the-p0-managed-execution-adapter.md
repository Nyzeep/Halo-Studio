---
status: superseded by ADR-0078
supersedes: 0071 for P0 execution ownership and transport
related: 0065 deep Workbench Runtime seam
---

# 使用 Pi RPC 作为 P0 唯一受管执行 Adapter

## ADR 关系

- ADR-0065 remains accepted: Halo Workbench Runtime stays the deep authority at
  the Tauri seam.
- This ADR supersedes ADR-0071 only for P0 execution ownership and transport;
  the OpenCode Server document remains historical evidence and is not a
  compatibility fallback.

Halo Studio P0 只实现一个生产受管执行 Adapter：由 Halo Workbench Runtime 受控启动用户本机已安装的 Pi，并以 `pi --mode rpc` 通过 stdin/stdout 严格 LF JSONL 驱动它。目标链路固定为：

`Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`

Pi 负责 Provider、模型、原生 Session 和 Agent 工具循环；Halo Workbench Runtime 继续负责工作区信任、任务状态、文件写入租约、一次性决议、脱敏、交付证据和生命周期。Renderer 只消费 Halo 的小型 command/event Interface。

## 决策

- RPC client 必须使用 LF (`\n`) 作为唯一 record delimiter；输入可剥离尾部 CR，但不得按 Unicode 行分隔符切帧。command response 使用可选 `id` 关联，事件按到达顺序和 Halo 本地序号规范化。
- 首期只依赖 `prompt`、`follow_up`、`abort`、`get_state`、`get_entries` 和已验证的 message/tool/settled 事件。`agent_end` 不等同于最终结算，`agent_settled` 才表示 Pi 不会继续自动 retry、compaction retry 或 queued continuation。
- Pi extension UI 通过 `extension_ui_request`/`extension_ui_response` 工作；P0 只加载 Halo 第一方、固定版本、来源和许可证已审计的 extension，并用 `--no-extensions` 加显式 `--extension` 路径阻止发现其他 extension。项目本地和用户全局 extension、Pi package、任意 Provider extension 不进入受管路径。
- P0 受管会话使用任务隔离的 Pi 配置和 session 目录；优先使用 `--no-session`，必要时使用 `--session-dir` 指向可清理目录。Pi 的原始 session ID、entry ID、完整会话、凭据、Authorization、命令输出和原始 JSONL 不得进入 Renderer、日志、持久化或交付证据。
- Pi 的 `models.json` 是 Provider/model 元数据和可选认证表达的原生配置面；它可能解析环境变量、命令值或 literal 值，Halo 不能把任意文件内容或命令解析当作安全凭据权威。`settings.json` 及项目 `.pi` 配置可能影响 package、extension 和资源发现，P0 必须隔离并关闭发现式加载。Pi `auth.json`/OAuth 状态属于 Pi 原生认证存储，不由 Halo 读取为产品凭据库，也不由 Halo 写入或展示。
- Halo 配置只保存非敏感 Provider/model/base URL/thinking 选择与 `credential_ref`。凭据从系统凭据存储短暂读取并通过受控子进程环境或已验证的 Pi Provider 认证入口使用；不得使用可被其他进程观察的 `--api-key`，不得由 Halo 写入 Pi `auth.json`。
- P0 不使用 Pi TUI 作为 Halo 执行接口，不使用 Pi 的 Unix/CBOR PiServer，不引入 HTTP/SSE、ACP 或 OpenCode Server 兼容层。任何新的 Pi 传输必须另立 ADR 并完成 Windows 可用性证明。

## 安全边界与剩余风险

Pi 的默认运行行为不是 Halo 权限系统，也没有内建沙箱。Pi 项目信任只控制项目设置、资源、包和 extension 的加载；Pi 进程及 extension 仍以启动用户权限访问本机。Halo 因此必须在 Pi 进程外拥有工作区信任、任务状态和决议权威，并把第一方 extension 的拒绝、超时、协议错误和 extension 错误全部收口为 fail closed。

该决定不能声称 Pi 天然安全。剩余风险包括模型诱导的本地工具操作、仓库提示注入、第一方 extension 代码缺陷、Provider 网络和凭据暴露面，以及 Pi 版本/协议漂移。P0 只记录可验证的进程、协议、权限、脱敏和清理边界；每个新 Pi 版本必须重新通过能力档案和真实验收。

## 来源与范围

协议和行为事实只来自 `D:\pi-main` 的只读参考：

- `packages/coding-agent/docs/rpc.md`：`--mode rpc`、严格 LF JSONL、command、event 和 extension UI 子协议。
- `packages/coding-agent/docs/extensions.md`：`tool_call` 执行前阻断、生命周期事件、`ctx.ui`、extension 错误和 RPC mode 行为。
- `packages/coding-agent/docs/models.md`、`custom-provider.md`：Provider/model 配置、`models.json`、环境变量解析和 `auth.json`/OAuth 边界。
- `packages/coding-agent/docs/session-format.md`：session JSONL、entry tree、稳定 entry cursor 和 session 目录边界。
- `docs/security.md`、`docs/usage.md`、`docs/environment-variables.md`、`docs/settings.md`：Pi 项目信任不是沙箱、`--no-extensions`、`--no-session`、`PI_CODING_AGENT_DIR`、`PI_CODING_AGENT_SESSION_DIR` 和 `PI_OFFLINE`。

该参考目录没有 Git 元数据，不是 Halo 构建依赖、运行时依赖或源码导入源。历史 OpenCode Server 方案由 ADR-0071 保留为 superseded 决策和比较材料，不再构成 P0 生产路径。
