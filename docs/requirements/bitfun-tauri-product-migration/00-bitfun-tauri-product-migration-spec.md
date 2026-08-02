# BitFun/Tauri 产品基座迁移规格

**Status:** ready-for-agent

## Problem Statement

Halo Studio 已把受跟踪的 BitFun 下游源码、Halo Tauri 入口和正式 BitFun Web UI 纳入产品树；工单 03/03A1 的构建与原生 UI 证据证明界面基座已经对齐，但当前桌面运行链尚未通过新的 Halo Workbench Runtime 接入本机 Pi Agent。旧 PySide6/QML、Rust Sidecar 和 OpenCode 方案只形成可迁移能力或历史决策基线，不能替代目标 Tauri 产品上的 Pi RPC 运行时迁移和真实 UI 验收。

项目需要在保留既有行为证据的同时，将 Halo Studio 建立为持续获取 BitFun 上游更新、但独立提交和发布的下游产品。迁移必须避免长期双运行时、外部目录构建依赖、旧 IPC 兼容层和过早删除旧实现。

## Solution

Halo Studio 在受跟踪的 Halo 产品树中纳入完整 BitFun 下游源码，建立 Halo 品牌的 Tauri 桌面入口，并只装配本地桌面编码主链。BitFun 上游只提供显式同步候选；所有 Halo 改动、验证和发布都发生在 Halo 自己的仓库中。

新的 Halo Workbench Runtime Module 位于 Tauri seam，成为工作区信任、标准会话、受管任务、配置、凭据引用、Git 状态、权限决议、脱敏、证据和结构化事件的唯一 Halo 权威。P0 在该 Module 内只提供 Pi RPC Adapter：受控启动本机已安装的 `pi --mode rpc`，通过 stdin/stdout 上严格 LF JSONL 复用 Pi 的 Provider、模型、Session 和 Agent 工具循环。旧 Sidecar 的用户可观察语义迁入该 Module，但旧 `stdio JSONL v1`、OpenCode HTTP/SSE、Pi TUI、Unix/CBOR PiServer、Pi 内部源码和原始远程标识都不作为兼容目标。

Pi 负责 Provider、模型、原生 Session 和 Agent 工具循环；Halo 负责工作区信任、系统凭据引用、受管任务状态、一次性决议、脱敏、交付证据和生命周期。前端只依赖 Halo Workbench Runtime 的小 Interface，不直接访问 Pi 子进程、凭据或原始 JSONL。

迁移采用扩展再收缩：旧实现作为可迁移能力基线保留，直到新 Tauri 产品逐项达到行为等价、通过一次上游同步演练、完成 Pi extension 审计并完成真实 Pi RPC UI 验收；随后再以独立变更删除旧产品入口和源码。历史 OpenCode 文档只保留比较结论和迁移记录。

## User Stories

