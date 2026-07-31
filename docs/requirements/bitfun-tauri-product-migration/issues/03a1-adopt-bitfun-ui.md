# 03A1 - 接入 BitFun 正式 Web UI 并完成 Halo 品牌适配

**What to build:** Halo Tauri 桌面入口必须实际加载已导入并受跟踪的 BitFun `src/web-ui`，直接复用 BitFun 的工作台布局、组件体系、视觉令牌和交互密度，并在不改变其核心 UI 结构的前提下完成 Halo 品牌、本地编码范围和中文文案适配。不得继续使用手写静态页面作为生产 UI。

**Parent:** 03 - 启动 Halo 品牌的 Tauri 开发工作台.
**Blocked by:** 02 - 固定并纳入可审计的 BitFun 上游基线.
**Blocks:** 03 的最终完成与 04 - 建立 Halo Workbench Runtime 公共契约.

**Status:** ready-for-review

## 决策边界

- BitFun UI 的唯一生产源码来源是 Halo 产品树中的 `product/Halo Studio/src/web-ui`；`D:\BitFun-main` 只用于只读对照，不得成为构建、开发或测试依赖。
- Tauri 桌面壳可以复用 `product/Halo Studio/src/apps/desktop`，但正式前端必须来自 `src/web-ui` 的真实构建入口和组件，而不是 `src/halo-workbench` 的手写 HTML/CSS/JavaScript 壳。
- “直接照搬”指保留 BitFun 的主要布局、组件层次、主题体系、字体、图标、面板行为和交互密度；Halo 只替换产品名称、Logo、必要的品牌色、中文文案和已批准的产品范围裁剪。
- 范围外的办公协作、Mini App、远程工作区、Relay、移动端和其他 BitFun 产品模块可以保留源码，但不得进入 Halo 首屏、导航、路由、后台初始化或构建产物。
- 旧 PySide/QML、旧 Python 和旧 Sidecar 在工单 15 前保持不动。本票只处理新 Tauri 产品树中的 UI 接入，不提前删除旧产品。

## 交付要求

- [x] 记录 BitFun UI 实际入口、构建命令、主要组件目录和 Halo 适配边界；证明构建使用当前受跟踪快照，而不是 `D:\BitFun-main`。
- [x] 将 Tauri 的 `frontendDist`、开发服务器和正式构建链路切换到真实 BitFun `src/web-ui` 构建产物；生产构建不得依赖 `src/halo-workbench`。
- [x] 保留并实际呈现 BitFun 工作台的核心桌面结构：主导航、工作区切换、编码会话、文件树、编辑器、终端、版本控制入口和必要的状态面板。
- [x] 保留 BitFun 的主题、排版、间距、面板层级、图标语义、交互状态和响应式行为；不得以几块自制面板、静态文件名或占位命令预览替代真实组件。
- [x] 完成 Halo 品牌适配：产品名、图标、启动标题、核心简体中文文案和范围外能力裁剪一致，且不破坏 BitFun 的 UI 视觉语言。
- [x] 通过源码和构建扫描确认正式入口没有旧 PySide/QML、外部 `D:\BitFun-main` 绝对路径、旧 Halo Web 页面或 `src/halo-workbench` 生产入口引用。
- [x] 保留并核对 BitFun MIT 许可证、第三方声明以及被复用 UI 资源的归属信息。
- [x] 为桌面宽窗口和窄桌面窗口各留下真实原生 Tauri 截图；截图应显示实际 BitFun UI，不得使用 HTTP 页面或静态 mock 作为原生验收证据。
- [x] 对关键用户可观察流程完成原生 UI smoke：启动、首屏加载、工作区入口、主导航切换、编码会话入口、文件树/编辑器、终端和版本控制入口均可见且可交互。运行时真实连接和受管任务行为仍由 04 及后续工单负责。

## 验收命令与证据

在 VS x64 Rust 工具链环境中，从当前工单 worktree 执行仓库已有的前端、scope 和桌面命令，并记录完整退出码：

```text
node scripts/halo-scope.mjs
node --test scripts/halo-scope.test.mjs
pnpm run build
pnpm run desktop:build
git diff --check
```

如果仓库现有 BitFun Web UI 测试命令与上述入口不同，必须使用实际 `package.json` 脚本并在证据中说明映射关系，不得为了让命令通过而新增只验证 mock 页面存在的测试。

