# Pi RPC 适配

## 1. 目标

P0 中，Halo Workbench Runtime 通过 `halo-pi-rpc-adapter` 受控启动本机 `pi --mode rpc`，把 Pi 的 Provider、模型、Session 与 Agent 工具循环规范化为 Halo 运行事实。适配器是深模块内的执行 seam（ADR-0065、ADR-0072），不是 Pi 源码的复制，也不与前端直连。

## 2. 启动与能力检查

启动前必须通过真实探测：

- 可执行文件解析与版本探测；
- `--mode rpc` 启动与严格 LF JSONL framing 握手；
- 能力档案包含 `prompt`/`follow_up`/`abort`/`get_state`/`get_entries`、事件、extension UI、取消与清理语义；
- 新主版本或不兼容协议必须先建立新档案，运行时按档案放行。

未通过检查时 fail-closed，不降级、不静默放行。

## 3. 配置与凭据引用

- 受管启动配置由 Halo Workbench Runtime 解析并投影给适配器：Provider、模型、Base URL、思考级别、凭据引用与受控启动选项；
- 凭据明文只在子进程启动或 Provider 认证瞬间短暂存在，不落盘、不进日志、不进交付历史；
- 产品配置只保存凭据引用（操作系统凭据存储条目名）；系统存储不可用时失败关闭；
- `HALO_USER_ROOT`/`HALO_HOME` 作用域内的配置与日志由 Halo 管理，不自动改写用户的 Pi 全局配置、`auth.json` 或项目扩展配置（配置事务需 Diff 预览与确认）。

## 4. 一次性决议

高风险外部操作（浏览器/系统级 Computer Use 对工作区外写入、上传、下载、剪贴板写入、进程/系统控制等）每次都必须通过 Agent 操作请求获得本地开发者的一次性决定（ADR-0012）；不创建会话级或永久放行规则。

## 5. 中断语义

- 取消：Workbench Runtime 向当前 Pi RPC 会话发送原生 `abort`，宽限期后关闭 stdin 并回收子进程，记录最终取消方式；
- 中断：应用或运行时意外退出前未进入终态的状态如实记录为中断，不自动恢复或重放；
- 强制回收：abort 响应超时不得延长强制回收宽限期；已回收的会话生成号不可复用。

## 6. 契约测试

`halo-pi-rpc-adapter` 的契约测试覆盖：

- 启动/握手/能力检查（`pi_configuration_contract.rs` 11 项）；
- 传输与协议 fail-closed、取消与回收、脱敏与事件投影（`pi_rpc_contract.rs` 43 项）；
- 凭据引用与配置事务（含回滚）；
- 扩展 UI 一次性决议与敏感参数脱敏。

运行：

```powershell
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter
```

测试通过受控 fixture（fake Pi 进程）验证公开 seam 行为，不依赖真实模型或网络。
