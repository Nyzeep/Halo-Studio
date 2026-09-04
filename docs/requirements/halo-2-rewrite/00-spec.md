# Halo Studio 2.0 重构规格

**Status:** draft（决策地图 14/14 决议后起草；主会话验收后随规格 issue 发布）

> 决策来源：Halo Studio 2.0 重构决策地图 `Nyzeep/Halo-Studio#32`（工单 #38–#51 全部决议）。权威决策记录为 ADR-0076 至 ADR-0080；本规格只做范围、主线与里程碑的工程切分，不复制其论证。上游冻结声明见 `docs/architecture/upstream-freeze-20260905.md`。

## Problem Statement

Halo Studio 1.x 的产品形态由上游（原 BitFun）导入树、三栏工作台与「单一 Pi RPC Adapter」定义：前端是上游交互骨架的 Halo 品牌化表达，执行基座只有一个生产 Adapter，执行器选择与交接只是词汇表里的预留扩展点。DeepSeek Harness（DSH）成熟为可接入的第二执行器，niri + DMS 范式为工作台提供了更契合「多任务并行 + 位置稳定 + 交付审查」的空间模型，继续在旧形态上叠加将同时付出两套成本。

2.0 重构因此分三条主线推进：执行基座建立双受管 Adapter 与统一执行器端口；前端整体重写为条带空间模型；迁移侧把旧验收链封版为基线、冻结上游树，在新基座首个真实验收后整体删除上游。重构不背旧 P0 发布目标（tag `migration-baseline-20260905`）。

## 目标

1. **执行基座**：Halo Workbench Runtime 保持 Rust 单一权威；halo-dsh-adapter 与 halo-pi-rpc-adapter 同级为生产受管 Adapter，统一收敛到 `ManagedExecutorPort`（ADR-0078；#38–#42）。
2. **事实模型**：运行事实以「不虚构历史」为总原则，committed 粒度、执行器中立 kind、单一脱敏闸门在双 Adapter 路径上统一（ADR-0080；#51/#41）。
3. **前端**：工作台整体重写为 niri 条带空间模型（工作区轨 + 任务条带 + Overview，六表面、手势 P0 集，ADR-0076；#43–#45），技术栈延续并收敛 token 层与双 store（ADR-0077；#46/#47）。
4. **迁移**：上游源码树冻结只读（ADR-0079；#50），旧验收链封版（#48）；新基座首个真实验收通过后整体删除上游树，仓库只余 Halo 自有基座。
5. 每条主线都以可测试的验收语句收口（见里程碑节），旧基线证据在重构期间保持可复现。

## 非目标

- 云同步、多用户协作（产品边界不变）。
- OpenCode Server Adapter 复活（ADR-0074 方向不变，OpenCode 不进入 2.0 受管路径）。
- DSH agent loop 内嵌为 Runtime 第二执行引擎（#41 决议不做；双受管 Adapter 已满足同级诉求）。
- 非 Tauri 宿主形态（移动原生等）。
- 会话中切换主执行器（P0 无运行中重绑定；「执行器交接」是任务级动作，ADR-0078）。
- 对旧三栏工作台、上游装配机制或旧 P0 发布目标做增量投资。
- DSH skills/workflows/goals、运行中模型切换、fork 归因链、compaction、session_stats/export、图片附件（均为 P1，#41/#42）。

## 主线一：执行基座

依据 ADR-0078（工单 #38/#39/#40/#41/#42）与 ADR-0080（工单 #51）：

