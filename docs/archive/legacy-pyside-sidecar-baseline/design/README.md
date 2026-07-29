# 设计文档目录（docs/design/）

依据 `requirements-alignment/03-ide-editor-and-reference-alignment.md`。

## 当前发布依据

真实 Agent 首个可用版本以 [04 - 真实 OpenCode 受管任务会话：需求对齐与范围统一](../requirements-alignment/04-real-opencode-managed-task-session.md) 与其 [06 - 规格](../requirements-alignment/06-real-opencode-managed-task-session-spec.md) 为当前发布边界。10–13、15 号设计的实现状态以下表为准，但不改变该发布边界；14 号真实协议设计在该对齐记录所列 P0 修订下实施。

## 编号规则

- `references/R1–R5`：五个参考开源项目的分析报告（输入材料）。
- `10–15`：本项目各模块的开发设计文档（每份对应一个落地开发子代理 + 代码审查）。
- 后续新增模块设计沿用两位数递增编号；修订以追加“修订记录”小节的方式进行，不覆盖原结论。

## 目录

| 编号 | 文档 | 状态 |
| --- | --- | --- |
| R1 | [references/R1-vscode-analysis.md](references/R1-vscode-analysis.md) — VS Code 工作台布局/编辑器组/命令面板/主题令牌 | 已完成 |
| R2 | [references/R2-zed-analysis.md](references/R2-zed-analysis.md) — Zed 原生编辑器/设计语言/面板停靠/Agent 面板 | 已完成 |
| R3 | [references/R3-opencode-analysis.md](references/R3-opencode-analysis.md) — OpenCode 真实服务协议与适配器差距 | 已完成 |
| R4 | [references/R4-pi-analysis.md](references/R4-pi-analysis.md) — Pi 真实 RPC 协议与适配器差距 | 已完成 |
| R5 | [references/R5-bitfun-analysis.md](references/R5-bitfun-analysis.md) — BitFun 分层/边界/审查生命周期借鉴 | 已完成 |
| 10 | [10-ide-shell-and-design-language.md](10-ide-shell-and-design-language.md) — IDE 壳层与设计语言 | 已完成 |
| 11 | [11-editor-core.md](11-editor-core.md) — 编辑器内核 | 已完成 |
| 12 | [12-fs-contract-and-explorer.md](12-fs-contract-and-explorer.md) — 文件系统契约与资源管理器 | 已完成 |
| 13 | [13-command-palette-and-quick-open.md](13-command-palette-and-quick-open.md) — 命令面板与快速打开 | 已完成 |
| 14 | [14-agent-protocol-alignment.md](14-agent-protocol-alignment.md) — Pi/OpenCode 真实协议对齐 | 已完成 |
| 15 | [15-differentiation-features.md](15-differentiation-features.md) — 差异化功能 | 已实现（F1–F5） |

## 设计文档统一提纲

每份 10–15 号文档必须包含：

1. **目标与范围**（含明确的“范围外”）
2. **参考结论引用**（引用 R1–R5 的具体小节，注明借鉴什么、不借鉴什么）
3. **与现有契约的关系**（对 `docs/ipc-protocol.md`、`docs/module-contracts.md` 的增量，逐条列出）
4. **详细设计**（类型/API/QML 组件树/线程与数据流）
5. **差异化点**（如适用）
6. **实施计划**（新建/修改文件清单，与其他模块的依赖顺序）
7. **测试计划**（单元/集成/UI 各层）
8. **风险与缓解**
