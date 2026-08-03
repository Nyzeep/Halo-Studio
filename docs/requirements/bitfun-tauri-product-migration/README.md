# BitFun/Tauri 产品迁移

**Status:** ready-for-agent

本迁移把 Halo Studio 建立为 BitFun 的受控下游产品，并将已验证的受管交付行为迁入 Halo 品牌的 Tauri 工作台。

- [迁移规格](00-bitfun-tauri-product-migration-spec.md)
- [实施工单](issues/)

## 当前检查点

- 工单 01–03A1 已建立可迁移能力基线、受跟踪 BitFun 产品树、Halo Tauri 入口和正式 BitFun Web UI；UI 对齐不等于模型或 Agent 执行链已经接入。
- 新增工单 03B：将 P0 受管执行器从历史 OpenCode Server 决策切换为 Pi RPC，并固定 P0 目标链路。
- 下一项是工单 04：在 Tauri seam 建立深的 Halo Workbench Runtime Module；04 及后续工单均阻塞于 03B。
- P0 只实现本机已安装 Pi 的生产执行 Adapter：Halo Workbench Runtime 受控启动 `pi --mode rpc`，通过 stdin/stdout 严格 LF JSONL 使用 Pi 的 Provider、模型、Session 与 Agent 工具循环。
- `D:\pi-main` 只作为只读协议与行为参考，不复制源码、不建立依赖、不修改该目录；Pi TUI、Unix/CBOR PiServer、HTTP/SSE 与历史 OpenCode Server 不属于 Windows P0 生产路径。

## 执行依赖

## P0 Pi RPC contract map for issues 04–15

All tickets in this slice refer to one production identity, `pi-rpc-p0`, and
the same Halo Workbench Runtime seam. The ticket-specific responsibility is:

| Issue | Pi RPC responsibility and evidence |
| --- | --- |
| 04 | Halo-local command/event Interface and the deep Runtime owner; no raw Pi identifiers. |
| 05 | Standard-session lifecycle and isolated `--session-dir`; managed sessions use `--no-session` or a disposable directory. |
| 06 | Non-sensitive `--provider`/`--model` selection and `credential_ref`; `models.json` metadata is bounded, while `auth.json`/OAuth remains Pi-owned and unread by Halo. |
| 07 | Windows executable resolution and readiness: `get_state`, `get_entries` with `entries`/nullable `leafId`, then `since` cursor validation; strict LF JSONL and the `--provider`/`--model`/`--no-session`/`--session-dir` boundary. |
| 08 | `prompt`, message/tool events, and `agent_settled`; prompt acceptance or `agent_end` is not task settlement. |
| 09 | First-party `tool_call` gate, matched `extension_ui_response`, one-shot task/session/tool-call binding, and fail-closed behavior. |
| 10 | Same-session `follow_up`, explicit finish, read-only review, and evidence freshness. |
| 11 | `abort`, EOF/crash handling, grace period, forced collection, no replay, and temporary-resource cleanup. |
| 12 | Forward behavior-equivalence matrix from the historical six issues; no protocol or raw-identifier equivalence claim. |
| 13 | First-party extension source/version/hash/dependency/host-permission/update/license inventory and upstream sync gate. |
| 14 | Only authorized real Pi RPC native UI acceptance; no manual Pi process, Pi TUI, HTTP/SSE, or fake-Pi substitution. |
| 15 | Final reference audit and retirement of superseded OpenCode paths only after all preceding gates pass. |

The canonical `get_entries` readiness rule is: validate the `entries` array and
nullable `leafId`; when a leaf cursor is returned, issue `since` and validate
the increment. A missing, mismatched, or malformed cursor never becomes
`ready`. Input framing is LF-only; one trailing CR may be stripped, while
U+2028/U+2029 are data, not line delimiters.

The active OpenCode scan allowlist is limited to the explicitly historical or
comparison material: ADR-0071; the old issue-07 and issue-14 documents; the
historical product-requirements document; the historical sections of the core
verification guide; and the OpenCode rows in the migration map. Any
`opencode serve`, HTTP/SSE, OpenCode Server adapter registration, or old auth/
health check outside that allowlist is an active-document or production-path
failure.

`03A1 → 03B → 04 → 06 → 07 → 05 → 08 → 09 → 10 → 11 → 12 → 14 → 15`

工单 13 依赖 03B 和 04，并可与 06–12 的实现并行；工单 14 同时受 12 和 13 阻断。编号表达需求来源，`Blocked by` 才是实际执行顺序。04 是新的实现起点；未完成 03B 时不得开始 04。

## 旧六票策略

GitHub #9–#14 保持原始需求、状态和历史验收证据，不因执行器切换改写、重开或关闭。它们只作为可迁移能力基线；工单 12 负责把这些行为前向映射到新的 Pi RPC-backed Tauri 产品，工单 14/15 才负责真实 UI 与最终发布验收。

工单 07–15 中出现的 OpenCode Server、`opencode serve`、HTTP/SSE、Basic Auth 和 OpenCode Server Adapter 只可作为历史比较对象或已废弃决策出现，不能作为 P0 生产路径。当前 issue-04 worktree 中未提交的 OpenCode 实现不改变本规格；后续实现者须按 03B 将可迁移语义移植到 Pi RPC，或在经过审计后废弃，不得直接合并为 P0。Pi extension 的安装、版本、依赖、权限和许可证核对由 13 负责，工具请求的第一方阻断由 09 负责。

任何阻断未完成时不得跳到真实 UI 验收或旧产品收缩。