- **双 Adapter 与端口**：生产受管执行 Adapter 恰为 halo-pi-rpc-adapter 与新增 halo-dsh-adapter 两个同级实现；`ManagedExecutorPort`（runtime-ports）定义共同面——prompt/follow_up/abort/get_entries/决议流/事件投影——并携带每 Adapter 能力档案 flag（pi 有 steer、DSH 无则如实降级）。DSH `sdk` profile 降为协议金丝雀/降级通道，降级时证据不断链。
- **DSH 接入形态**：主通道 = `acp` profile，`session/requestPermission` 与 ADR-0012 一次性决议同构；每受管任务一个受控进程，取消统一为回收阶梯；`SUPPORTED_DSH_PROFILES` 锚 0.1.3-alpha.1 + `initialize` 就绪探测 + fail-closed；CredentialRef 走子进程 env 零落盘，`DSH_HOME` 指向 Halo 管理目录，`.env` 不作注入通道。
- **pi 侧对齐**：0.85.0 档案验收、`@earendil-works` 安装源钉版、`bash` 命令不消费守卫固化（#42）。
- **选择与交接**：工作区默认执行器 + 任务创建时显式覆盖，绑定入任务基线、保持到终态；UI 三则（选择器只在创建处、如实显示身份与能力降级、决议统一渲染为「Agent 操作请求」卡）。
- **事实层**（ADR-0080）：三类核心 kind（用户消息摘要/Agent 回复摘要/工具活动）执行器中立；事实只取 committed 粒度，流式帧只进活动会话记录；attempt 独立记录，取消落地「已交付前缀 + interrupted」；`normalize_summary` 是唯一脱敏/限长/fail-closed 闸门。

## 主线二：前端

依据 ADR-0076（工单 #43/#44/#45）与 ADR-0077（工单 #46/#47）：

- **条带空间模型**：工作区轨 + 任务条带 + Overview；新任务焦点右插、不挤压不重排既有列；每工作区一条带承载 ADR-0028/0029 的隔离语义。
- **六表面**：工作台（主表面）、任务列（会话流 + 工具活动 chips + 内联决议卡 + 交付审查区）、命令面板（Spotlight 式，ADR-0030 入口演进）、Git 面板（ADR-0055–0063 语义，可开合）、设置（分区式）、标准编码模式分层（ADR-0016）。
- **手势 P0 集**：触摸板双指横滚 = 条带横移；滚轮只滚列内；Shift+滚轮 = 条带横移；键盘全集（←→ 焦点列、`n` 新建、`o` Overview、`1..9` 工作区跳转、Esc 退层）；Overview 极端列数分页；全部手势有等价键盘路径；捏合 → Overview 为 P1。
- **技术栈延续**：React 18 + Vite + TS + zustand + Tauri v2 平移，agent 可维护性为显式判定标准；Tauri 插件面、i18next、markdown/Monaco/xterm 组件层平移。
- **样式与状态收敛**：token 层为纯 CSS custom properties（DMS MD3 角色命名），组件样式 CSS Modules 去 sass，lint 禁裸值；`[data-theme]` 双主题，品牌色作 `--primary` seed；动效 token 三档 + prefers-reduced-motion（动态取色 P1）。`WorkbenchRuntimeStore`/`WorkbenchUIStore` 双 store 分离，边界即 durable/live 的 UI 投影；虚拟化统一 `@tanstack/react-virtual`，删 `react-virtuoso`。

## 主线三：迁移

依据 ADR-0079（工单 #48/#50）：

- **基线封版**：旧验收链（旧六票验收记录 + Runtime/PiRpcAdapter/Web 契约证据）封版为 tag `migration-baseline-20260905`；未完成的旧 P0 发布目标如实标记「由 2.0 重构取代」。
- **上游冻结**：`product/Halo Studio/vendor/`、`halo-scope.json`、`MiniApp/` 冻结只读，根 `BitFun-latest/` 保持未跟踪；卫生检查 `check-repo-hygiene.mjs` 守卫任何未提交改动。冻结期间无上游同步候选、无装配范围变更。
- **最终删除**：新基座首个真实验收版本通过后，`vendor/`、上游来历树与 `halo-scope.json` 作为独立、可审查的最终变更整体删除（不逐项解冻、不渐进拆改）；MIT 归属（ADR-0052）在删除后继续满足。
- **行为等价**：重构期间旧基线契约测试保持可复现，作为行为对照物；新基座以同等契约 + 真实 UI 验收建立自己的证据链。

## 里程碑切分

