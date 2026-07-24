# Halo Studio

Halo Studio 是面向 **Pi** 与 **OpenCode** 的精简桌面受管工作台。它以 Electron 为桌面外壳，Windows 是 R1 的验收平台；代码结构避免绑定单一平台，但 macOS/Linux 发布不属于当前交付。

当前分支处于 R1 核心重构阶段。这里的目标是让一个受信任工作区中的 Pi 与 OpenCode 具有可核对的检测、启动、停止和状态边界，而不是交付完整 IDE。

## R1 的范围

R1 只处理以下事情：

- 打开一个本地工作区，并显式维护它的信任状态。
- 由 Electron Main 进程拥有 Pi 与 OpenCode 的检测、生命周期和公开状态。
- Pi 使用 JSONL RPC，并在 `get_state` 就绪后才报告可用；其模型、thinking 和 Provider 凭据只在 Main 进程解析。
- OpenCode 使用锁定依赖版本的本地运行时、回环地址、每次启动的新认证信息、健康检查和版本握手。
- 通过受限的 Preload API 提供工作区、运行时、配置事务和存储健康度的固定 IPC 契约。
- 提供紧凑的 VS Code 风格工作台框架：标题栏、活动栏、侧栏、中心区、运行时状态栏、底部面板和状态栏。

当前界面能打开/信任工作区、刷新 Pi 与 OpenCode 状态；在受信任工作区中，Pi 面板可使用固定 IPC 与 Main 已有的受管启动配置来启动 Pi、在启动中或就绪后停止 Pi，并在崩溃或不可用后执行停止再重试。界面不接收、展示或保存模型、thinking、Provider 或凭据输入；这些内容缺失时，Pi 启动会失败关闭。当前界面也能启动 OpenCode。配置事务的底层包已存在，但桌面服务暂不开放写入，相关 IPC 会失败关闭。

以下内容不在 R1 内：完整编辑器或文件修改、完整对话、命令执行、嵌入式终端、配置编辑界面、打包安装器和跨平台发布。不要把工作台布局或占位视图当作这些能力已经交付。

## 架构与安全边界

- Renderer 只负责 React UI 与经过验证的快照，不能访问 Node、凭据、子进程或本地回环服务。
- Preload 仅暴露按业务域划分的固定方法，并校验每次请求和响应。
- Main 进程独占工作区、信任、进程、SQLite、凭据库和配置事务；切换、撤销信任或退出时会尝试停止该工作区的运行时。
- 凭据只保存在 Electron `safeStorage` 保护的凭据库中。公共 IPC、状态、日志和 SQLite 不应包含明文 Provider 值。
- 运行时环境使用白名单构造；不会将宿主进程的完整环境直接传给 Pi 或 OpenCode。

详细设计与当前实现状态见：

- [产品需求](docs/requirements/2026-07-24-halo-studio-pi-opencode-product-requirements.md)
- [架构边界 ADR](docs/adr/0001-pi-opencode-managed-workbench-boundary.md)
- [核心架构](docs/architecture/pi-opencode-core.md)
- [验证指南](docs/testing/core-rebuild-verification.md)

## R2 会话边界（当前受限切片）

在 R1 运行时边界之上，当前版本还提供了受限的结构化会话投影：仅当 Pi 或 OpenCode 已由 Main 进程启动且运行正常时，界面才可以查看有界历史、发送普通提示词或中止当前会话。输入 `/` 时，界面只展示当前受管应用实际公开的原生命令，并把选择结果插入输入框；Halo 不提供独立的命令执行 API。

这不是完整聊天产品、IDE 或终端。它不提供任意 Shell、PTY、嵌入式终端、文件读写/编辑、Diff 定位、模型或 Provider 输入、凭据输入、权限代理或完整会话归档。Pi/OpenCode 自身的文件写入和权限语义仍由各自运行时负责。完整边界见[核心架构](docs/architecture/pi-opencode-core.md)。

## 数据与故障排查

应用运行数据只保存在 Electron 的 `userData` 目录下，路径由宿主系统决定，项目代码不会硬编码本机路径。该目录包含：

- `storage/halo-studio.sqlite3`：Halo 自身的元数据与迁移状态；
- `credentials/`：由 Electron `safeStorage` 保护的凭据引用内容；
- `runtime/pi/` 与 `runtime/opencode/`：两个受管运行时的私有宿主目录。

