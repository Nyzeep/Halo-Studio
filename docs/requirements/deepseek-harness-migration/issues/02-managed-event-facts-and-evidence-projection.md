# 02 - 受管事件事实与交付证据投影（总规格）

**待实现内容：** 在 Halo Workbench Runtime 内建立由 Halo 自己拥有的事件事实日志，并从它、任务基线和工作区指纹投影独立且可冻结的交付证据快照。开发者可以在应用重启后重新打开任务状态和交付审查；未完成执行明确中断，覆盖文件后来变更时证据明确过期。Pi/DSH 只经 Adapter 提供脱敏摘要，Runtime 保持单一权威，人工接受或拒绝始终是交付结论的唯一入口。

**阻塞于：** 无（ADR-0075 已接受；本工单是总规格与子任务锚点）。

**状态：** ready-for-agent

## 交付范围

- 事件事实具有 Halo 本地身份、顺序、schema 版本、闭合种类和受控摘要；重复输入、旧 schema、大小上限和脱敏有确定契约。
- 事件事实支撑运行轨迹、当前受管任务快照恢复和交付证据投影，但不等于交付证据，不保存原始 Pi/DSH 数据或完整聊天内容。
- 交付证据由事实、任务基线和工作区指纹独立生成并冻结；覆盖文件后来改变时证据变为 `stale`，回放不能自动接受或拒绝。
- 重启不自动恢复运行、不自动重放 prompt、工具、执行器或 Agent 操作请求；不一致明确投影为 `interrupted` 或 `stale`，等待本地开发者处理。
- UI 继续只使用 Halo Workbench Runtime 的 typed snapshot、intent 和投影事件，不直读内部事实存储或外部执行器日志。

## 子任务与阻塞关系

1. [03 - 建立受管事件事实契约与内存 Adapter seam](03-establish-managed-event-facts-contract-and-memory-adapter.md) — 无阻塞，可先实施。
2. [04 - 让 Runtime 写入事件事实并恢复安全快照](04-record-managed-event-facts-and-restore-runtime-snapshots.md) — 被 03 阻塞。
3. [05 - 投影交付证据、新鲜度与 typed UI 状态](05-project-delivery-evidence-freshness-and-typed-ui-state.md) — 被 04 阻塞。

## 验收标准

- [ ] 三个子任务按上述 blocker-first 顺序完成各自可验证的垂直切片。
- [ ] 事件顺序、重复、旧 schema、重启恢复、脱敏、大小上限、证据新鲜度和人工决议不能由回放替代，均有契约测试。
- [ ] P0 仍只有 `Halo Workbench Runtime → Pi RPC Adapter → pi --mode rpc`；没有新增第二执行权威、替代服务器或前端直连。
- [ ] 真实 DSH/Pi 会话验收如未在验收工作区实际执行，明确保持 `not-run`，不以 fake 或文档替代。

## 不在本票

- 将 DSH `SessionEvent`、存储或 Cordis 生命周期直接接入 Halo P0。
- 自动恢复、自动重放、自动工具执行、自动接受/拒绝交付。
- OpenCode/ACP/旧 Sidecar 回退、Pi RPC 替换或 TypeScript/Cordis 基座迁移。
