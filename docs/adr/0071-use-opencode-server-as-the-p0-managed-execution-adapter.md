---
status: accepted
supersedes: 0002-bitfun-runtime-as-the-single-managed-coding-owner.md execution ownership; 0002-verified-dual-agent-delivery-workflow.md and 0023-use-locally-installed-pi-and-opencode-executors.md for P0 executor scope
---

# 使用 OpenCode Server 作为 P0 唯一受管执行 Adapter

Halo Studio P0 只实现一个生产受管执行 Adapter：由 Halo Workbench Runtime 受控启动用户本机已安装的 OpenCode 1.x `serve` 进程，并通过经过兼容性验证的回环 HTTP/SSE Interface 使用 OpenCode 的 Provider、模型、Session、Prompt、Agent 工具循环、权限、澄清、取消和清理能力。OpenCode 负责模型连接与原生 Agent 执行；Halo 负责工作区信任、系统凭据引用、受管任务状态、一次性决议、脱敏、交付证据和生命周期。

该 Adapter 的实现必须使用随机回环端口、每次启动的新认证材料、受控子进程环境和真实健康/能力检查。OpenCode 的原始端口、Authorization、Session/Message 标识、完整对话与原始工具日志不得跨越 Halo Workbench Runtime 的公开 Interface，也不得进入诊断或交付证据。

Halo 不复制或分叉 OpenCode 内部 Provider、模型注册表、Session 数据库或 Agent 循环源码。`D:\opencode-dev` 只作为本机只读协议研究快照；它没有 Git 元数据，不是可审计导入源、构建依赖或运行时依赖。生产运行依赖用户可见的本机 OpenCode 安装和版本/能力档案，旧 Halo Sidecar 的 `opencode.rs` 只提供可迁移行为参考，不迁移其 JSONL 进程边界。

Pi 与 BitFun 内置 Code Agent 不进入 P0 的选择器、配置、测试矩阵或发布门槛。未来增加第二个生产 Adapter 时，必须另立 ADR、定义兼容性与安全档案，并证明第二个 Adapter 足以使执行器 seam 从假设变为真实变化点。
