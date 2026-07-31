# 04 - 建立 Halo Workbench Runtime 公共契约

**What to build:** 在 BitFun Desktop/Tauri seam 建立一个深的 Halo Workbench Runtime Module。Halo 前端只通过它的小型 Interface 读取工作区、运行时、会话和结构化事件，并提交有限意图；Module 的实现封装 OpenCode 执行适配、信任、配置、任务、证据和生命周期。工单 04 先固定 Interface、状态所有权和可替换测试 Adapter，不在本票启动真实 OpenCode 或发送真实模型消息。

**Blocked by:** 03 - 启动 Halo 品牌的 Tauri 开发工作台；03A1 - 接入 BitFun 正式 Web UI 并完成 Halo 品牌适配.

**Status:** ready-for-agent

## 架构边界

- Halo Workbench Runtime 是 Halo 工作区、标准/受管会话投影、任务状态和有序事件的唯一权威 Module；不得在 React、分散 Tauri command 或旧 Sidecar 中建立第二套状态机。
- 在 Module 内定义 P0 专用的小型 OpenCode 执行 Interface；生产 Adapter 身份为 `opencode-server-1.x`。测试替身只验证同一 Interface，不得演化成通用多执行器分派或生产回退。
- Interface 表达 Halo 意图和结果，例如运行时快照、打开工作区、开始/发送/停止会话、解决操作请求和订阅事件；不得向前端暴露 OpenCode URL、端口、Authorization、原始 Session/Message 标识或原生事件载荷。
- OpenCode HTTP/SSE 路径、工作区路由头、Provider/模型载荷和兼容差异属于 Adapter 实现，不属于前端 Interface。
- 复用 BitFun 已有 Agent Runtime owner 与 Tauri 装配点；应用 deletion test，删除新 Module 后复杂度应重新扩散到多个调用方，而不是只少一层转发。

## 实施起点

- `product/Halo Studio/src/apps/desktop/src/runtime/` 的 `DesktopRuntimeContext` 只作为组合宿主：注入 owner、持有桌面生命周期并调用稳定 Interface，不拥有任务、权限、Session 或凭据业务规则。
- 平台无关的任务状态、权限/取消策略和执行事实进入现有 Agent Runtime/能力 owner；只在确有跨层消费时把稳定 DTO/port 放入 `src/crates/contracts`，不建立通用 facade。
- OpenCode HTTP/SSE 投影进入 `src/crates/adapters` 的 Halo 专用外部执行 Adapter；进程、操作系统凭据和本机服务实现进入 `src/crates/services`；P0 能力选择与装配进入 `src/crates/assembly`。
- 在 `product/Halo Studio/src/apps/desktop/src/lib.rs` 的 Tauri 装配点注册少量薄的 Halo command/event adapter；不要让前端从现有数百个 BitFun command 自行拼装 Halo 状态机。
- 正式前端改动只进入 `product/Halo Studio/src/web-ui` 的 Halo 产品路径；不得恢复 `src/halo-workbench` 静态生产入口。
- BitFun `src/crates/interfaces/acp` 中的 OpenCode 路径使用 `opencode acp` stdio，不是目标 Server Adapter；`src/crates/adapters/opencode-adapter` 只做静态配置/扩展投影，也不得冒充执行链。
- 旧 `sidecar/crates/halo-runtime/src/opencode.rs` 可作为进程、认证、健康、事件和中断行为参考，但不得被产品树直接依赖。

## 验收标准

- [ ] 公共 command Interface 覆盖：读取完整 runtime snapshot、打开/关闭活动工作区、创建标准或受管逻辑会话、发送用户输入、请求停止/结束以及提交一次性操作决议。
- [ ] 公共 event Interface 只有一个有序事件流，包含本地关联标识、单调序号、事件种类、脱敏摘要和状态版本；快照可用于首连和事件缺口恢复。
- [ ] Runtime snapshot 至少区分 `disconnected`、`probing`、`starting`、`ready`、`failed`、`stopping`，并提供稳定错误码与可操作恢复建议。
- [ ] 前端只依赖该 Module 的 Interface，不直接调用 OpenCode、旧 Sidecar、`src/halo-workbench` 静态壳或多组相互重叠的 BitFun command。
- [ ] `halo-local-coding` 正式入口不把 BitFun 内置模型执行或 ACP 会话作为并行权威；可复用 BitFun UI/基础设施，但 Halo P0 会话事实只来自 Workbench Runtime 的 OpenCode 执行链。
- [ ] 能力 owner 通过窄 port 接收 OpenCode Adapter、凭据读取、工作区事实和时钟；Desktop 只注入实现并投影结果。测试与调用方跨越同一个 seam，不测试私有函数。
- [ ] 应用关闭、工作区切换和 Module drop 都会触发一次有序清理；并发 start/stop、重复命令和迟到事件有确定结果。
- [ ] P0 UI 不显示 Code Agent/Pi/多执行器选择器；运行时能力只报告 OpenCode 生产 Adapter 是否真实可用。

## 验证要求

- Rust 契约测试：snapshot、状态迁移、事件顺序/缺口、幂等、并发 start/stop、迟到事件、敏感字段剥离和资源清理。
- Tauri Interface 测试：从注册 command/event seam 验证序列化形状、稳定错误码与前端可观察状态。
- 前端测试：断言真实 runtime snapshot 驱动连接状态，并扫描不存在 OpenCode 端口、Authorization、原始远程标识或旧 Sidecar 调用。
- `cargo test`/前端测试、Halo scope 检查和 `git diff --check` 必须记录精确命令与退出码。

## 不在本票

- 不启动真实 `opencode serve`，不录入真实凭据，不发送真实模型请求。
- 不复制 `D:\opencode-dev` 或旧 `sidecar/crates/halo-runtime/src/opencode.rs`；后者只作为行为与测试语义参考。
- 不实现 Pi、BitFun 内置 Code Agent、多执行器选择或旧 JSONL 兼容层。