1. 作为本地开发者，我希望启动 Halo Studio 时直接进入 Halo 品牌的 Tauri 工作台，以便使用真正的目标产品而不是旧 QML 界面。
2. 作为本地开发者，我希望保留 BitFun 成熟的三栏工作台和高密度交互，以便迁移后仍拥有完整的编码工作流。
3. 作为本地开发者，我希望只看到 Halo 首期本地编码主链，以便办公、Mini App、远程、Relay 和移动端能力不会干扰产品。
4. 作为本地开发者，我希望标准编码模式通过 Pi 的原生会话与工具能力运行，以便日常编码和 P0 受管交付复用同一条可靠执行链，同时保持两种模式的状态与保留策略隔离。
5. 作为本地开发者，我希望显式进入受管交付模式，以便标准会话不会被错误归为可审查交付。
6. 作为本地开发者，我希望打开并确认一个 Git 工作区后再使用受管能力，以便任务、文件和证据边界清晰。
7. 作为本地开发者，我希望受管任务记录创建时的工作区基线，以便区分已有改动和任务期间改动。
8. 作为本地开发者，我希望 P0 受管任务明确使用已通过真实探测和 RPC 能力检查的本机 Pi，以便获得一条可靠执行链而不是一个尚未实现的多执行器选择器。
9. 作为本地开发者，我希望在系统凭据存储中管理凭据并只向界面暴露引用，以便密钥不会进入产品状态、日志或证据。
10. 作为本地开发者，我希望检查 Pi 可执行文件、版本和 RPC 能力并看到真实失败原因，以便不会把未就绪运行时误报为可用。
11. 作为本地开发者，我希望通过真实 Pi RPC 发送首轮任务消息，以便建立受管任务会话而不是模拟流程。
12. 作为本地开发者，我希望看到经脱敏和限长的用户消息、Agent 回复与结构化运行轨迹，以便理解过程但不暴露原始日志。
13. 作为本地开发者，我希望首轮回复后任务进入等待开发者状态，以便我可以决定追问或结束，而不是被自动标为完成。
14. 作为本地开发者，我希望权限请求只能本次允许或拒绝，以便不会建立永久或跨任务授权。
15. 作为本地开发者，我希望澄清请求只能回答或拒绝，以便决定与原生请求精确对应。
16. 作为本地开发者，我希望操作请求只有在原生执行器确认后消失，以便界面不会伪造决议送达。
17. 作为本地开发者，我希望在同一受管任务会话中发送追问，以便保持任务连续性而不创建第二条会话。
18. 作为本地开发者，我希望追问在任务运行中不可重复提交，以便避免并发回合和消息重放。
19. 作为本地开发者，我希望执行器可以直接在验收工作区产生无害改动，以便验证真实原生工作流。
20. 作为本地开发者，我希望显式结束受管会话后才固定证据，以便等待状态不会提前进入交付审查。
21. 作为本地开发者，我希望在只读审查中核对 Diff、摘要、归因和验证结论，以便作出人工接受或拒绝决定。
22. 作为本地开发者，我希望接受或拒绝交付不会自动提交、推送、回滚或删除文件，以便 Git 控制权始终属于我。
23. 作为本地开发者，我希望应用或运行时意外退出后任务显示为中断，以便不会把异常退出伪装为完成。
24. 作为本地开发者，我希望重启后不会自动重连、重发消息、重放操作请求或重复文件写入，以便中断恢复保持安全。
25. 作为本地开发者，我希望旧六票中的可迁移能力逐项在 Tauri 产品中得到行为等价证明，以便迁移不会静默丢失功能。
26. 作为维护者，我希望 Halo 仓库跟踪完整 BitFun 源码关系，以便持续同步时减少遗漏和重复冲突。
27. 作为维护者，我希望 BitFun 上游远端只用于获取更新，以便 Halo 提交不会意外推送到上游。
28. 作为维护者，我希望每次上游同步都记录精确来源 commit，以便变更、许可证和安全修复可审计。
29. 作为维护者，我希望上游更新通过独立候选和完整验证门槛，以便未经验证的上游变化不会直接进入 Halo 产品。
30. 作为维护者，我希望范围外 BitFun 模块不进入构建、路由、导航和初始化，以便保留源码关系而不扩大产品范围。
31. 作为维护者，我希望保留 BitFun MIT 许可证和第三方归属，以便源码与发行包满足许可证义务。
32. 作为维护者，我希望 Halo Workbench Runtime Module 提供小而稳定的前端接口，以便底层适配器和策略变化不会扩散到整个 UI。
33. 作为维护者，我希望迁移期旧实现仍可运行其自动化基线，以便新旧行为可以逐项比较。
34. 作为维护者，我希望迁移完成前不删除旧源码，以便任何缺失行为都有可执行的参照。
35. 作为维护者，我希望迁移验收后通过独立收缩变更删除旧 QML、旧 Sidecar 和旧入口，以便最终仓库只有一个正式产品路径。
36. 作为发布负责人，我希望真实 Pi RPC 原生 UI 验收只记录脱敏结论，以便发布证据不包含密钥、完整对话、凭据、命令输出或原始标识。
37. 作为发布负责人，我希望删除旧源码后重新执行构建、自动化、同步演练和桌面验收，以便最终产品状态而非迁移中间态获得放行。
38. 作为维护者，我希望通过 Pi RPC 和公开 extension 边界复用 Pi 的 Provider、模型连接和 Agent 循环，以便 Halo 不复制 Pi 内部实现或承担重复维护。
39. 作为维护者，我希望 `D:\pi-main` 只作为只读协议研究快照，以便正式产品不依赖没有可审计 Git 来源的本机目录。

