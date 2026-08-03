# Halo Studio 文档地图

本目录只让权威文档指导当前实现。历史资料可以证明过去的行为，但不能覆盖当前词汇、决策、架构或需求。

## 权威顺序

发生冲突时按以下顺序处理；冲突本身必须被修正，不能长期依赖读者自行判断：

1. `CONTEXT.md` 定义领域词汇，不承载实现决策。
2. `adr/` 记录已接受及被替代的架构决策。
3. `architecture/` 描述当前目标产品结构。
4. `requirements/` 描述待实施规格、工单和验收边界。
5. `verification/` 记录已执行事实，不扩大产品需求。

## 目录

| 路径 | 角色 | 可否指导新实现 |
| --- | --- | --- |
| `adr/` | 决策日志；以状态和 supersede 链判定有效性 | 仅 `accepted` ADR 可以 |
| `architecture/` | 当前目标产品架构 | 可以 |
| `requirements/` | 当前规格与 agent-ready 工单 | 可以 |
| `verification/` | 自动化和人工验收事实 | 只能证明事实 |
| `archive/` | 不可变历史基线 | 不可以 |

## 维护规则

## Worktree document synchronization

- `D:\Halo Studio\.worktrees\` is the implementation isolation area; root
  `D:\Halo Studio\docs\` is the shared documentation mirror and handoff entry.
- When an authoritative document or ticket changes in a worktree, the same
  relative path must be synchronized to root `docs/` before handoff.
- Historical OpenCode, old protocols, and superseded decisions may remain, but
  they must be labeled historical, comparison material, or superseded.
- Recheck relative links, `git diff --check`, and the active OpenCode scan
  allowlist after synchronization.

- 根目录只保留 `README.md` 与 `CONTEXT.md` 两个项目级文档入口。
- 新的长期文档必须进入上述目录之一，不得只保存在 `.scratch/`。
- `.scratch/` 只存可删除的临时产物；Git 工作树由 `git worktree` 管理，不作为文档存储。
- 归档正文不作事后修订；失效路径和替代关系集中记录在归档索引。
- 重复副本、临时计划和被权威文档完整替代且没有审计价值的资料直接删除。
