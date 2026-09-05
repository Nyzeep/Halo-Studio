# DankMaterialShell（DMS）设计语言研究（2026-09-05）

> 状态：研究输入，服务 Halo Studio UI 向「niri + DMS 范式」重构的决策票
> 主源：`AvengeMedia/DankMaterialShell`（GitHub, MIT）README、仓库结构、主题参考文档

## 1. 项目事实

- 仓库 `AvengeMedia/DankMaterialShell`，**MIT**，7.9k stars，活跃（最新推送 2026-09-04），官网 danklinux.com。
- 定位：Wayland 桌面 shell，**Quickshell（QML）+ Go** monorepo，为 niri/Hyprland/Sway/MangoWC/labwc/Scroll/MiracleWM 优化；niri README 也官方点名它。目标是"replaces waybar, swaylock, swayidle, mako, fuzzel, polkit"——整套桌面体验的一体化 shell。
- 架构分层：`quickshell/Modules`（面板/挂件/overlay 等界面组件）、`quickshell/Services`（系统集成）、`quickshell/Widgets`（可复用控件）、`quickshell/Common`（主题与共享资源）；`core/`（Go 后端：dms daemon、CLI、IPC、matugen 内嵌实现）。
- 与 Halo 的工艺相似点：插件系统带 **plugins.lock.json**（钉住每个插件的精确 git commit，可跨机复现）——与 Halo 的 `skills-lock.json` 同一思路。

## 2. 组件面清单（README「Features」+ 截图分区）

| 表面 | 内容 | 对 Halo 的映射潜力 |
|---|---|---|
| Spotlight launcher | 应用/文件（dsearch）/emoji/运行中窗口/计算器/命令的聚合搜索，插件可扩展 | 命令面板（受管任务的创建/切换/取消等操作入口） |
| Control Center | 网络/蓝牙/音频/显示/夜模式统一面板 | 设置与模型/凭据配置面板的交互范式 |
| 通知中心 | 分组、富文本、键盘导航 | Agent 操作请求（一次性决议）的通知流呈现 |
| Dashboard | dgop 驱动的实时 CPU/RAM/GPU 指标 + 进程管理 | 运行时任务/进程健康面板 |
| 条（bar）与 dock | 顶部状态条 + 常驻启动器 | 工作区切换轨 + 当前任务状态条 |
| 会话管理 | 锁屏/空闲/自动挂起（AC/电池分别设置） | 无直接对应 |
| 剪贴板历史/媒体/日历/天气 | 桌面级功能 | 无直接对应 |
| 设置应用 + dank-greeter 前端 | 完整设置树 | 设置信息架构参考 |

**与 niri 的集成点**（README：Works best with niri）：完整工作区切换、**overview 集成**、显示器管理——即 DMS 的条/launcher/overview 操作全部驱动 niri 的条带-工作区模型。这正是「niri 提供空间模型、DMS 提供表面与视觉」的分工。

## 3. 视觉语言（design token 层）

- **动态取色**：从壁纸生成全套配色，内嵌 Go 版 [matugen](https://github.com/InioX/matugen)（`core/internal/matugen/sourcecolor.go`——Material You 的 source color 生成算法），统一铺到 GTK/Qt/终端/编辑器；终端另走 dank16。
- **MD3 token 词汇**（`.agents/skills/dms-plugin-dev/references/theme-reference.md`，`Theme` 单例强制全站使用、禁止硬编码）：
  - 颜色角色：`surface`、`surfaceContainerLow/…/Highest`（五层表面容器）、`onSurface`、`onSurfaceVariant`、`outline` 等——**标准 Material Design 3 角色**；
  - 尺度：字号 4 档（12/14/16/20 × fontScale）、图标 3 档、间距 5 档、圆角 3 档；
  - 立体与动效：`ElevationShadow.qml`、`Anims/DankAnim/DankColorAnim`（统一动画原语）。
- 「Theme 单例 + 禁止硬编码」的纪律本身值得移植：Halo web-ui 应有等价的 CSS custom properties 层。

## 4. 对 Halo Studio web-ui 的可移植性判断

**MIT = 代码与设计都可合法移植（保留归属）；QML → web 的转译是机械性的。**

| DMS 模式 | 可移植 | 建议 |
|---|---|---|
| MD3 颜色角色 + surfaceContainer 五层 | **是，直接** | 落成 CSS custom properties（`--surface-container-low` 等），深浅双主题呼应 ADR-0038 |
| matugen 式动态取色 | 是（可后置） | 从工作区图标/品牌色生成主题属 P1 锦上添花 |
| Theme 单例纪律 | **是，高价值** | token 层 + lint 禁止裸值，配合 ADR-0036/0037 的品牌工作 |
| Spotlight 式命令面板 | 是 | 替代上游 BitFun 式菜单导航，成为「会话内命令」（ADR-0030）的入口 |
| 通知中心式 Agent 操作请求流 | 是 | 「一次性决议」卡片的呈现范式（分组、键盘导航） |
| Control Center 式设置分区 | 是 | 模型/凭据/诊断导出设置的信息架构 |
| Dashboard 实时指标 | 部分 | 任务运行时健康（token 用量、进程状态）投影 |
| compositor 专属（锁屏/挂起/媒体/夜模式） | 不可移植 | 与应用内 UI 无关 |

## 参考

- README（主源）：https://github.com/AvengeMedia/DankMaterialShell
- 主题 token 参考：仓库内 `.agents/skills/dms-plugin-dev/references/theme-reference.md`
- 文档站：https://danklinux.com/docs · 插件注册表 https://plugins.danklinux.com
