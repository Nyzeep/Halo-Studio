# 03B - 将 P0 受管执行器从 OpenCode Server 切换为 Pi RPC

**What to build:** 将 Halo Studio P0 的唯一生产受管执行路径重新定为本机 Pi Agent。Halo Workbench Runtime 受控启动 `pi --mode rpc`，通过 stdin/stdout 上严格 LF 分隔的 JSONL command/response/event 流驱动 Pi；OpenCode Server、`opencode serve`、HTTP/SSE、Pi TUI 和 Unix/CBOR PiServer 不再是 Windows P0 目标。

**Blocked by:** 03A1 - 接入 Halo Studio 正式 Web UI 并完成 Halo 品牌适配.

**Status:** ready-for-agent

## 决策

- P0 固定链路为：`Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。
- Halo Workbench Runtime 继续拥有工作区信任、任务状态、权限决议、文件写入租约、脱敏、证据和生命周期唯一权威；Pi 拥有 Provider、模型、原生 Session 和工具循环。
- RPC framing 只接受 LF (`\n`) 记录分隔；输入可剥离尾部 CR，但不得按 Unicode 行分隔符切帧。每条 command 使用可选 `id` 做 response 关联，event 独立按序规范化。
- P0 使用 `prompt`、`follow_up`、`abort`、`get_state`、`get_entries` 以及 `message_update`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`agent_settled` 等必要事件；不把完整消息、原始工具结果、Session ID 或 Entry ID 暴露给 Renderer 或证据。
- P0 不使用 Pi TUI 作为 Halo 执行接口，不使用 Pi 的 Unix/CBOR PiServer，不引入 HTTP/SSE 或 ACP 兼容层。任何跨平台 PiServer 目标必须先有独立可用性证明。
- P0 受管会话默认使用临时、任务隔离的 Pi 配置和会话目录；可用 `--no-session` 时优先使用内存会话，不能把 Pi 原始 session JSONL 当作交付历史。
- Provider、模型、Base URL、思考级别和凭据引用属于 Halo 非敏感启动配置；凭据由 Halo 从系统凭据存储短暂读取，仅注入受控 Pi 子进程，不使用 `--api-key`、不写入 Pi `auth.json`，也不把 `models.json`/`.pi/settings.json` 的任意内容当作 Halo 配置权威。
- `D:\pi-main` 只读用于协议和行为核对，不复制源码、不建立依赖、不修改该目录；当前安装版的 `pi --version` 结果只形成环境证据，不自动放宽未来版本。

## 验收标准

- [ ] 活动 CONTEXT、ADR、架构、测试说明、迁移规格和 04–15 工单把 Pi RPC 定义为 P0 生产路径；历史 OpenCode 内容均明确标为历史、比较对象或已废弃决策。
- [ ] 新 ADR-0072 被接受，ADR-0071 被标记为 superseded；ADR-0001、0002、0023、0065 及相关活动决策不再把 OpenCode Server 作为 P0 所有者。
- [ ] 工单 04–15 的 `Blocked by` 均显式包含 03B；README 的执行图与工单边一致，且 03B → 04 是重新开始实现的唯一入口。
- [ ] 测试矩阵删除 P0 OpenCode HTTP/SSE 健康、认证、SSE 和 dispose 检查，替换为 Pi executable probe、严格 LF JSONL、RPC command/event、extension UI、取消和清理检查。
- [ ] 工单 09 明确第一方 Pi extension 的 `tool_call` 执行前阻断、`extension_ui_request/response` 一次性决议、超时/协议/extension 错误 fail closed，以及 task-scoped 脱敏 toolCallId 绑定。
- [ ] 文档明确 Pi 的项目信任不是沙箱、默认没有 Halo 权限弹窗，第一方 extension 具有完整宿主权限；未审计的项目/全局扩展、包、Provider extension 和 Pi 原生授权不进入 P0。

## 精确验证命令

在本票文档修改完成后，在独立 worktree 根目录执行：

```powershell
git diff --check
where.exe pi
Get-Command pi -All | Select-Object Name,CommandType,Source,Path
pi --version
rg -n '03B|03b' docs/requirements/halo-tauri-product-migration/README.md docs/requirements/halo-tauri-product-migration/issues
```

`where.exe pi`、`Get-Command pi -All` 和 `pi --version` 的完整输出必须作为环境事实记录；任一命令找不到 Pi 时，不得把文档检查标为 P0 运行时通过。以上命令不发送 prompt，不读取真实凭据，不启动模型请求。

## 当前环境事实（2026-08-02）

- `where.exe pi` 返回退出码 `1`，报告找不到匹配文件。
- PowerShell `Get-Command pi -All` 可以解析 `C:\Users\Nyzee\AppData\Roaming\npm\pi.ps1`、`pi.cmd` 和 `pi` shim。
- `pi --version` 返回 `0.83.0`，退出码 `0`。
- 以上只证明本机存在可调用的版本 shim，不证明 `pi --mode rpc`、协议能力、第一方 extension 或 Provider 凭据边界已经通过；Windows resolver 必须覆盖 `pi.cmd`，不能只依赖 `where.exe pi` 的成功结果。

## 不在本票

- 不实现 Pi Runtime、第一方 extension、Provider 配置、真实 UI 或任何运行时代码。
- 不启动 `pi --mode rpc` 真实会话，不发送真实模型请求，不读取真实凭据。
- 不修改 GitHub #9–#14，不修改或清理当前 issue-04 worktree 的未提交 OpenCode 实现。
