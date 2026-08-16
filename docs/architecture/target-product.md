---
status: authoritative
---

# Halo Studio 目标产品架构

Halo Studio 是 BitFun 的受控下游产品。仓库跟踪完整的 BitFun 源码关系并持续获取上游更新，但 Halo 的变更、验证、提交和发布只发生在自己的产品仓库中。

## 产品边界

- `product/` 是长期受跟踪的 Halo 产品树，也是迁移完成后的唯一桌面构建入口。
- 桌面应用使用 Halo 品牌的 Tauri 工作台，并保留 BitFun 成熟的本地编码交互骨架。
- 首期只装配本地编码主链；办公协作、Mini App、远程、Relay 和移动端能力不进入构建、路由、导航或初始化。
- 仓库外的 BitFun 上游参考树只用于获取和检查候选更新，正式构建不得依赖其绝对路径。

## 运行时边界

BitFun 提供产品基座和工作台能力；Halo Workbench Runtime Module 位于 Tauri 接缝，是 Halo 工作区信任、编码会话投影、受管任务、权限决议、Git、配置、凭据引用、脱敏、证据和结构化事件的唯一权威。它向前端提供小而稳定的 command/event 接口。

前端不得分别直连大量底层命令，也不得调用旧 Halo Sidecar。旧 `stdio JSONL v1` 只作为行为迁移证据，不是兼容目标。

## P0 执行链

P0 只有一个生产受管执行 Adapter：本机已安装的 Pi RPC。Runtime 受控创建 Pi 子进程并驱动：

`Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`

RPC 输入输出严格按 LF (`\n`) 分帧；客户端可剥离输入记录尾部的 CR，但不能把 Unicode 行分隔符当作协议分隔符。Runtime 只在内部消费 `prompt`、`follow_up`、`abort`、`get_state`、`get_entries` 及已验证的 message/tool/settled 事件，并将它们转换为 Halo 本地事件。Pi 原始 session/entry 标识、完整会话、工具参数和结果、命令输出、凭据、Authorization 与原始 JSONL 不进入 Renderer、日志、持久化或证据。

Pi TUI、Unix/CBOR PiServer、HTTP/SSE、历史 OpenCode Server 和 ACP 均不是 Windows P0 执行接口。任何新传输必须先有独立 ADR 和跨平台可用性证明。

## 产品模式

- **标准编码模式**沿用 BitFun 原生会话、工具、历史与 Git 能力，不产生 Halo 受管交付结论。
- **受管交付模式**由开发者显式接入受信任 Git 工作区，记录任务基线、一次性操作决定、结构化运行轨迹和可审查交付。
- 两种模式共享安全的模型与凭据配置服务，但受管任务的信任、证据和人工决策边界不会扩散到标准模式。

## 安全边界

- 凭据明文只在系统凭据存储读写和执行器启动瞬间短暂存在；产品状态只处理凭据引用。
- 日志、事件、配置、备份和交付证据统一脱敏。
- 接受或拒绝交付不自动暂存、提交、推送、回滚、删除文件或改写 Git 历史。
- 中断任务不会自动重连、重发消息、重放一次性决定或重复写入。

## 迁移门槛

历史 PySide/QML 与 Rust Sidecar 基线已通过工单 15 的独立收缩变更移除，仅 `docs/archive/legacy-pyside-sidecar-baseline/` 保留为历史比较对象；仓库、脚本与发布配置只指向 `product/Halo Studio` Tauri 产品。完整发布验证仍以工单 14 真实 Pi RPC 原生 UI 验收和工单 15 复验记录为准。OpenCode Server 相关内容只作为历史比较对象，不是当前验收门槛。

具体决策以 ADR-0065 至 ADR-0072 为入口，实施顺序以 BitFun/Tauri 迁移规格和工单为准；新的实现入口是 03B → 04。