执行顺序建议 M1 → M6；M4/M5（前端）与 M2/M3（基座）在 M1 完成后可并行。每个里程碑以可测试语句验收；不含代码细节。

### M1 事实与端口

统一事件事实层与 `ManagedExecutorPort`，pi adapter 收敛到端口（ADR-0078/0080；#51/#41/#40）。

验收标准：

1. `ManagedEventFactKind` 词汇执行器中立：pi 事件与 DSH 事件的等价输入经各自规范化路径产生相同 kind 的事实（契约测试断言同一词汇）。
2. 事实日志只含 committed 粒度事实：注入 token 级流式帧后事实日志与证据不变，活动会话记录出现对应条目。
3. 取消中的任务在事实日志留下已交付前缀事实与 `interrupted` 生命周期事实，且不存在「完成」事实；失败尝试记独立 attempt 事实。
4. 所有事实落盘路径都经过 `normalize_summary`；契约测试覆盖闸门一处即可证明脱敏/限长/fail-closed 生效。
5. `ManagedExecutorPort` 在 runtime-ports 定义共同面与能力档案 flag；pi adapter 仅经端口被消费，不存在 pi 专属旁路命令。
6. 迁移基线 tag `migration-baseline-20260905` 对应的既有契约测试在本里程碑结束时仍全部通过。

### M2 dsh-adapter

halo-dsh-adapter 以 acp 为主通道接入（ADR-0078；#38/#39）。

验收标准：

1. `SUPPORTED_DSH_PROFILES` 锚定 0.1.3-alpha.1：`initialize` 探测通过的版本方可启动受管任务，未知/漂移版本 fail-closed 并如实上报原因（契约测试含负例）。
2. DSH `session/requestPermission` 映射到统一 approval 契约（`allowed-once | rejected | cancelled | unavailable` 四值封闭枚举 + `approval/asked|decided` 审计对）；deny、超时、协议错误均 fail-closed，且决议卡与 pi 的统一渲染。
3. 每受管任务一个受控进程：任务取消走「关 stdin→宽限→回收」阶梯，进程无孤儿残留（契约测试断言进程清理）。
4. 凭据零落盘：凭据值仅存在于受控子进程 env；磁盘扫描负面测试找不到凭据内容；`DSH_HOME` 指向 Halo 管理目录；`.env` 未被用作注入通道。
5. DSH acp 事件规范化后落入 M1 的事实词汇；attempt/interrupted 语义与 pi 的差异在事实层如实表达，无虚假对齐。
6. `sdk` 金丝雀降级通道：降级期间事件投影与证据链不断（契约测试覆盖降级映射）。

### M3 pi 档案升级

pi 0.85.0 档案与执行器选择落地（ADR-0078；#42/#40）。

验收标准：

1. pi 0.85.0 档案契约测试全数通过；安装源探测钉定 `@earendil-works`，旧 scope 安装被如实报告为不匹配。
2. `steer` 在等待开发者态可用，并经能力档案 flag 投影：DSH 任务上无 steer 能力时 UI 如实降级，不出现不可用的转向控件。
3. `bash` 命令不消费守卫有契约测试覆盖并生效。
4. 工作区默认执行器 + 任务创建时覆盖生效：绑定记入任务基线并保持到终态；任务创建处选择器只列真实生产 Adapter；会话中无切换入口。

### M4 token 层

DMS MD3 token 架构与主题（ADR-0077；#46/#47）。

验收标准：

1. `tokens/` 目录为纯 CSS custom properties，MD3 角色命名（`--surface-container-*`、`--on-surface`、`--outline`）与三档圆角/五档间距/四档字号 × fontScale 齐备。
2. 禁裸值 lint 生效：颜色/间距/圆角出现非 token 引用的样式文件导致检查失败（CI 断言）。
3. sass 依赖移除，组件样式全部为 CSS Modules。
4. `[data-theme]` 提供 dark/light 两套角色值，默认随 prefers-color-scheme；品牌色作为 `--primary` seed 注入，无第二套主题文件。
5. prefers-reduced-motion 下全部动效停用（自动化断言 + 真实环境抽查）；动效 token duration/easing 各三档。
6. `react-virtuoso` 从依赖中移除，长列表虚拟化统一为 `@tanstack/react-virtual`。

