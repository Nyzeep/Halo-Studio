# DeepSeek Harness 结合评估

> 状态：讨论输入，尚未形成迁移规格或实现任务
> 日期：2026-08-18

## 目的

评估 Halo Studio 是否、以及以什么方式吸收 DeepSeek Harness（DSH）的 Agent 能力，同时保持 Halo Studio 当前 P0 的产品边界：本地开发者、受信任 Git 工作区、Halo Workbench Runtime 单一权威、Pi RPC 唯一生产受管执行 Adapter、交付证据与人工审查。

## 已确认事实

### Halo Studio

- 正式产品源码树是 `product/Halo Studio/`，入口为 Tauri 桌面应用。
- Halo Workbench Runtime 位于 Tauri seam，是管理受管任务、运行事实、文件写入租约、交付证据和人工决策的深 Module。
- P0 受管执行链固定为 `Halo Workbench Runtime -> halo-pi-rpc-adapter -> pi --mode rpc -> LF JSONL`。
- P0 不允许前端直连 Pi、不允许旧 Sidecar/ACP/OpenCode Server 形成并行权威，不允许发现式加载第三方扩展。
- `CONTEXT.md` 明确把“Agent 任务”定义为有限、显式、工作区绑定的编码请求，而不是无限制自由聊天或自动委派。
- ADR-0074 已接受“将 OpenCode 全量移除延后至 DeepSeek Harness 基座迁移”，但尚未定义迁移规格、阶段切分、兼容矩阵或验收任务。

### DeepSeek Harness

- DSH 是 developer preview，README 明确警告存在兼容性破坏变更。
- DSH 采用 Cordis 的 everything-is-a-plugin 架构：模型适配器、工具、session log、agent loop 都是可组合的插件。
- 核心能力包括：`ctx.agentLoop` agent loop、`ctx.tools` 工具管线、`ctx.fs` 文件系统能力、`ctx.sandbox` 沙箱、`packages/interaction/approval` 审批、`core/session` append-only SessionEvent log、`packages/skill` skills、`packages/workflow` workflows、`packages/todo` 目标管理，以及 profiles/bundles/patches 装配机制。
- DSH 的扩展点是事件和 capability seam；model-visible 输入必须进入 durable session log，UI 从 log 投影。

## 核心判断

不建议第一阶段把 Halo Studio 直接改造成开放式 Agent 平台，也不建议把 DSH 的 Cordis plugin tree 原样嵌入 P0 受管交付路径。两者的权威模型不同：DSH 以可替换插件树和通用 Agent loop 为中心；Halo 以受管任务的安全、归因、新鲜证据和人工决议为中心。

更稳妥的方向是先把 DSH 当作“可验证的 Agent runtime 能力参考/候选执行 Adapter”，把能提高 Halo leverage 的能力收敛到 Halo Workbench Runtime 的现有 seam 后面。任何 DSH 能力进入 P0 之前，都必须有来源、版本、权限、凭据、文件写入、事件脱敏、取消和证据新鲜度的独立契约与真实验收。

## 待探讨候选

### A. DSH Agent Loop 作为第二执行 Adapter

- 关注模块：`Halo Workbench Runtime`、`halo-pi-rpc-adapter`、未来 DSH Adapter、`src/crates/contracts/runtime-ports`。
- 价值：验证 Halo 的受管任务 Interface 是否足够深，未来可以在不改变前端和交付审查的情况下替换 Agent loop。
- 主要风险：增加 P0 多执行器选择、双重 session authority、审批模型和文件写入模型冲突；DSH developer preview 的版本漂移会扩大发布面。
- 讨论前提：是否把它定义为 P1 实验性 Adapter，而不是 P0 替代；是否允许只读/沙箱验收工作区。
- 建议强度：Worth exploring。

### B. DSH SessionEvent 投影用于交付证据与运行轨迹

- 关注模块：Halo Workbench Runtime 的事件投影、`WorkbenchRuntimeSnapshot`、交付证据，以及 DSH `core/session`。
- 价值：借鉴 append-only durable facts、事件重放和 model-visible 可重建性，减少当前运行轨迹、活动会话记录和交付证据之间的概念漂移。
- 主要风险：DSH 原始 session log 不能直接成为 Halo 交付证据；完整工具日志、模型内容、凭据和原始 JSONL 仍必须排除；证据新鲜度和人工决议不能被 log replay 自动替代。
- 讨论前提：只借鉴事件分类和投影思想，还是要建立受 Halo 脱敏/限长/快照规则约束的本地事实日志。
- 建议强度：Strong。

