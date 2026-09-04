---
status: accepted
date: 2026-09-05
related: 0076 条带工作台 UI 架构; 0065 Tauri seam 深 Module; 0075 事件事实与证据投影; 0038 亮暗主题; 0036/0037 产品标识
decision-map: Nyzeep/Halo-Studio#32（工单 #46 DMS 设计 token 与主题架构、#47 前端技术栈选型、#49 ADR supersede 波次）
---

# 前端整体重写下延续 React/Vite/Tauri 技术栈并收敛 token 与状态层

## 决策

（决策地图 #32；#47）2.0 重写的是 UI 架构（条带范式、组件与信息架构，见 ADR-0076），不是基座框架：

- **框架基座保留 React 18 + Vite + TypeScript + zustand + Tauri v2**；Tauri 插件面、i18next、markdown/Monaco/xterm 组件层平移。重写不引入新框架、新状态库或新构建链。显式判定标准是 **agent 可维护性**：框架与范式选择以「agent 能在既有语料与工具链下正确修改、测试和重构前端」为准，凡需要 agent 重新学习私有范式的方案一律不取。
- **token 层**（#46）：
  - 纯 CSS custom properties（`tokens/` 目录），采用 DMS 的 MD3 角色命名（`--surface-container-low…highest`、`--on-surface`、`--outline`）+ 三档圆角、五档间距、四档字号 × fontScale。
  - 组件样式一律 CSS Modules，移除 sass；lint 禁止裸值——颜色、间距、圆角必须引用 token（DMS「Theme 单例、禁止硬编码」的 web 等价物）。
  - `[data-theme]` 双主题：dark/light 两套角色值，默认随 prefers-color-scheme（ADR-0038 延续）；品牌色（ADR-0036/0037）作为 `--primary` seed 注入，不设第二套主题文件。
  - 动效 token 层：duration/easing 各三档，全局尊重 prefers-reduced-motion（ADR-0076 手势集引用）；动态取色为 P1。
- **状态两层分离**（#47）：`WorkbenchRuntimeStore`（zustand）只持 Runtime 投影——Snapshot 与运行事实事件的 reduce，对齐 ADR-0075；`WorkbenchUIStore` 持条带交互瞬态（焦点列、Overview、手势状态）。两店边界即 ADR-0080 durable/live 事件域边界的 UI 投影：UI store 不得持有事实、凭据或证据状态，事实状态只来自 Runtime 投影。
- **虚拟化收敛**（#47）：长列表虚拟化统一 `@tanstack/react-virtual`，移除上游遗留的 `react-virtuoso`。

## 后果

- 旧 UI 的组件实现、sass 样式与混合状态管理不迁移；新表面按六表面清单（ADR-0076）在 token 层与双 store 之上重建。
- 禁裸值 lint 使主题与品牌调整收敛到 token 层一处；双主题只需维护两套角色值映射。
- DMS（DankMaterialShell）设计语言只作角色命名与视觉规则来源，不引入其实现代码；MIT 归属不受影响（ADR-0052）。
- 条带 shell、Overview 与命令面板作为实现票依次落在 token 层与双 store 之上；i18n 工程化机制显式延后（见 2.0 重构规格「显式延后」）。
