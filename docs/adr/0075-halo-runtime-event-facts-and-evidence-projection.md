---
status: accepted
related: 0065 Halo Workbench Runtime seam; 0072 Pi RPC P0 Adapter
---

# 在 Halo Workbench Runtime 内建立事件事实与交付证据投影

Halo Studio 需要在应用重启后重新打开受管任务和交付审查，同时不能把 Pi 或 DeepSeek Harness（DSH）的原始会话、日志或状态提升为产品权威。本决策在 ADR-0065 的深 Halo Workbench Runtime Module 内建立 Halo 自己的事件事实日志，并使交付证据保持为独立、可冻结的派生快照。

## 决策

- Halo Workbench Runtime 是事件事实的唯一权威。Pi RPC、DSH 或未来执行 Adapter 只能提供外部输入；Runtime 在 Adapter seam 后规范化、脱敏、限长并追加保存后，才形成 Halo 事实。原始 Pi/DSH session、message、JSONL、工具参数、工具输出和凭据不进入事件事实、Renderer、普通日志或交付证据。
- 事件事实日志是按受管任务追加的版本化事实序列，而不是完整聊天记录或执行器日志。第一阶段记录任务生命周期、脱敏且限长的用户消息与 Agent 回复摘要、工具活动摘要、高风险 Agent 操作请求及一次性决议、文件变更指纹、任务基线关联、交付证据版本和证据新鲜度变化。每条事实具有 Halo 本地身份、顺序、时间、闭合事实种类和 schema 版本；相同事实身份的重复输入必须有确定的幂等结果，旧 schema 必须保持可读。
- 事件事实的存储、恢复、保留和删除由 Halo 本地历史策略管理。存储 Adapter 必须把脱敏和大小上限收口在 Runtime 所有的深 Module 内；持久化实现不能把原始外部日志当作回退数据源。用户可以按本地历史策略删除事实与证据。
- 运行轨迹、任务快照恢复和交付证据投影都以事件事实为输入，但它们不是同一对象。Renderer 只经 Halo Workbench Runtime 的 typed Interface 消费快照和事件投影，不得直读事件存储、Pi 日志或 DSH 数据。
- 交付证据是以事件事实、任务基线和工作区指纹按规则派生并冻结的独立快照。它记录覆盖范围和新鲜度；覆盖文件后来发生变化时必须变为 `stale`。事件回放、恢复或外部执行器状态不得自动接受、拒绝、重新执行或重放交付，只有本地开发者可以作出接受或拒绝决定。
- 应用重启只恢复当前受管任务的 Halo 快照和可重新打开的交付审查页，不恢复运行中的执行。若事实、工作区或外部执行器状态不一致，任务明确变为 `interrupted`，或相应证据明确变为 `stale`，并等待开发者再次决定。

## 后果

- Runtime 的 Interface 保持小而稳定：内部事实存储通过窄的 Adapter seam 注入，前端继续只消费 Runtime snapshot、intent 和有序投影事件。这提高 Module 的 depth，并把版本、脱敏、顺序、恢复和投影的复杂性集中在一个位置，保持维护 locality。
- DSH `core/session` 的 append-only、纯投影和持久化协调思想可作为设计参考；DSH `SessionEvent`、Cordis 生命周期、存储 schema 和 replay 不是 Halo 的 Interface 或权威。DSH 是 developer preview，任何真实 DSH runtime 集成仍须独立规格、兼容性证明和验收。
- 本决定不改变 Pi RPC 作为 P0 唯一生产受管执行 Adapter，不增加 OpenCode、ACP、旧 Sidecar、DSH Agent loop 或前端直连的并行权威，也不启动 TypeScript/Cordis 基座迁移。
- 实施按独立 tracer-bullet 任务推进：先建立事件事实契约和内存 Adapter，再接入 Runtime 写入与恢复，最后接入证据投影、新鲜度和 typed UI projection。首个内存契约切片不能被宣称为已完成持久化或真实 Pi/DSH 验收。
