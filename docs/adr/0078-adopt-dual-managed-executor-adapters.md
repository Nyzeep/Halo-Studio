---
status: accepted
date: 2026-09-05
supersedes: 0072 pi RPC 作为 P0 唯一受管执行 Adapter
amends: 0023 本地已安装执行器
related: 0065 Tauri seam 深 Module; 0012 一次性决议; 0075/0080 事件事实与脱敏闸门; 0024 执行器配置分层; 0008 系统凭据引用
decision-map: Nyzeep/Halo-Studio#32（工单 #38 协议研究、#39 dsh 接入形态、#40 执行器选择与交接、#41 P0/P1 提取边界、#42 pi 版本档案、#49 ADR supersede 波次）
---

# 采用双受管执行 Adapter 与统一执行器端口

## ADR 关系

- Supersede(s): ADR-0072。Pi RPC 不再是 P0 唯一生产受管 Adapter；其协议结论（LF-only JSONL framing、`agent_settled` 可靠终态、第一方 extension fail-closed）继续约束 halo-pi-rpc-adapter。
- 修订 ADR-0023：Pi 继续作为用户本机已安装、不下载打包不代升级的执行器；OpenCode 部分随之失效，不进入 2.0 受管路径（ADR-0074 的移除延后判断被本次基座决议吸收，OpenCode 不复活）。
- ADR-0065 保留：Halo Workbench Runtime 仍是 Tauri seam 的 Rust 单一权威；halo-dsh-adapter 与 halo-pi-rpc-adapter 在 Adapter seam 同级并列。ADR-0012 一次性决议、ADR-0008 凭据引用、ADR-0024 配置分层语义不变。

## 决策

（决策地图 #32）生产受管执行 Adapter 恰为两个同级实现，统一收敛到 `ManagedExecutorPort`：

- **双受管 Adapter**：halo-pi-rpc-adapter 与新增 halo-dsh-adapter 同级，均为生产通道；两者都不把执行器原生会话、凭据或原始日志提升为 Halo Interface，事实与证据权威仍在 Runtime（ADR-0075/0080）。CONTEXT.md「主执行器」「受管执行器」词条已按此改写。
- **DSH 主通道 = acp profile**（#38/#39）：`session/requestPermission`（allow-once/reject-once、不推断持久放行）与 ADR-0012 一次性决议直接同构——这是受管交付的硬需求；`sdk` profile 的 wire 面没有审批通道，结构上无法承载受管交付，降为协议金丝雀/降级通道（降级通道的事件面映射到统一事实词汇，证据不断链）。`session/resume` 不消费：Halo 中断语义不自动恢复。`sdk-minimal` 硬编码 full-access 且审批 fail-closed `unavailable`，不作生产通道；进程内嵌 Cordis 不采用（#41：DSH agent loop 不内嵌为 Runtime 第二执行引擎）。
- **每受管任务一个受控进程**（#39）：与 pi adapter 同构——取消统一为「关 stdin→宽限→回收」阶梯（DSH acp 的 `session/cancel` 与 per-session close 用于进程内子会话）；凭据进程级隔离；故障爆炸半径最小。共享进程池为 P1，视运行数据再议。
- **版本档案机制**（#39/#42）：与 pi 档案同构。`SUPPORTED_DSH_PROFILES` 锚定 0.1.3-alpha.1 + `initialize` 就绪/能力探测 + fail-closed；上游 minor 升级必须建立新档案并全数通过契约测试后才放行（DSH developer preview 高频漂移的防线）。pi 侧同步执行 0.85.0 档案验收、`@earendil-works` 安装源钉版（npm scope 迁移）与 `bash` 命令不消费守卫的固化（#42）。
- **凭据注入零落盘**（#39）：CredentialRef（env 变量名）进入 patch/初始化参数，凭据值只经受控子进程 env 注入；`DSH_HOME` 指向 Halo 管理目录实现配置与状态隔离；`.env` 不作为注入通道（bootstrap 名 `DSH_*`/`XDG_*` 禁止进入任何 `.env`）。对齐 ADR-0008/0025。
- **统一 `ManagedExecutorPort`**（#40）：runtime-ports 定义两 Adapter 的共同面——prompt/follow_up/abort/get_entries/决议流/事件投影；每个 Adapter 附带能力档案 flag（如 pi 有 steer、DSH 无），UI 按档案如实降级，不虚构能力。统一 approval 契约（#41）：封闭枚举 `allowed-once | rejected | cancelled | unavailable`、fail-closed、`approval/asked|decided` 审计对、callId 防漂移；不放宽 ADR-0012。sandbox 契约仅契约层（模式枚举 + `SandboxEnforcement: full | partial` 如实上报），不引入 DSH 执行后端，文件写入租约维持 Halo 自有（ADR-0004/0005）。
- **执行器选择与交接**（#40）：工作区默认执行器 + 任务创建时可显式覆盖；绑定记入任务基线并保持到任务终态。P0 无会话中切换；「执行器交接」是任务级动作（经任务说明与交付历史关联），不是会话中重绑定。UI 三则：选择器只出现在任务创建处且选项仅为真实生产 Adapter；运行中如实显示执行器身份与能力降级；两执行器决议流统一渲染为同一「Agent 操作请求」卡片。
- **能力提取边界**（#41/#42）：P0 取事件域三分词汇、approval 契约、sandbox 契约（仅契约层）与 pi `steer`/`agent_settled`/`queue_update`；skills/workflows/goals、运行中模型/思考级切换、fork 归因链、compaction、session_stats/export（须过脱敏）、图片附件为 P1。

## 后果

- 执行基座从「单 Adapter + 预留扩展点」变为「双 Adapter + 统一端口」；`ManagedExecutorPort` 成为事实模型（#51/ADR-0080）与前端 Runtime 投影（ADR-0077 双 store）的唯一执行器输入面。
- 新增执行器必须走「版本档案 + 契约测试 + 真实验收」全流程；未建档案的执行器版本 fail-closed，不静默放行。
- UI 不再因单执行器而隐藏选择面，也不伪造多执行器能力：一切能力差异如实进能力档案 flag 并投影到界面。
