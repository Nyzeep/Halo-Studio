# 07 - 在 Tauri 运行时探测并启动 Pi RPC

**What to build:** Halo Workbench Runtime 的 Pi RPC Adapter 可以探测用户本机 Pi、受控启动 `pi --mode rpc`、完成真实版本和协议能力检查，并把就绪或失败状态投影到工单 04 的 Interface。版本、能力、协议或子进程启动失败均如实显示，不回退模拟协议。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；04 - 建立 Halo Workbench Runtime 公共契约；06 - 管理 Pi Provider、模型与系统凭据.

**Status:** ready-for-agent

## 实现边界

- Main 侧只探测和启动本机 `pi`; 公开快照只包含版本、能力、状态、稳定错误码和恢复建议。
- Pi 进程使用受控工作区、临时 config/session、白名单环境和 `--no-extensions`; 不建立 TCP/HTTP listener，不使用 TUI 或历史 OpenCode transport。
- 本票只证明协议和生命周期 readiness；不读取真实凭据、不发送 prompt、不执行真实模型回合。

## 兼容性档案

- P0 档案名为 `pi-rpc-p0`；版本检查只是入口，真正放行依赖所需 RPC 能力和语义。
- 必需能力至少包括：`pi --mode rpc` 启动、严格 LF JSONL 分帧、command response ID 关联、`prompt`、`follow_up`、`abort`、`get_state`、`get_entries`、`message_update`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`agent_settled`、`extension_ui_request` 和 `extension_ui_response`。
- Pi 原生 session/config 路径、Provider/model 载荷、事件载荷和 extension 路径封装在 Adapter 实现中，不进入 Halo 公共 Interface。
- 启动参数边界必须按 Pi 原生语义记录：`--provider` 和 `--model` 只接受 Halo 已校验的非敏感选择；`--session-dir` 只指向 Halo 创建并可清理的受管目录；`--no-session` 用于不持久化的受管任务或 readiness fixture。标准持久会话的具体保留策略由工单 05 完成，在此之前不得把 `PiRpcSessionMode::Standard` 静默当成持久会话。
- Pi 的 `models.json` 只作为 Provider/model 元数据与可用性输入；`auth.json`/OAuth 由 Pi 原生认证边界管理，Halo 不读取、展示或写回；`settings.json`、项目 `.pi` 配置及自动发现 extension 在 P0 隔离并关闭。显式固定 extension 只能通过 `--no-extensions` 加精确 `--extension` 路径加载。
- 新主版本、RPC schema 变化或 extension API 变化必须建立新档案；未知版本或能力探测不完整必须失败关闭。

## 验收标准

- [ ] probe 使用 PATH/用户选择解析到的本机 `pi`，记录可公开版本与能力结论；不下载、打包、升级或从 `D:\pi-main` 运行源码。
- [ ] start 只创建受控 Pi 子进程，使用 `pi --mode rpc`、明确工作区、隔离 config/session 目录、白名单环境和 `--no-extensions`；不创建 HTTP listener，不把 session/config 路径或凭据进入公开状态。
- [ ] 标准会话使用 Halo 管理的可持久 Pi session；受管任务使用隔离 session/config，任务结束、取消或中断后清理原始会话状态，符合标准/受管保留策略分离。
- [ ] readiness 必须完成进程启动、严格 LF JSONL reader、`get_state` 成功和必需能力校验；仅观察 stdout 文本或进程存活不算就绪。
- [ ] framing 只以 LF (`\n`) 分隔输入记录；读取时可剥离一条尾部 CR (`\r`)，不得把 U+2028/U+2029 当作换行或记录边界。command response 的可选 `id` 必须支持匹配和单请求无 id 兼容；乱序 response、未知 id、多个 pending 时无 id response、坏 JSON 和嵌入换行均失败关闭。
- [ ] `get_entries` readiness 必须验证 `entries` 数组和可空 `leafId`；当返回 leaf cursor 时，必须用 `since` 请求增量并验证其响应，`since` 不匹配或失败不能伪造 ready。
- [ ] ready、failed、stopping 和恢复建议经 Halo Workbench Runtime 投影；Pi stderr、原始 JSONL 和 extension 错误先脱敏、限长，再进入诊断。
- [ ] stop 先发送 RPC `abort`，在受控宽限期后关闭 stdin 并回收子进程；失败或超时必须有确定强制清理结果。
- [ ] 工作区切换、信任撤销、应用退出和并发 start/stop 不留下孤儿 Pi 进程、session/config 材料或可复用凭据。
- [ ] 生产路径不存在旧 JSONL、ACP、Pi TUI、Unix/CBOR PiServer、HTTP/SSE 或模拟执行器的静默回退。

## 验证要求

- 受控替身测试覆盖兼容通过、不支持版本、能力缺失、无效 JSON、错误 framing、事件流断开、abort 失败、强制回收和敏感字段脱敏。
- Windows 集成测试绑定真实 Pi 子进程并断言严格 LF reader、`get_state` readiness、EOF、退出后进程和临时目录释放；不得发起模型请求。
- 工单 14 至少执行一次已安装 Pi 的真实 probe/start/RPC/stop 资格验证；本票只准备能力检查，证据只记录版本、档案和脱敏结论。

## 精确验证命令

```powershell
where.exe pi
Get-Command pi -All | Select-Object Name,CommandType,Source,Path
pi --version
pnpm --dir "product/Halo Studio" run check:repo-hygiene
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio" run desktop:build:fast
git diff --check
```

前三条只做可执行文件/版本探测，不启动 `pi --mode rpc`，不发送 prompt，不读取真实凭据。

## 不在本票

- 不创建真实标准或受管 Agent 回合；工单 05 和 08 分别负责。
- 不复制 Pi Provider/Core/Session/Agent 源码，不修改用户全局 PATH 或 Pi 安装，不启动 Pi TUI。