## Implementation Decisions

- Halo Studio 是 BitFun 的受控下游产品：上游只拉取，Halo 的提交、远端和发布完全独立。
- 完整 BitFun 源码关系进入受跟踪的 Halo 产品树；外部上游参考树只用于获取和检查变更，正式构建不依赖外部绝对路径。
- Halo 产品树是长期产品源码边界，不在迁移完成后再次搬回仓库根目录。
- 范围外 BitFun 模块可以保留源码，但不得进入 Halo 首期构建、路由、导航、后台初始化、配置入口或隐藏开关。
- Halo 保留 BitFun 工作台交互骨架，并用 Halo 名称、图标、视觉令牌、简体中文文案和产品范围重新表达。
- Halo Workbench Runtime Module 位于 Tauri seam，并拥有工作区、会话、受管任务、工具、Git、配置和结构化事件的唯一权威状态。
- 前端只依赖该 Module 的小型公开接口，不分别直连大量底层命令，也不调用旧 Halo Sidecar。
- P0 唯一生产受管执行 Adapter 是本机已安装的 Pi RPC；历史 OpenCode Server、BitFun 内置 Code Agent 和多执行器交接不进入当前 UI 选择器或发布矩阵。
- P0 固定链路为 `Halo Workbench Runtime → 受控 Pi 子进程 → pi --mode rpc → stdin/stdout JSONL`。Pi 的 CLI、TUI、Unix/CBOR PiServer 和任意 HTTP/SSE 传输不是 Windows P0 目标。
- Pi 负责 Provider、模型、Session 和 Agent 工具循环；Halo 不复制 Pi Provider/Core 源码，不让前端直连 Pi，也不持久化原始 session ID、entry ID、完整消息、工具结果、凭据、Authorization、命令输出或原始 JSONL。
- Pi RPC 兼容性档案至少验证可执行文件探测、`pi --mode rpc` 启动、LF-only framing、`prompt`、`follow_up`、`abort`、`get_state`、`get_entries`、`message_update`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`agent_settled`、`extension_ui_request` 和 `extension_ui_response`；具体原生载荷只存在 Adapter 内。
- `get_entries` readiness 必须验证 `entries` 数组和可空 `leafId`；存在 leaf cursor 时必须用 `since` 做增量核对。Pi 启动边界必须记录 `--provider`、`--model`、`--no-session` 和隔离的 `--session-dir`；这些参数不能把 Pi session、entry、Provider 或凭据对象提升为 Halo 公共 Interface。
- Pi 默认没有 Halo 权限弹窗且不提供沙箱。工单 09 定义 Halo 第一方 extension：用 `tool_call` 在工具执行前拦截，以 `extension_ui_request/response` 取得一次性 allow/deny；deny、超时、协议错误和 extension 错误均 fail closed。
- P0 extension 只通过显式固定路径加载；运行时使用 `--no-extensions` 禁止发现式加载，并仅允许版本、来源、权限、依赖和许可证已审计的 Halo 第一方 extension。项目本地、用户全局、Pi package 和任意 Provider extension 不进入受管路径。
- 旧 Sidecar 中可复用的纯领域逻辑和测试夹具可以迁移，进程边界、JSONL envelope 和 Python/QML 适配层不保留兼容性。
- 迁移保持用户可观察行为等价，不要求内部模块、文件布局或传输协议等价。
- 标准编码模式和受管交付模式共享 Halo Workbench Runtime Interface 与安全配置权威源，并通过隔离的 Pi 配置/session 目录执行；受管任务还必须经过显式接入、工作区信任和受管策略。
- 凭据明文只在系统凭据存储读取与执行器启动时短暂存在；前端、事件、日志、证据和持久化只处理凭据引用。
- 上游同步必须形成独立、可审查的候选，记录来源 commit，并通过产品裁剪、运行时契约、自动化和真实 UI 门槛。
- 迁移采用扩展再收缩；旧产品源码删除是最后的独立变更，受全部行为迁移和验收票据阻断。
- 旧六票被记录为可迁移能力基线，不重新定义为最终产品验收；真实 UI 验收只在目标 Tauri 产品上完成。
- GitHub #9–#14 保持历史需求、状态与验收记录不变；Pi RPC P0 决策只通过本规格、ADR-0072、迁移工单 03B 和工单 04–15 建立前向映射。

