# 第三方软件与许可证说明

本文件记录 Halo Studio 当前工作树中直接声明的第三方依赖及其包元数据所载许可证。清单依据 `package-lock.json` 和本工作树已安装的包清点于 2026-07-24；它不是替代各依赖分发包中完整许可证文本的法律意见。

发布前必须在干净环境重新安装锁定依赖，核对实际发布产物的生产依赖，并随产物附带适用的完整许可证与版权声明。尚未存在桌面打包/发布流程时，不应把本文件当作已完成的发布合规证明。

## 运行时依赖

| 组件 | 当前版本 | 许可证 | 用途 |
| --- | ---: | --- | --- |
| Electron | 33.4.11 | MIT | 桌面 Main、Preload 与窗口运行时 |
| React / react-dom | 18.3.1 | MIT | Renderer UI |
| lucide-react | 0.468.0 | ISC | 工作台图标 |
| Zod | 3.24.1 | MIT | IPC 与数据契约校验 |
| opencode-ai | 1.18.4 | MIT | OpenCode 的锁定运行时工件来源 |
| better-sqlite3 | 11.7.0 | MIT | 本地 SQLite 存储 |
| diff | 7.0.0 | BSD-3-Clause | 配置事务差异生成 |
| jsonc-parser | 3.3.1 | MIT | JSONC 配置解析与修改基础 |

## Pi 契约参考

Pi `0.81.1` 仅作为运行时探测和 JSONL RPC 兼容性的契约参考。Halo Studio 没有复制、打包或分发 Pi 源代码、图标或二进制工件；若未来将 Pi 的任何工件纳入发布物，必须先核对上游许可证、归属要求和分发条件，并在本文件及发布 notice 中补充精确记录。

## 构建与测试依赖

| 组件 | 当前版本 | 许可证 | 用途 |
| --- | ---: | --- | --- |
| TypeScript | 5.7.3 | Apache-2.0 | 类型检查与构建 |
| Vite | 6.4.3 | MIT | Renderer、Main 与 Preload 构建 |
| @vitejs/plugin-react | 4.3.4 | MIT | React 构建插件 |
| Vitest | 2.1.8 | MIT | 单元、集成与 Renderer 测试 |
| @testing-library/react | 16.1.0 | MIT | React 组件测试 |
| @testing-library/jest-dom | 6.6.3 | MIT | DOM 断言 |
| jsdom | 25.0.1 | MIT | 测试 DOM 环境 |
| @types/node | 22.10.5 | MIT | Node 类型声明 |
| @types/react | 18.3.12 | MIT | React 类型声明 |
| @types/react-dom | 18.3.1 | MIT | React DOM 类型声明 |
| @types/better-sqlite3 | 7.6.12 | MIT | better-sqlite3 类型声明 |

## 图标与参考资料边界

当前仓库的界面图标来自 `lucide-react`。`用于参考的几个项目的代码/` 是只读参考资料，不是本项目的依赖、构建输入或发布内容；本仓库没有从该目录复制代码、图标或其他资源。

若未来引入新的第三方代码、图标、字体、二进制工件或资源，必须在合并前：

1. 记录来源、版本、许可证和所需归属说明。
2. 核对该许可证是否适用于目标分发方式。
3. 更新本文件及发布产物的完整 notice 清单。

用于发布前复核的最低命令：

```powershell
npm ci
npm ls --omit=dev --all
npm audit --omit=dev
```

`npm audit` 的结果需要结合上游修复状态评估；命令成功或失败都不能替代许可证审查。
