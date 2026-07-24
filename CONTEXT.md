# Halo Studio

Halo Studio 是面向本地开发者的 Pi 与 OpenCode 桌面工作台。它以受信任 Git 工作区中的可验证双 Agent 编码交付为后续产品方向，不把自身定义为完整 IDE 或通用 AI 应用平台。

## Language

**受管应用（Managed Application）**：
Halo Studio 明确支持并负责发现、配置或启动的 Pi 或 OpenCode。_Avoid_: 四 Agent、通用 Agent 注册表、Codex、Claude Code。

**工作区（Workspace）**：
用户显式打开、经真实路径校验并具有信任状态的单一项目上下文。_Avoid_: 任意目录、全局当前路径。

**Git 工作区（Git Workspace）**：
包含已初始化 Git 仓库，并为 Agent 任务提供权威变更基线的工作区。_Avoid_: 普通文件夹、文件系统快照工作区。

**受管运行时（Managed Runtime）**：
代表一个受工作区信任边界约束、由 Main 进程拥有生命周期的 Pi 或 OpenCode 进程/服务。_Avoid_: Renderer 进程、浏览器回退服务、静态在线状态。

**受管 TUI 会话（Managed TUI Session）**：
用户显式开启、绑定当前受信任工作区且仅面向 Pi 或 OpenCode 的交互式原生命令会话。_Avoid_: 任意 Shell、通用 PTY、Renderer 直接启动进程。

**受管启动配置（Managed Launch Configuration）**：
由 Main 进程解析的模型、thinking、Provider 凭据引用及启动选项集合。_Avoid_: Renderer 表单中的明文凭据、硬编码模型默认值、继承完整宿主环境。

## 任务交付

**本地开发者（Local Developer）**：
拥有当前工作区并独自接受或拒绝其交付结果的个人开发者；不依赖团队协作或远程执行。_Avoid_: 团队用户、办公用户。

**任务说明（Task Brief）**：
本地开发者提供的任务目标，以及显式选取的文件、已有 Diff 或简短补充说明。_Avoid_: 隐式完整工作区上下文、自动附带完整历史。

**Agent 任务（Agent Task）**：
一个由本地开发者在单一 Git 工作区中分配给主 Agent 的有限编码请求；显式重试和 Agent 交接为同一任务追加交付证据版本。_Avoid_: 聊天、自由运行 Agent。

**任务基线（Task Baseline）**：
创建 Agent 任务时记录的 Git 状态与变更，用于只归因其后的改动并保留用户既有修改。_Avoid_: 干净工作树要求、自动 stash。

**主 Agent（Primary Agent）**：
当前负责某个 Agent 任务交付证据版本的、由本地开发者显式选择的 Pi 或 OpenCode；交接可在下一版本中更换它。_Avoid_: 自动分配 Agent、默认 Agent。

**验证结果（Validation Outcome）**：
由受管应用原生运行时产生、或由用户显式标为未执行的通过、失败或未执行结果；Halo 在当前阶段不自行执行任意验证命令。_Avoid_: Halo 执行的测试、隐式验证。

**交付证据（Delivery Evidence）**：
将 Agent 任务与其摘要、文件变更和验证结果关联起来，供本地开发者审查和接受的记录。_Avoid_: 完成消息、聊天记录。

**交付证据版本（Delivery Evidence Revision）**：
一次运行、重试或 Agent 交接产生的追加式交付证据；只有最新的可审查交付可得到任务当前结论。_Avoid_: 覆盖旧结果、接受过期 Diff。

**可审查交付（Reviewable Delivery）**：
主 Agent 已结束并生成、可供用户审查或交接的交付证据。_Avoid_: 运行中交接、实时转交。

**已接受交付（Accepted Delivery）**：
本地开发者已接受的最新可审查交付；只记录任务结论，不创建 Git 提交、分支、拉取请求或发布。_Avoid_: 自动提交、自动发布。

**已拒绝交付（Rejected Delivery）**：
本地开发者拒绝但不会改动关联工作区文件或删除任务记录的交付证据。_Avoid_: 自动回滚、丢弃会话。

**原生工作区修改（Native Workspace Modification）**：
主 Agent 依其原生权限模型直接在受信任工作区做出的文件变更；Halo 负责观察和审查而不代理应用。_Avoid_: Halo 受管文件写入、暂存应用。

**交接包（Handoff Package）**：
经用户预览的有限上下文，包含任务目标、主 Agent 摘要、选定文件变更和验证结果；默认排除完整对话、原始工具日志、凭据与配置文件。_Avoid_: 全量上下文转交、自动共享会话记录。

**Agent 交接（Agent Handoff）**：
本地开发者把已审阅的交接包转交给另一受管应用继续或审查同一 Agent 任务的行为。_Avoid_: 自动委派、自动故障转移。

**审查请求（Review Request）**：
以评估可审查交付为目的的 Agent 交接；它不承诺 Halo 强制只读，任何实际文件改动都会形成新的交付证据。_Avoid_: 只读审查模式、不可变审查者。

**交付历史（Delivery History）**：
本地持久化的 Agent 任务状态、交付证据、交接包和接受结论；默认不保存完整对话、原始工具日志或凭据。_Avoid_: 云同步、对话档案。

**持久化交付证据（Persisted Delivery Evidence）**：
交付历史中保存的、经过脱敏和大小限制的 Diff 与结果摘要；完整原始 Diff 只从当前 Git 工作区按需读取，无法安全保留时必须明确标记。_Avoid_: 原始 Diff 档案、静默截断。

## 运行时与界面

**运行时可用性（Runtime Availability）**：
Pi 或 OpenCode 独立报告的健康与兼容状态，只决定该受管应用能否运行任务，不代表另一应用或工作台不可用。_Avoid_: 全局在线状态、模拟就绪状态。

**运行轨迹（Runtime Trace）**：
任务运行中对原生消息、工具状态和操作请求的瞬时界面展示；它不是默认持久化的原始会话记录。_Avoid_: 会话档案、永久原始日志。

**Agent 操作请求（Agent Action Request）**：
暂停 Agent 任务、等待本地开发者通过该受管应用支持的原生通道作出权限或澄清决定的请求。_Avoid_: 通用批准对话框、隐式权限。

**开发工作台（Developer Workbench）**：
围绕工作区、任务证据、受管应用状态和项目变更组织重复编码工作的 IDE 式 Halo 界面。_Avoid_: 聊天优先助手、通用 AI 应用。