### M5 条带 shell

条带工作台六表面与手势 P0 集（ADR-0076/0077；#44/#45/#47）。

验收标准：

1. 工作台呈现工作区轨 + 任务条带 + Overview；新建任务在焦点右侧插入，既有列位置与尺寸不变（端到端断言列不重排）。
2. 手势 P0 集全部生效且各有等价键盘路径：双指横滚横移条带、滚轮只滚列内、Shift+滚轮横移、←→ 焦点列、`n`/`o`/`1..9`/Esc。
3. Overview 按工作区分组，状态色 + 标题截断呈现，极端列数下分页可用。
4. 双 store 边界成立：`WorkbenchUIStore` 不持有事实/凭据/证据状态，运行事实只来自 Runtime 投影（类型与契约断言）。
5. 两执行器的决议流统一渲染为「Agent 操作请求」卡；运行中如实显示执行器身份与能力降级。
6. 六表面全部可达：命令面板完成任务创建、工作区跳转与执行器选择（仅创建处）；Git 面板保留 ADR-0055–0063 用户驱动语义；设置含模型/凭据、执行器默认与主题。
7. 真实 UI 主链验收：真实双 Adapter 受管任务从创建、决议、交付审查到接受/拒绝全程在新 UI 完成，证据脱敏。

### M6 迁移收尾删除上游

整体删除上游树（ADR-0079；#48/#50）。

验收标准：

1. M1–M5 全部验收通过，且新基座首个真实验收版本（真实双 Adapter + 真实 UI 主链）放行。
2. 上游树删除是独立、可审查的最终变更：`product/Halo Studio/vendor/`、`halo-scope.json`、`MiniApp/` 一并移除，根 `BitFun-latest/` 兜底忽略随之清理。
3. 删除后完整复验：产品构建、Rust/前端契约测试、端到端与真实 UI 验收全绿；迁移基线 tag 保持可检出，历史证据不回改。
4. 仓库不再含 BitFun 上游源码关系，`GCWing/BitFun.git` 同步流程退役；MIT 归属与第三方声明（ADR-0052）测试仍通过。
5. 冻结路径不复存在，卫生检查不再报告冻结路径失败；守卫随最终变更收敛为防回归断言。

## 显式延后（雾区）

以下事项在决策地图中未决议，本规格不做承诺，各自独立立项：

- **MiniApp 去留**：`MiniApp/` 随上游树冻结（ADR-0079），但删除冻结树不预决其去留；mobile-web 类能力是否以 Halo 自有实现延续待议。
- **安装器与更新链**：整体重写后 ADR-0039/0040（签名安装器、用户确认更新）的延续性需按新打包形态重新评估。
- **诊断导出落点**：ADR-0042–0044 的零遥测与脱敏导出语义不变，但导出入口在新 UI（设置表面）中的信息架构与交互落点待定。
- **条带虚拟化性能预算**：长任务列表条带 UI 的性能指标（列数/事实量上限、渲染帧预算）未定，需在 M5 实测后设立。
- **i18n 工程化**：zh-first（ADR-0035）延续，i18next 平移，但抽取机制与 key 管理工程化待独立设计。

## Further Notes

- 本规格与 ADR-0076–0080 一同构成 2.0 重构的完整决策集；若里程碑验收与 ADR 冲突，以 ADR 为准并回到决策地图补票。
- 执行在新票分批进行（plan, don't do）；里程碑切分是建议而非排期，M4/M5 与 M2/M3 的并行由主会话按人力与 git 写槽安排。
- 冻结路径（`product/Halo Studio/vendor/`、`halo-scope.json`、`MiniApp/`）在 M6 前禁止任何改动；卫生检查是唯一守卫，绕过它等于放弃行为等价对照。
