# 工单 02：BitFun 上游导入证据

**Status:** ready-for-review

本工单把一个可由上游仓库追溯的 BitFun commit 导入 Halo 自有的
`product/Halo Studio/` 源码树。完整的逐文件机器清单位于
`bitfun-upstream-manifest.json`，可使用 `npm run bitfun:verify-import` 重复核对。

## 上游来源

- 仓库：`https://github.com/GCWing/BitFun.git`
- 引用：`refs/heads/main`
- 精确 commit：`ca56631e38f36db675583288df2bd44c540d250a`
- 远端关系核对：`git ls-remote https://github.com/GCWing/BitFun.git HEAD refs/heads/main`，退出码 `0`，`HEAD` 和 `refs/heads/main` 均指向上述 commit。
- 导入日期：`2026-07-29`

`D:\BitFun-main` 仅作为只读参考树检查。以下命令确认它不是 Git 工作树，
因此没有可记录的本地 remote、commit、分支或引用，也没有把它作为来源证明：

| 只读检查 | Git 退出码 | 结果 |
| --- | ---: | --- |
| `git -C D:\BitFun-main rev-parse --is-inside-work-tree` | 128 | 不是 Git 工作树 |
| `git -C D:\BitFun-main remote -v` | 128 | 没有可读取的 remote |
| `git -C D:\BitFun-main rev-parse HEAD` | 128 | 没有可读取的 commit |
| `git -C D:\BitFun-main branch --show-current` | 128 | 没有可读取的分支 |
| `git -C D:\BitFun-main status --porcelain=v2 --branch` | 128 | 没有可读取的工作树状态 |

共同错误为：`fatal: not a git repository (or any of the parent directories): .git`。

参考树的远端 URL、精确 commit、branch/ref 和工作树状态均记录为
`unavailable: no Git metadata`。最终导入内容由上游 commit 的 Git tree
逐文件 blob SHA 证明。

## 导入范围

- 目标：`product/Halo Studio/`
- 范围：上游 commit 的完整 Git tree，`5254` 个文件、`6241` 个 tree/blob 条目。
- 组装方式：参考树中与上游 blob 相同的 `4784` 个文件复用本地内容；其余
  `470` 个文件从上游 Git blob 获取；最终 `5254` 个文件全部通过上游 blob SHA 核对。
- 清单：每个文件的相对路径、Git mode、blob SHA 和字节数见
  `bitfun-upstream-manifest.json`。

排除内容：上游 `.git` 历史和对象、submodule/gitlink、参考树的未跟踪或忽略
文件、仓库外绝对路径依赖和本地临时构建产物。上游 commit 中已经被 Git 跟踪的
`src/crates/contracts/product-domains/src/miniapp/builtin/assets/ppt-live/dist/ui.bundle.js`
随完整源树保留；这不表示它是本地构建输出。

范围外 BitFun 模块仅作为完整源码关系保留，不报告为 Halo 已支持能力；工单 02
没有添加构建、路由、导航、初始化或 Tauri 启动入口。

## 许可证和归属

- BitFun MIT 许可证、原版权声明：`product/Halo Studio/LICENSE`
- 上游嵌套许可证和适用归属：`product/THIRD_PARTY_NOTICES.md` 及其中列出的
  `product/Halo Studio/**/LICENSE*` 文件。
- 依赖声明位置：`product/Halo Studio/package.json`、`pnpm-lock.yaml`、`Cargo.toml`
  和 `Cargo.lock`。

导入没有改写 BitFun 源文件中的版权或许可证文字；Halo 自有归属索引位于
`product/THIRD_PARTY_NOTICES.md`。

## 验证记录

- 机器清单、上游身份、Git mode/type 元数据和正式入口外部路径门禁：
  `npm run bitfun:verify-import`，退出码 `0`。
- 外部路径扫描：对 Halo 正式脚本、配置、构建入口和测试入口执行
  `rg -n -F 'D:\BitFun-main' package.json package-lock.json scripts sidecar product`，
  退出码 `1`（无匹配）；同一检查也由上述 verifier 自动执行。
- 结构检查：无 `.git` 目录、`.gitmodules`、submodule/gitlink 或本地构建产物。
- 格式检查：`git diff --check`。

本工单只证明源码导入、来源和许可证审计；它不证明 Tauri 应用已经启动。
正式桌面入口属于工单 03。