### C. DSH 工具审批与沙箱能力映射到 Halo Agent 操作请求

- 关注模块：Halo 的高风险外部操作、Pi 第一方 extension 决议、文件写入租约、`ctx.fs`/`ctx.sandbox`/approval。
- 价值：系统化描述“工具调用前策略检查 -> 一次性开发者决议 -> 执行 -> 结构化结果”的流程，并为未来非 Pi Adapter 提供一致的策略检查模型。
- 主要风险：DSH 的通用 approval/sandbox 语义可能被误解为会话级授权；Halo 明确要求高风险外部操作每次一次性决议，且文件写入需要任务级租约和归因。
- 讨论前提：只提取威胁模型和契约测试，还是做一个隔离的 Halo policy Adapter。
- 建议强度：Strong。

### D. DSH Skills/Workflows/Goals 作为标准编码模式的可选扩展

- 关注模块：Halo 标准编码模式、历史 session、Skills/插件/工作流边界、受管工具集。
- 价值：把复杂任务分解、目标续跑、可发现能力和 agent-ready 开发流程引入标准模式，提升个人开发者的自动化能力。
- 主要风险：用户容易把标准模式能力误认为受管交付能力；skills/workflows 可能访问工作区外资源或产生不可审查副作用；“自动委派”与 Halo 当前 Agent 任务定义冲突。
- 讨论前提：是否接受“标准模式可实验、受管模式严格封闭”的双轨；目标是否只做本地可见的任务编排。
- 建议强度：Worth exploring。

### E. 迁移到 TypeScript/Cordis 基座

- 关注模块：整个 `product/Halo Studio` Rust/Tauri 产品树、Halo Workbench Runtime、前端装配、历史 OpenCode 清理。
- 价值：直接获得 DSH plugin/bundle/profile 生态，降低自建 Agent runtime 的长期维护成本。
- 主要风险：这是产品基座重写，不是能力接入；会重做 Windows 原生进程/凭据/文件租约/交付证据/桌面宿主契约，且 DSH 当前 developer preview 不适合作为立即替换的稳定基座。
- 讨论前提：只做长期架构研究，还是启动迁移决策地图；在 P0 真实验收前是否冻结基座迁移。
- 建议强度：Speculative。

## 建议讨论顺序

1. 先确定 P0 是否继续以 Pi RPC 为唯一生产受管执行 Adapter。
2. 若确定，优先讨论 B + C：事件事实与策略审批是可复用能力，也不要求立刻引入第二执行器。
3. 再讨论 D：明确标准编码模式的 Agent 化边界。
4. 最后才讨论 A 或 E：它们需要独立 ADR、兼容矩阵和真实验收工作区。

## 需要用户决定的问题

1. 你希望 Halo Studio 的“Agent 化”首先解决什么：更强的自主编码、可复用任务编排、可审查交付，还是把产品迁移到 DSH 基座？
2. P0 是否继续坚持 Pi RPC 单一生产受管执行 Adapter，DSH 只作为后续 P1 Adapter/能力参考？
3. 标准编码模式是否可以实验 DSH 的 skills/workflows，而受管交付模式继续封闭？
4. 你是否接受第一阶段只改文档、契约和实验性集成，不直接改核心产品实现？
5. 讨论结果是否需要发布为 GitHub Issue；若需要，应该先发布一份总规格 Issue，还是先发布一个决策型 Issue 只回答“Pi + DSH 的关系”？

## 权威关系

本文件是讨论输入，不覆盖 `CONTEXT.md` 或 accepted ADR。形成选择后，应进入 `docs/requirements/` 规格、`docs/adr/` 决策和 GitHub Issues；未确认的设想不得写入领域词汇表。

## 参考

- 本地 DSH checkout：`D:\DeepSeek Harness\deepseek-harness`
- DSH architecture：`D:\DeepSeek Harness\deepseek-harness\docs\architecture.md`
- DSH README：`D:\DeepSeek Harness\deepseek-harness\README.md`
- Halo 架构：`docs/development/architecture.md`
- Halo Pi RPC：`docs/development/pi-rpc-adapter.md`
- Halo 现行基座迁移决策：`docs/adr/0074-defer-opencode-removal-until-deepseek-harness-migration.md`