不要将该目录、`.halo-runtime/` 的本地原生模块缓存或任何凭据复制进仓库。排查时先确认工作区是否受信任，再刷新 Main 进程提供的 Pi/OpenCode 真实状态；运行时无法启动时，可依次执行 `npm run verify`、`node scripts/windows-smoke.mjs` 和[验证指南](docs/testing/core-rebuild-verification.md)中的针对性命令。开发态 Electron 图形烟测必须在交互式、非受限 Windows 宿主运行，不能用 `--no-sandbox` 绕过受限宿主。

## 环境与命令

- Node.js `>= 20.18`
- npm `>= 10.8`

在本工作树根目录安装锁定依赖：

```powershell
npm ci
```

常用检查：

```powershell
npm run check:repository
npm run typecheck
npm test
npm run build
npm run verify
node scripts/windows-smoke.mjs
```

`npm run verify` 依次执行仓库检查、类型检查、测试和构建。`node scripts/windows-smoke.mjs` 仅在 Windows 上运行，并用受控子进程验证 Main 服务组合；它不是已打包桌面应用的人工 GUI 验收。

`npm run dev` 会转发到桌面工作区：它构建 Main/Preload 开发入口，在固定回环地址 `http://127.0.0.1:5173` 启动 Vite Renderer，并启动加载该地址的 Electron。它用于实际桌面开发会话，但不能替代已打包安装器或 Pi/OpenCode 真实环境的验收。

`better-sqlite3` 是原生模块，因此开发链在 Node 与 Electron ABI 间显式切换。桌面工作区的 `dev`、`build` 和 `smoke:dev` 前置步骤会运行 `scripts/prepare-native-runtime.mjs electron`；根测试、桌面测试和 `windows-smoke.mjs` 则在执行前使用 `scripts/prepare-native-runtime.mjs node` 恢复当前 Node ABI。已构建副本只缓存于 Git 忽略的 `.halo-runtime/native-build-cache/`，绝不能暂存或提交。

Windows 上可运行实际开发态烟测：

```powershell
npm run smoke:dev --workspace @halo-studio/desktop
```

该命令以临时用户数据目录启动同一套 Vite + Electron 开发流程，确认 Renderer 已由回环开发服务器提供且 Electron 已加载开发窗口；它不验证已打包产物，也不启动真实的 Pi/OpenCode 服务。正式执行必须使用具有交互式桌面会话、且不受当前 Codex 限制的 Windows 宿主：当前受限环境会阻断 Chromium 的 sandboxed Renderer，不能作为此烟测的有效运行环境。不得追加 `--no-sandbox` 规避该限制；这会破坏桌面窗口既有的安全边界。完整命令说明、测试覆盖与待补验收项见[验证指南](docs/testing/core-rebuild-verification.md)。

## Pi 探测与启动配置（当前内部接缝）

Pi 的版本探测刻意不携带 Provider 凭据：`pi --version` 只接收白名单化的基础运行时环境，探测阶段不会读取凭据库。只有版本探测成功且即将创建确认的 Pi JSONL RPC 子进程时，Main-only 启动解析器才一次性解析模型、thinking、Provider 环境键和受保护的 Provider 值；这些值只在该子进程的创建范围内使用，不会被运行时或服务缓存。

当前临时解析器从 Main 环境读取非敏感选择器 `HALO_PI_MODEL`、`HALO_PI_THINKING`、`HALO_PI_PROVIDER_ENV_KEY`（Provider 环境变量名）和 `HALO_PI_CREDENTIAL_REFERENCE`，再按凭据引用从受保护凭据库取得 Provider 值。不要把 Provider 明文放入环境变量、Renderer、IPC 请求或测试快照。

这是一条内部实现接缝，不是面向最终用户的配置协议；R1 虽已提供固定的 Pi 启动、停止和重试界面操作，但尚未提供保存启动选择器、录入凭据或管理凭据的设置页面。缺少选择器、凭据库不可用或凭据不存在时，Pi 启动必须失败关闭。

## 参考资料与仓库卫生

`用于参考的几个项目的代码/` 是永久只读的参考资料目录。禁止在其中修改文件、安装依赖、执行构建或运行测试；它不得被暂存、提交、发布或作为本项目构建输入。若未来需要复用任何外部代码、图标或资源，必须先单独核对许可证并在本仓库中记录来源。

当前实现使用 `lucide-react` 图标；没有把参考资料目录中的代码、图标或资源纳入本仓库。第三方依赖说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 开发约定

- 活跃重构分支为 `develop`；提交信息使用中文且保持单一职责。
- 需求的唯一活动来源是[当前产品需求](docs/requirements/2026-07-24-halo-studio-pi-opencode-product-requirements.md)。历史计划仅作追溯，不直接生成实现任务。
- 提交前至少运行与改动相称的验证，并执行 `git diff --check`。