证据至少包括：

- 前端入口和产物路径审计；
- BitFun UI 组件、主题、图标和字体来源清单；
- 范围排除扫描结果；
- desktop build 的退出码和生成物；
- PID 绑定的真实 `halo-studio.exe` 原生窗口截图与交互摘要；
- 许可证和第三方归属核对；
- `git diff --check` 与最终改动范围。

## 执行证据（2026-07-30）

工作树：`D:\Halo Studio\.worktrees\issue-03a1-bitfun-ui`。

执行环境：VS x64 Build Tools，`D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat -arch=x64 -host_arch=x64`。普通 PowerShell 中 `cl.exe` 不在 PATH；VS 环境中 `where cl` 首项为 `D:\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\cl.exe`。本轮未安装软件，未修改系统 PATH、注册表、全局 Cargo 配置或系统环境变量。

### 实际入口和构建产物

- Tauri 正式入口：`product/Halo Studio/src/apps/halo-desktop/tauri.conf.json`。
- 开发前端命令：`beforeDevCommand = node ../../scripts/halo-web-ui-dev-server.mjs`，`devUrl = http://localhost:1422`。
- 正式前端命令：`beforeBuildCommand = node ../../scripts/halo-web-ui-build.mjs`，`frontendDist = ../../../dist`。
- Halo scope：`product/Halo Studio/halo-scope.json` 的 `frontendRoot = src/web-ui`，`desktopRoot = src/apps/halo-desktop`。
- Web UI 真实入口：`product/Halo Studio/src/web-ui/index.html` -> `product/Halo Studio/src/web-ui/src/main.tsx`。
- Web UI 构建脚本：`product/Halo Studio/scripts/halo-web-ui-build.mjs` 校验 `src/web-ui/index.html` 与 `src/web-ui/src/main.tsx`，复制 Halo 图标到 `src/web-ui/public/halo-icon.svg`，执行 `pnpm --dir src/web-ui run build:desktop`。
- 开发服务器脚本：`product/Halo Studio/scripts/halo-web-ui-dev-server.mjs` 执行 `pnpm --dir src/web-ui run dev`，并设置 `HALO_PRODUCT_SCOPE=local-coding`、`HALO_PRODUCT_NAME=Halo Studio`。
- 产物：`product/Halo Studio/dist/index.html` 与 `product/Halo Studio/dist/assets/*`；`product/Halo Studio/target/release/halo-studio.exe`；`product/Halo Studio/target/release/bundle/msi/Halo Studio_0.1.0_x64_en-US.msi`；`product/Halo Studio/target/release/bundle/nsis/Halo Studio_0.1.0_x64-setup.exe`。

### BitFun UI 复用范围

真实 UI 来源保留在 `product/Halo Studio/src/web-ui`，包括：

- 应用入口、启动主题、字体：`index.html`、`src/main.tsx`、`public/fonts/fonts.css`、`src/infrastructure/theme`、`src/infrastructure/font-preference`。
- 核心布局和导航：`src/app/layout/AppLayout.tsx`、`src/app/components/NavPanel/*`、`src/app/components/SceneBar/*`、`src/app/scenes/SceneViewport.tsx`。
- 工作区和会话：`src/app/components/NavPanel/sections/workspaces/*`、`src/app/components/NavPanel/sections/sessions/*`、`src/app/scenes/session/*`。
- 文件树和编辑器：`src/tools/file-system/*`、`src/app/scenes/file-viewer/*`、`src/app/components/panels/content-canvas/*`、`src/tools/editor/*`。
- 终端和版本控制：`src/app/scenes/shell/*`、`src/tools/terminal/*`、`src/app/scenes/git/*`、`src/tools/git/*`。
- 主题、令牌、组件密度：`src/component-library/styles/*`、`src/app/styles/*`、`src/shared/theme/*`。

Halo 适配仅限产品名、图标、启动标题、核心中文文案、`local-coding` runtime scope、更新/Agent Companion 等范围外入口裁剪；未以 `src/halo-workbench` 或静态 mock 替换 BitFun Web UI。

### Halo 品牌和目录适配

