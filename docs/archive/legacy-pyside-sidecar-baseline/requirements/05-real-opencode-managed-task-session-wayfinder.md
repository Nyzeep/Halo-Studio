# 05 - 真实 OpenCode 受管任务会话：需求地图

**状态：** 已建图  
**地图类型：** 本地 Markdown 决策地图  
**说明：** 依据本项目“文档统一存放于 `docs/`”的约定，本地图和其决策记录位于 `docs/requirements-alignment/`，不修改既有 `.scratch/` 历史票据。

## Destination

让 Halo Studio 在原生界面中通过真实、兼容的 OpenCode 1.x 启动受管任务会话，完成多轮交流、一次性操作请求、无害文件改动和只读交付审查，并以自动化和真实会话验收共同证明该能力。

## Notes

- 术语以根目录 `CONTEXT.md` 为准。
- 本地图记录需求决定；可实施规格见 `06-real-opencode-managed-task-session-spec.md`。
- 开发工作必须保持单工作区、单任务、凭据不可见、Agent 原生写入和审查只读等既有边界。
- P0 不被 Pi 真实运行或 10–13、15 号 IDE 功能阻塞。

## Decisions so far

- [定义受管任务会话边界](05-real-opencode-managed-task-session-decisions/01-managed-task-session-boundary.md) — 会话是有限、多轮、由开发者显式结束的 Agent 任务，而不是自由聊天。
- [确定真实运行时、兼容性与凭据边界](05-real-opencode-managed-task-session-decisions/02-real-runtime-compatibility-and-credentials.md) — OpenCode 1.x 通过兼容性档案可用，凭据只经 Windows 凭据管理器引用注入。
- [确定 P0 发布范围与真实验收场景](05-real-opencode-managed-task-session-decisions/03-release-scope-and-real-acceptance.md) — 使用验收工作区完成真实会话闭环，IDE 扩展能力后置。
- [确定交互、安全与质量边界](05-real-opencode-managed-task-session-decisions/04-interaction-safety-and-quality-boundaries.md) — 会话内容脱敏且临时保留，权限一次性处理，中断不重放，IPC 是主测试边界。

## Not yet specified

- 每个受支持 OpenCode 次版本的兼容性档案具体字段和适配器测试矩阵，随实现票据落地并受真实会话验收约束。
- 2.x 兼容性档案的准入流程；它不属于 P0，不能从 1.x 支持策略自动推导。
- 10–13、15 号设计在 P0 之后的实施先后顺序；它们不改变本地图的目的地。

## Out of scope

- Pi 真实启动和 Pi 多轮任务会话：本轮不作为发布门槛。
- 通用自由聊天、会话历史云同步、完整对话存档、自动恢复或自动重放。
- IDE 壳层、编辑器、资源管理器、命令面板和差异化编辑功能的实施。
- 自动 Git 操作、自动权限规则、自动委派或自动重试。

## Delivery tickets

已确认的纵向开发票据位于 [07-real-opencode-managed-task-session-tickets](07-real-opencode-managed-task-session-tickets/)。首个可开始的票据是 [受管 OpenCode 1.x 兼容启动](07-real-opencode-managed-task-session-tickets/01-managed-opencode-1x-compatible-startup.md)。
