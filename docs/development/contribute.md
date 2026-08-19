# Halo Studio 贡献指南

## 1. 开发流程

仓库使用 Matt 工作流：

1. `/to-spec`：把已讨论的需求综合为规格，发布到 issue tracker（GitHub Issues，远端 `Nyzeep/Halo-Studio`）并打 `ready-for-agent` 标签；
2. `/to-tickets`：把规格拆成 tracer-bullet 工单，声明阻塞边；宽重构按 expand–contract 分批；
3. `/implement`：逐票实现，票内使用 `/tdd`（红→绿→重构放到 review 阶段）与 `/code-review`；
4. 构建/测试出现难以定位回归时切换到 `/diagnosing-bugs`，先建红绿反馈回路再修；
5. 涉及模块接口/seam 设计时使用 `/improve-codebase-architecture` 与 `/codebase-design` 词汇。

## 2. 分支与提交规范

- 新工作基于最新 `main` 建分支，分支前缀 `codex/`（例如 `codex/halo-studio-debrand-20260818`）；
- 提交信息使用中文，一次提交对应一个可独立审查的逻辑单元；
- 禁止改写 Git 历史、禁止 force push、禁止自动合并到 `main`；合并由用户决定；
- 推送使用系统凭据（HTTPS），远端只推新分支。

## 3. 代码审查要求

每票合入前执行 `/code-review` 双轴审查：

- **Standards**：是否符合仓库文档化标准（`AGENTS.md`、`CONTRIBUTING.md`、各目录 `AGENTS.md`）与 Fowler 代码味道基线；
- **Spec**：是否忠实实现来源规格/工单，是否有范围蔓延。

两轴并行、独立报告，不合并重排结论。

## 4. i18n、主题与脱敏规则

- Locale id、别名、回退规则由 `src/shared/i18n/contract/locales.json` 拥有；修改后运行 `pnpm run i18n:generate`；
- 共享稳定文案放 `src/shared/i18n/resources/shared/<locale>/terms.json`；工作流文案归产品面所有；
- 用户可见日期/时间/数字使用共享 i18n 格式化助手；
- 修改后运行 `pnpm run i18n:contract:test` 与 `pnpm run i18n:audit`；基线是 no-growth 约束，不得为过关放宽；
- 主题/颜色：修改 CSS 变量、widget 载荷、移动端、安装器或 CLI/TUI 颜色后运行 `pnpm run theme:color-audit:all`；基线不得通过提高数值来通过；
- 日志：全英文、无 emoji；诊断日志统一脱敏，不记录密钥、模型请求/响应明文；
- 凭据：只保存凭据引用，不提交任何密钥/令牌/私钥文件。

## 5. 命名与品牌

- 本项目命名空间使用 `halo_studio`、`halo-*`、`@halo-studio/*`；
- 上游（原 BitFun）只作为历史/上游对照出现，统一标注“历史记录/上游对照（已归档）”；
- 禁止在活动文档、UI、日志中把 Halo Studio 表述为 BitFun 品牌；
- 真实外部域名（如 openbitfun.com）与上游仓库标识保留并标注，不伪称从未依赖。
