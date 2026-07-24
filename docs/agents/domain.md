# Domain docs

Halo Studio 采用 single-context 文档布局。

## Before exploring

- 阅读仓库根目录的 `CONTEXT.md`（存在时）。
- 阅读 `docs/adr/` 中与当前主题相关的 ADR（存在时）。

## Layout

```text
CONTEXT.md       # 产品领域术语表，只记录稳定的领域语言
docs/adr/        # 仅记录难以逆转、存在明确取舍的架构决策
```

需求、计划、实现细节和临时笔记不写入 `CONTEXT.md`。在需求对齐过程中确认术语后，再按需创建该文件和 ADR。