- 产品树目录已由 `product/bitfun` 改为 `product/Halo Studio`；Tauri、Vite、scope、脚本、文档和构建路径均指向新目录，正式构建不依赖 `D:\BitFun-main`。
- Web UI 的用户可见产品名称、启动标题、Splash 文案、Canvas 默认标题和核心简体中文文案已改为 Halo Studio。
- 欢迎页“早安，编程搭档”旁边的 BitFun panda/logo JSX 和对应样式已移除；Tauri 应用启动图标保留。
- 为保持真实 BitFun UI 的运行时兼容性，CSS class、事件名、存储键、内部 crate/package 名、上游许可证和技术协议标识未做全局重命名；这些不是用户可见品牌文案。

### 验证命令

所有最终验收命令均在 VS x64 环境中执行。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `node scripts/halo-scope.mjs` | `0` | `frontendRoot=src/web-ui`，`frontendEntry=src/web-ui/src/main.tsx`，`buildOutDir=dist`，包含 local workspaces / coding sessions / file explorer / editor / git / terminal，排除 office collaboration / mini app / remote workspace / relay / mobile client。 |
| `node --test scripts/halo-scope.test.mjs` | `0` | 9 个测试通过，覆盖 Halo wrapper、真实 Web UI 入口、Tauri hook、desktop build、preview 和范围裁剪。 |
| `pnpm run type-check:web` | `0` | 映射到 `pnpm --dir src/web-ui run type-check`，即 `tsc --noEmit`。 |
| `pnpm --dir src/web-ui run test:run src/app/scenes/settings/SettingsScene.test.tsx` | `0` | 映射到 `src/web-ui/package.json` 的 `test:run=vitest run`；1 个测试文件、5 个测试通过。 |
| `pnpm run build` | `0` | 映射到 `node scripts/halo-tauri.mjs build`；先执行 Halo scope 和真实 `src/web-ui` desktop Vite 构建，再编译 `halo-studio.exe` 并生成 MSI/NSIS 包。 |
| `pnpm run desktop:build` | `0` | 映射到同一 `node scripts/halo-tauri.mjs build`；独立复验桌面正式构建入口，生成同一 release exe 和两个 bundle。 |
| `node scripts/halo-native-smoke-03a1.mjs` | `0` | 启动当前 worktree 的 release `halo-studio.exe`，PID 绑定原生窗口，验证真实 BitFun/Halo Web UI 与关键交互；最终摘要为 `03a1-native-smoke-20260731-034714-summary.json`。 |
| `git diff --check` | `0` | 无空白错误；Git 输出大量 vendor 文件 CRLF/LF 规范化 warning，归类为换行提示，不是 diff-check 失败。 |

补充说明：一次较早的 `pnpm run build` 直接运行因工具超时在 604 秒被切断，未取得有效退出码；确认残留进程属于本 worktree 构建并等待自然结束后，已在 VS x64 环境以更长超时重跑并取得退出码 `0`。两次较早 native smoke 在文件树/编辑器证据过窄处失败，归类为 smoke 对真实 BitFun 打开态的 selector/交互证据不足；随后修正为点击真实非目录 `main.ts` 节点、等待 Monaco 模型加载，并要求 tab、code editor、文件路径和真实模型内容证据，最终退出码 `0`。最终 smoke 摘要中 `monacoModelCount=1`、`monacoTextLength=34`，仅包含临时 smoke 文件内容。定向 Vitest 曾以 `pnpm --dir src/web-ui exec vitest...` 运行并因离线 workspace 的可执行文件解析失败退出 `1`；随后使用 package script 的 `pnpm --dir src/web-ui run test:run <file>` 成功。另一次带 `--` 的参数转发使 Vitest 错误地运行全量测试，属于命令转发错误，不纳入最终测试结论。

### 原生 Tauri smoke

最终结构化摘要：

- `docs/requirements/bitfun-tauri-product-migration/artifacts/03a1-native-smoke-20260731-034714-summary.json`

最终脱敏截图：

- `docs/requirements/bitfun-tauri-product-migration/artifacts/03a1-native-smoke-20260731-034714-wide-before.png`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03a1-native-smoke-20260731-034714-wide-after.png`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03a1-native-smoke-20260731-034714-narrow-after.png`
- `docs/requirements/bitfun-tauri-product-migration/artifacts/03a1-native-smoke-20260731-034714-cdp-after.png`

