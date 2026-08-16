# 工单 14：真实 Pi RPC 原生 UI 验收记录（脱敏）

记录日期：2026-08-16

记录状态：`not-run`（真实原生 UI 主链未能启动，P0 不放行）

规范来源：`docs/requirements/bitfun-tauri-product-migration/issues/14-complete-real-pi-rpc-native-ui-acceptance.md`

本记录只保存公开版本、兼容性档案、退出码、状态枚举、Git 摘要和清理结论。它不保存密钥、完整凭据或 Base URL、Authorization、完整对话、Pi session/entry/toolCall 标识、原始 JSONL、完整命令输出或含敏感值的截图。

## 验收前授权与边界

- 用户已明确授权：可删除独立验收工作区、使用真实 `credential_ref`、选择 Provider/模型，以及执行本次 Halo/Pi 受控流程所需的工作区外系统凭据和临时目录读写。
- 已确认本机安装的 Pi 公开版本和第一方 extension 审计清单。授权不等于已读取凭据：本记录阶段未读取凭据、未启动 Pi RPC、未发送真实模型请求。
- 验收工作区标签：`<approved-worktrees>/i14`。该标签代表可删除的独立 Git clone，不记录本机用户名或绝对路径；初始 `git status --short` 改动数为 `0`。
- 工单 14 固定审查 base：`69b59009006a06036d2b99d1ebf17ae3a2c8d932`（工单 13 收口分支）。验收 clone 初始提交：`9592e622f319cbc16568d1c82a7eebd64dddc840`。
- 上游 UI 参考（流程补充）：根目录只读克隆 `BitFun-latest` 已快进至公开提交 `bcd622a287c03da9383d97d9491cf809ec2fc468`（2026-08-16）。它仅用于参考最新 UI 布局，不是构建输入、验收工作区或放行证据；未复制其配置或凭据。

## 公开 Pi 兼容性档案

以下是工单 07/03B 定义的唯一 P0 档案。档案字段来自仓库内公开规范；由于正式 Tauri 入口未能启动，所有需要现场 RPC/UI 观察的能力均保持 `not-run`，不能以替身、HTTP smoke 或静态契约测试代替。

| 档案字段 | 现场状态 | 脱敏事实 |
| --- | --- | --- |
| P0 identity | `not-run` | `pi-rpc-p0` 是唯一生产身份；未在原生 UI 中显示或核验 |
| `pi --mode rpc` 启动 | `not-run` | Halo 未进入受控启动阶段 |
| 严格 LF JSONL framing、response ID 关联 | `not-run` | 未观察真实 stdin/stdout RPC 流 |
| Commands | `not-run` | `prompt`、`follow_up`、`abort`、`get_state`、`get_entries` 未在真实会话中执行 |
| Events | `not-run` | `message_update`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`agent_settled` 未在真实会话中观察 |
| Extension UI | `not-run` | `extension_ui_request`/`extension_ui_response` 未在真实会话中观察 |
| Provider/model/session/agent ownership | `not-run` | 规范归属为 Pi；Halo 应只投影非敏感就绪事实并持有信任、任务、权限、证据和生命周期权威，现场未核验 |

### 第一方 extension 审计基线

| 项目 | 状态 | 公开记录 |
| --- | --- | --- |
| 审计 CLI | `pass` | `eligible`；1 项清单、0 个 blocking reason、6 项明确 `blocking:false` 排除记录 |
| Extension ID | `pass`（静态审计） | `halo-workbench-permission-gate` |
| 固定版本 | `pass`（静态审计） | `1.0.0` |
| Git source commit/tree/blob | `pass`（静态审计） | `e8c445d6a81d90851ac03d6aac7a4f11b6b749a3` / `f50918b6bdebc6067f409f248cc9182ff5bcdec3` / `15d6908cc30e45f8812a87c591e58799d2f7ae69` |
| SHA-256 | `pass`（静态审计） | `a6f704110e56be3c1c0754dadde1be2b27f65c76ee03f2c19a1e43cd06848c0b` |
| 加载边界 | `pass`（静态审计） | adapter-owned 临时副本，`--no-extensions --extension <adapter-owned-temp>`；拒绝项目/用户自动发现和运行时下载 |
| 能力/权限边界 | `pass`（静态审计） | 仅在 `tool_call` 前请求一次性 UI 决议；extension 源码无直接文件、网络、进程、Git、凭据或 Renderer API，但继承 Pi 子进程宿主权限且不是沙箱 |
| 原生 UI 中的真实 gate | `not-run` | 未触发真实 `tool_call`，不能把静态审计当作 allow/deny、超时或 fail-closed 的现场证据 |

