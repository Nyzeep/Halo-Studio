# niri 交互模型研究（2026-09-05）

> 状态：研究输入，服务 Halo Studio UI 向「niri + DMS 范式」重构的决策票
> 主源：`YaLTeR/niri`（已迁至 niri-wm org）README 与官方文档站，v26.04

## 1. 项目事实

- 仓库 `github.com/YaLTeR/niri`（README 内链接已指向 niri-wm org），Rust 编写，**GPL-3.0**，27.4k stars，最新 release **v26.04**（calver，2026-04-25），文档站 niri-wm.github.io/niri。
- 官方定位："A scrollable-tiling Wayland compositor"，"stable for day-to-day use"。
- **许可证约束（决策票输入）**：GPL-3.0 意味着**不可复制代码**进 Halo 产品树；只能借鉴交互范式与行为语义（设计思想不受版权保护，但实现必须原创）。
- 血统：深受 GNOME Shell 上的 PaperWM 启发；同类实现有 karousel（KDE）、scroll/papersway（sway/i3）、Hyprland scrolling layout——说明「可滚动平铺」是可移植的**交互范式**，不绑定 compositor。
- README 明确：niri 本身不是完整桌面环境，官方点名 **DankMaterialShell** 与 Noctalia 作为配套 shell——用户「niri + DMS」的组合是官方推荐组合。

## 2. 核心交互模型（README「About」逐条）

1. **无限右向条带上的列**：窗口按列排在一条向右无限延伸的条带（strip）上；**打开新窗口从不引起既有窗口重排/缩放**——这是与 i3/sway/Hyprland 平铺的本质差异（后者会挤压既有窗口）。
2. **每显示器独立条带**：窗口永不"溢出"到相邻显示器。
3. **动态工作区垂直排列**：每个显示器有独立工作区集合，底部永远存在一个空工作区（类似 GNOME）；显示器断开时工作区迁移、重连时迁回。
4. **Overview**：把工作区与窗口缩小縮远呈现的导航层（README Features 首位提及，配视频）。
5. 其余特性：窗口可分组为 **tabs**、触摸板+鼠标**手势**、可配置布局（gaps/borders/struts/窗口尺寸）、Oklab/Oklch 渐变边框、背景模糊、动画（可自定义 shader）、配置热重载、屏幕阅读器可达。

## 3. 对 Halo Studio（Tauri web-ui 工作台）的可转译性判断

Halo 不是合成器：没有「窗口」概念，转译对象是**面板/卡片/任务**。逐项判断：

| niri 原语 | 可转译 | 转译建议 |
|---|---|---|
| 右向无限列条带 | **是，核心** | 受管任务卡按列排在可横向滚动的条带上；打开新任务卡不挤压既有卡——直接替换 ADR-0027 的「三栏工作台」空间模型 |
| 列内堆叠（隐含） | 是 | 同一任务的会话流/证据/日志纵向堆叠在列内 |
| 永不重排 | 是，且契合 Halo | 进行中任务的视觉位置稳定，符合「交付证据新鲜度」需要的位置连续性 |
| 每显示器独立条带 | 部分有意义 | 应用内可映射为「工作区（Git workspace）→ 独立任务条带」：条带互不溢出对应 ADR-0028 任务列表按工作区隔离 |
| 垂直动态工作区 + 底部恒空 | 是 | 左侧/顶部的工作区切换轨；「底部恒空工作区」转译为「永远可以新建任务」的视觉承诺 |
| Overview 缩放导航 | 是 | 任务多时的缩略导航层（全部条带的 zoom-out 总览），替代传统标签页 |
| tabs 分组 | 是 | 同一工作区的多个任务列分组 |
| 手势导航 | 谨慎 | 触摸板双指横滚驱动的条带平移在 web 内可实现；触摸手势优先级低（桌面产品） |
| gaps/边框/动画/模糊 | 是 | 纯视觉层，随 DMS 设计 token 走 |
| screencast/截图/xwayland 等 compositor 能力 | 不可转译 | 与应用内 UI 无关 |

**结构性冲突提示（进 wayfinder）**：此转译**推翻 ADR-0027**（dense three-pane workbench）并**实质性超越 ADR-0018**（保留上游 BitFun 工作台交互）——去 BitFun 化后 ADR-0018 的约束对象消失，正好一并解决；但两条 ADR 需要正式 supersede 流程（`/domain-modeling` 或 wayfinder 决策票处理）。

## 参考

- README（主源，本文所有交互事实出处）：https://github.com/niri-wm/niri（raw 读取于 2026-09-05）
- 文档站：https://niri-wm.github.io/niri/ · Workspaces/Overview/Tabs/Layout 各页见 README 链接
- LWN 综述（二手，仅佐证）：https://lwn.net/Articles/1025866/