## Testing Decisions

- 主要程序化测试 seam 是 Halo Workbench Runtime Module 的公开 Tauri command/event Interface；测试用户可观察结果，不锁定私有 Rust 函数或 React 组件状态。
- Rust 契约测试覆盖工作区快照、标准会话、受管任务、一次性决议、证据、凭据边界、Git 操作和中断语义。
- Pi RPC Adapter 契约测试覆盖可执行文件解析、受控子进程、LF-only JSONL、命令 response 关联、Provider/model 投影、Session/Prompt/follow-up、事件顺序、`agent_settled`、abort/EOF/清理和敏感字段剥离；受控替身只用于自动化，不构成生产回退。
- 第一方 Pi extension 契约测试覆盖 `tool_call` 执行前阻断、task-scoped 脱敏 `toolCallId`、`extension_ui_request/response` ID 匹配、allow/deny、超时、协议错误、extension 错误和 fail-closed；测试不得依赖 Pi 默认权限弹窗。
- Tauri 桌面烟测和少量端到端测试覆盖真实 Halo 启动入口、三栏工作台、产品裁剪、品牌、关键导航和运行时连接。
- 行为等价矩阵把旧六票的自动化证据映射到新接口与桌面路径；旧 JSONL 封包不作为兼容断言。
- 上游同步测试至少演练一次新的 BitFun commit 候选，证明来源记录、冲突处理、产品裁剪和回归门槛可执行。
- 许可证测试验证 BitFun MIT 归属、来源记录和第三方声明进入源码与发行包。
- 真实发布验收使用本机 Pi、系统凭据存储、可删除 Git 工作区和 Halo 原生 Tauri UI；证据必须脱敏，不启动 Pi TUI，不使用 HTTP/SSE，不保存完整 session。
- 删除旧源码后重跑完整产品构建、Rust/前端契约、桌面端到端、同步演练和真实 UI 验收。

## Out of Scope

- 向 BitFun 上游仓库直接提交、推送或发布 Halo 改动。
- 自动无审查地合并 BitFun 上游更新。
- 保持旧 Python/QML UI、旧 Sidecar 进程或 `stdio JSONL v1` 的向后兼容。
- 在首期启用办公协作、Mini App、远程、Relay、移动端或其他范围外 BitFun 产品模块。
- 在迁移完成前删除旧产品源码或伪造真实 Pi RPC UI 验收。
- 自动提交、推送、改写 Git 历史或替用户完成交付接受决定。
- 历史 OpenCode Server、`opencode serve`、HTTP/SSE 或 OpenCode 2.x 兼容性放行。
- 复制、vendor 或分叉 `D:\pi-main` 的 Pi Provider/Core/Session/Agent 实现，或把该目录作为构建依赖。
- 在 P0 中实现 Pi TUI、Unix/CBOR PiServer、BitFun 内置 Code Agent 或多执行器选择/交接。

## Further Notes

- 首次导入前必须从 `GCWing/BitFun.git` 选择并记录精确上游 commit；本地参考树当前没有 Git 元数据，不能单独作为来源证明。
- `D:\pi-main` 没有 Git 元数据，只能用于协议和行为研究；生产兼容结论必须来自本机 `where.exe pi`、`pi --version`、受控 `pi --mode rpc` probe 和不发送模型请求的协议测试。当前安装版 `pi --version` 结果不自动放宽未来版本。
- 现有根目录工作树包含多组未归类改动。能力基线提交、迁移文档提交、上游源码导入和临时产物清理必须保持独立。
- 迁移完成的必要条件包括：Tauri 构建与打包、标准工作台、唯一 Halo Workbench Runtime、Pi RPC Adapter、第一方 extension 审计、行为等价矩阵、上游同步演练、许可证审计、真实 Pi RPC UI 验收，以及删除旧源码后的完整复验。
