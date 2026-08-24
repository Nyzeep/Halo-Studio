# DeepSeek Harness 结合与基座迁移

本目录承载 DeepSeek Harness（`dsh`）能力结合和可能的基座迁移的规格与本地工单草案。当前 P0 受管执行链仍固定为 Halo Workbench Runtime 经 Pi RPC Adapter 驱动本机 `pi --mode rpc`；DSH 是 developer preview，只能经 Halo Runtime 的 Adapter seam 被评估或吸收，不能不加隔离地成为 P0 权威。

| 文档 | 内容 | 状态 |
| --- | --- | --- |
| [01 - Pi RPC 与凭据 Provider 规格](01-pi-rpc-and-credential-provider-spec.md) | dsh 基座采纳、Pi RPC 执行插件与系统凭据 Provider 的历史迁移规格 | `ready-for-agent`（待 seams 确认） |
| [02 - 受管事件事实与交付证据投影规格](02-managed-event-facts-and-evidence-projection-spec.md) | Halo 本地事件事实、独立证据快照、恢复与新鲜度规则 | `ready-for-agent` |
| [02 - 总规格本地任务草案](issues/02-managed-event-facts-and-evidence-projection.md) | GitHub 总规格 Issue 的本地发布源与子任务锚点 | `ready-for-agent` |
| [03 - 事件事实契约与内存 Adapter seam](issues/03-establish-managed-event-facts-contract-and-memory-adapter.md) | 第一个可验证的 Rust contract/fake 垂直切片 | `ready-for-agent` |
| [04 - Runtime 写入与安全快照恢复](issues/04-record-managed-event-facts-and-restore-runtime-snapshots.md) | 事实写入、脱敏、大小上限和不重放恢复 | `ready-for-agent`，被 03 阻塞 |
| [05 - 证据投影、新鲜度与 typed UI 状态](issues/05-project-delivery-evidence-freshness-and-typed-ui-state.md) | 证据冻结、`stale`、人工决议和 UI 投影 | `ready-for-agent`，被 04 阻塞 |

## 关联决策与背景

- [ADR-0075：事件事实与交付证据投影](../../adr/0075-halo-runtime-event-facts-and-evidence-projection.md) 固定 Halo 的事实和证据权威边界。
- [ADR-0072：Pi RPC P0 Adapter](../../adr/0072-use-pi-rpc-as-the-p0-managed-execution-adapter.md) 固定唯一生产受管执行链。
- [ADR-0074：延后 OpenCode 清理](../../adr/0074-defer-opencode-removal-until-deepseek-harness-migration.md) 保留迁移范围的历史决策。
- [DSH 结合评估](../../architecture/deepseek-harness-assessment-20260818.md) 是讨论输入；它不能覆盖已接受 ADR 或本目录规格。