摘要结果：

- 启动进程：`[product/Halo Studio]/target/release/halo-studio.exe`，PID `11072`。
- 窗口绑定：`windowBoundToPid=true`，窗口句柄 `0xC0B8C`，可见且非空；wide/narrow/native/CDP 截图均生成。
- 前端证明：`url=http://tauri.localhost/`，`lang=zh-CN`，`haloScope=local-coding`，`productId=halo-studio`。
- BitFun 真实选择器：`appLayout=true`、`workspaceBody=true`、`navPanel=true`、`sceneViewport=true`、`sceneBar=true`、`fileViewer=true`、`gitScene=true`、`shellPanel=true`。
- 旧页面排除：`oldHaloWorkbenchAbsent=true`，`scriptSourcesContainHaloWorkbench=false`。
- 品牌泄漏排除：`visibleBrandLeaks.bitfunText=false`，`rawI18nKeys=false`，`updateDialog=false`。
- 工作区：`openWorkspaceCommandOk=true`，`workspaceVisible=true`，仅记录 `[temp]/halo-03a1-smoke-workspace` 脱敏路径。
- 工作区入口：加号按钮和打开项目菜单项可见。
- 编码会话入口：`codeSessionButton=true`，`sessionScene=true`。
- 文件树/编辑器：点击真实非目录 `main.ts` 节点，`dataFile=true`，`dataIsDirectory=false`，打开 `canvas-tab[data-tab-type=code-editor]`，`codeEditor=true`，`editorReady=true`，`openedFileTextVisible=true`；真实 Monaco 模型 `monacoModelCount=1`、`monacoTextLength=34`。
- Git：`gitScene=true`，`gitInitOrStatus=true`。
- 终端：`shellPanel=true`，`shellTitle=true`。
- 脱敏：summary 明确省略 remote debugging port、CDP websocket URL、session ID、message ID、Authorization header；不记录完整工作区路径。

### 范围、路径和许可审计

- 生产入口链扫描 `D:\BitFun-main`：退出码 `1`（无匹配），结论为 `no-D-BitFun-main-reference-in-production-chain`。
- 生产入口链扫描 `src/halo-workbench`：退出码 `1`（无匹配），结论为 `no-src-halo-workbench-reference-in-runtime-build-entry`。`halo-scope.mjs` 内存在 `src/halo-workbench` 字符串仅作为禁止项守卫，不是生产入口引用。
- `product/Halo Studio/LICENSE` 保留 BitFun MIT License，Copyright `(c) 2026 CWing`。
- `product/Halo Studio/vendor/cargo` 有 1091 个 crate 目录；按文件名扫描有 1401 个 license/licence/copying/notice/copyright 文件；每个 vendor crate 均有顶层许可证类文件或 Cargo manifest 的 `license` / `license-file` 字段。
- `D:\BitFun-main` 未修改、未写入；主工作区未编辑；未提交、未推送、未创建分支、未改 Git 历史；未进入工单 04。

### 最终判定

03A1 达到完成门槛：正式 Tauri 入口实际加载 `product/Halo Studio/src/web-ui`，真实原生窗口 smoke 通过，范围裁剪、许可证、构建和差异审计均通过。状态为 `ready-for-review`，具备后续人工审查条件。

## 不在本票范围

- 不实现 Workbench Runtime command/event 契约，不迁移 OpenCode、凭据、受管任务或中断语义。
- 不把 BitFun 全部办公、远程、Relay、移动端能力接入 Halo。
- 不修改或提交 `D:\BitFun-main`，不向 BitFun 上游推送 Halo 改动。
- 不安装软件，不修改系统 PATH、注册表、全局 Cargo 配置或系统环境变量。
- 不删除旧 QML、Python、Sidecar 或旧启动入口。
- 不把 HTTP smoke、截图占位或静态 HTML 通过报告为 BitFun 原生 UI 验收。

## 完成门槛

只有在正式 Tauri 入口确实加载 BitFun `src/web-ui`、真实原生窗口通过上述 UI 验收、范围与许可证审计通过且证据已写入文档后，本票才能标记为 `ready-for-review`。若真实 BitFun UI 无法构建、无法接入或只能保留手写替代页面，必须标记为 `blocked`，不能把 03 标记为完整完成，也不能进入 04。