审计清单来源：`docs/architecture/pi-first-party-extension-inventory.json`；只读证据定位符：`readonly-evidence://bitfun-latest`（不作为构建输入）。

## 非敏感 preflight

| 项目 | 状态 | 脱敏事实 |
| --- | --- | --- |
| `where.exe pi` | `fail` | 退出码 `1`；未保存路径或命令回显 |
| `Get-Command pi -All` | `pass` | 发现 3 个 `Application`/`ExternalScript` 候选；未保存路径 |
| `pi --version` | `pass` | 公开版本 `0.83.0`，退出码 `0` |
| `check:repo-hygiene` | `pass` | 退出码 `0` |
| `product:check` | `pass` | 退出码 `0` |
| `product:test` | `pass` | 退出码 `0` |
| `type-check:web` | `pass` | 退出码 `0` |
| `desktop:build:fast` | `pass` | 退出码 `0` |
| `e2e:test:smoke`（规定默认端口） | `fail` | 启动器已指向 Halo 二进制和正式 startup spec；受限宿主无法绑定 WebDriver 默认端口，并报告 WebView2 资源占用及用户状态目录写入阻断；改用高位端口重试仍失败 |
| `git diff --check` | `pass` | 退出码 `0` |

为使 smoke 命令与正式 Halo 产物一致，提交 `9592e622…` 只更正 E2E 启动器的 `halo-studio` 二进制名、Tauri 构建包名和 startup spec；它不替代真实 UI 验收。

## 真实原生 UI 清单

正式入口 `pnpm --dir "product/Halo Studio" run desktop:dev` 在受限宿主的权限审批阶段被平台服务拒绝（服务不可用），因此命令未执行、Halo 窗口未启动，也没有由 Halo 启动 Pi RPC。以下每项均明确为 `not-run`：

| # | 状态 | 脱敏观察边界 |
| ---: | --- | --- |
| 1 | `not-run` | 正式 Tauri 入口、独立工作区打开/信任及已对齐 BitFun 工作台身份 |
| 2 | `not-run` | UI 中的 Pi 版本、`pi-rpc-p0` RPC 能力、Provider/模型就绪和失败关闭 |
| 3 | `not-run` | 首轮 prompt/reply、`agent_settled` 后进入 `waiting_developer` |
| 4 | `not-run` | 第一方 extension `tool_call` 执行前阻断、allow/deny、超时、错误和匹配 response |
| 5 | `not-run` | 同一 RPC session 的 follow-up 及一个可丢弃的无害工作区改动 |
| 6 | `not-run` | 结束并审查、只读 Diff/摘要/归因/验证/证据新鲜度及接受/拒绝 |
| 7 | `not-run` | 接受/拒绝不触发 Git 暂存、提交、推送、回滚、删除、建分支或改写历史 |
| 8 | `not-run` | 运行中关闭/重启后的 `interrupted`、无自动重连/重发/重放/重复写入 |
| 9 | `not-run` | 退出后的 Pi 子进程、RPC 句柄、临时认证材料和受管 session/config 清理 |

截图/录屏：`not-run`（没有启动原生窗口，未产生可保存的现场画面）。自动化 smoke、受控替身和静态 contract test 只作为前置检查，不能改变上述状态。

## Git、进程与清理边界

- 根工作树保持只读：HEAD `7aaa42105d429e7ba53a2db871abdfdd15bd0be8`，未提交改动计数仍为 `99`。
- 独立验收 clone 初始干净；在 UI 未启动前没有 Halo 自动暂存、提交、推送、回滚、删除、建分支或改写历史。
- 本记录阶段没有启动 Halo/Pi 进程，因此没有可报告的 Pi PID、RPC 句柄或运行时 session；未读取凭据，也没有临时认证材料产生。
- 另一个较长路径的失败 clone checkout 未能由平台审批服务清理（服务不可用），仍是外部残留阻断；该目录未用于验收，未运行 Pi/RPC 进程。不得把它解释为清理通过。
- 证据记录本身只保留脱敏摘要；不保存原始工作区内容、原始 JSONL、完整进程命令行或敏感路径。

## 结论

工单 14 当前结论为 `not-run`，P0 `blocked`。必须在交互式、非受限 Windows 宿主中按规范第 1–9 步完成真实 Halo 原生 Tauri UI 主链，并补充脱敏截图、Git 前后事实和进程清理结论后，才能重新评估放行。真实 Pi/模型请求在本记录阶段没有发生；任何后续真实请求仍须在授权范围内、仅由 Halo 受控启动。
