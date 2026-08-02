# 13 - 演练 BitFun 上游同步、Pi extension 依赖与许可证门槛

**What to build:** 维护者可以把一个新的 BitFun 上游 commit 作为独立候选同步到 Halo，审查冲突与产品范围，并同时审计 Halo 第一方 Pi extension 的来源、固定版本、权限、运行时依赖和许可证；通过构建、回归和许可证门槛后决定是否接纳。同步不得破坏 Halo Workbench Runtime 的 Pi RPC seam，也不得触碰 Pi 参考目录或任何历史 OpenCode 上游仓库。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；04 - 建立 Halo Workbench Runtime 公共契约.

**Status:** ready-for-agent

## 实现边界

- BitFun 上游同步候选、Pi extension 审计和产品发布树保持可审计边界；任何失败候选可丢弃，不污染已验证树、Pi 参考目录或上游。
- extension 清单必须覆盖加载来源、固定版本/hash、依赖、宿主权限、工具影响、许可证和更新责任；清单缺失或审计未完成时，候选不能成为发布版本，也不能把任意路径加入显式 `--extension` 加载清单。
- 本票不更新 Pi、下载包、修改 GitHub Issue、复制 `D:\pi-main` 或改变 P0 transport。

## 验收标准

- [ ] 从只读 BitFun 上游参考树获取一个不同于初始导入的精确 commit，并记录前后来源。
- [ ] 同步候选保持为独立、可审计的 Halo 变更，不自动合并，也不向 BitFun 上游提交或推送。
- [ ] 冲突处理保留 Halo 品牌、产品裁剪、Workbench Runtime 公共 Interface 和 Pi RPC Adapter seam，并记录所有需要人工判断的差异。
- [ ] 候选通过 Tauri 构建、产品范围检查、工单 04 Runtime Interface、工单 07 Pi RPC 契约和来源清单核对。
- [ ] Pi 第一方 extension inventory 记录源文件、来源 commit/tag、固定版本或内容 hash、加载参数、可访问工具/事件、宿主权限、网络/文件/进程影响、直接/传递依赖及许可证；未审计 extension 不得进入 `--extension` 加载清单。
- [ ] extension 依赖锁定在 Halo 可审计的 lockfile/manifest 中，不允许运行时从 npm、Git 或项目 `.pi/extensions` 自动下载；依赖漏洞、来源不可追溯或许可证不兼容均阻止接纳。
- [ ] 源码与发行包包含所需 BitFun MIT 归属、Pi extension 许可证和第三方声明，许可证检查失败阻止接纳。
- [ ] 同步不引入 `D:\BitFun-main`、`D:\pi-main`、`D:\opencode-dev` 或其他外部绝对路径，不复制/vendor Pi 或历史 OpenCode 内部源码，也不启用历史 OpenCode/Code Agent P0 路径。

## Halo 第一方 Pi extension inventory

这是本票必须填写并随候选保存的最小记录。当前实现的 extension 是 Halo 仓库内固定源码，不是从 Pi、npm、Git 或项目 `.pi/extensions` 安装的第三方包；该事实不能替代来源、hash 和许可证审计。

| 字段 | 当前 P0 事实与审计要求 |
| --- | --- |
| ID/版本 | `halo-workbench-permission-gate`, `HALO_PI_EXTENSION_VERSION = 1.0.0`；版本变更必须重新建候选并重新验收。 |
| 源码与来源 | `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts`，由 Rust `include_str!` 固定进 Adapter；记录来源 commit、`git hash-object` 和 SHA-256，不接受只记录文件名。 |
| 安装与加载 | Adapter 在每个受控进程独立的 `%TEMP%\\halo-studio\\pi-extensions\\<extension-id>-<uuid>\\` 目录写入 `<extension-id>-<sha256>.ts`，校验固定源码后以 `--no-extensions --extension <exact-path>` 加载，退出时清理；不得写入用户全局、项目 `.pi` 或 Pi `settings.json`。 |
| 可访问能力 | 仅注册 `tool_call` 前置拦截并调用 `ctx.ui.confirm`，由 Pi RPC 的 `extension_ui_request/response` 传递一次性决议；源码不得直接调用文件、网络、进程、凭据或 Git API。 |
| 宿主权限 | extension 与 Pi 进程仍继承启动用户权限；Halo 的工作区信任、任务状态和 fail-closed 决议是外部边界，不能声称 extension 提供沙箱。 |
| 依赖 | `@earendil-works/pi-coding-agent` 只出现在 TypeScript `import type`，不得进入运行时依赖或 lockfile 新增下载；实际 loader 来自用户已安装 Pi，运行时禁止 npm/Git 下载。候选必须用 `cargo tree`、`rg` 和 lockfile 核对这一点。 |
| 许可证 | Halo extension 源码的许可只能按仓库 `LICENSE` 和发布 notice 审计后记录；不能把 Pi 安装包、Pi Provider 或 `@earendil-works/pi-coding-agent` 的许可证推断为已审计。Pi 二进制不随 Halo P0 分发；若未来分发，许可证、归属和完整文本必须另行阻断审查。 |
| 当前门槛 | 没有来源 commit、hash、依赖清单、宿主权限说明和逐项许可证证据时，状态为 `blocked`，不能把 `--extension` 视为放行证据。 |

## 验证要求

- 记录精确上游 commit、候选 diff、冲突决策、清单变化、Pi extension manifest/hash/依赖/权限/许可证 inventory 和全部门槛命令/退出码。
- 失败候选必须可安全丢弃，不污染已验证产品树或上游参考树。

## 精确验证命令

```powershell
$extension = "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts"
Get-FileHash -Algorithm SHA256 $extension
git hash-object -- $extension
rg -n 'HALO_PI_EXTENSION_ID|HALO_PI_EXTENSION_VERSION|HALO_PI_EXTENSION_PERMISSIONS|include_str!|--no-extensions|--extension' "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/lib.rs"
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio" run product:check
pnpm --dir "product/Halo Studio" run product:test
pnpm --dir "product/Halo Studio" run type-check:web
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/formalPath.contract.test.ts src/infrastructure/workbench-runtime/client.test.ts
cargo tree --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter extension_decision_is_redacted_one_shot_and_duplicate_request_fails_closed
pnpm --dir "product/Halo Studio" run desktop:build:fast
git diff --check
```

上游候选还必须使用只读参考树记录 `git -C <reference-root> rev-parse --verify HEAD`、候选 commit 和 `git diff --stat <base> <candidate> -- product/Halo Studio`；`<reference-root>` 不能进入构建输入。许可证核对必须把 `LICENSE`、`THIRD_PARTY_NOTICES.md`、Cargo/PNPM lockfile、extension inventory 和实际分发文件逐项对照；本票不通过 `npx` 临时下载审计工具，不把网络可用性当作许可证证据。任何未能给出精确 hash、依赖来源、权限范围或许可证文本的项都保持阻断。
