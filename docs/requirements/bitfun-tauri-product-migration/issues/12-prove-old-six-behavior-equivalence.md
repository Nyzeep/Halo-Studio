# 12 - 证明旧六票到 OpenCode Tauri 产品的行为等价迁移

**What to build:** 发布负责人可以通过一份逐项矩阵确认旧 GitHub #9–#14 的可迁移能力已经在 OpenCode-backed Halo Tauri 产品中获得等价的用户可观察证据，同时保留旧 issue 的历史状态，不把旧内部协议或多执行器设想当作兼容目标。

**Blocked by:** 11 - 保持 OpenCode 中断如实化与不重放语义.

**Status:** ready-for-agent

## 验收标准

- [ ] 矩阵逐项记录：旧 GitHub issue/行为、旧证据、新 Halo Runtime Interface、新 OpenCode Adapter 证据、新原生桌面路径和当前结论。
- [ ] 工作区、配置与凭据、OpenCode 兼容启动、首轮 Session、一次性 permission/question、追问、显式结束、只读审查和中断均有新产品证据。
- [ ] 旧 #9–#14 不改写标题、需求、状态或历史验收；新矩阵是前向迁移证据，不是对旧 issue 的重新实现记录。
- [ ] Rust Adapter/Runtime 契约、前端测试、Tauri 桌面烟测和必要端到端测试从 Halo 正式入口执行并通过。
- [ ] 旧 Sidecar JSONL、Python/QML、私有 `opencode.rs` 布局、OpenCode 内部源码和原始远程标识不得成为等价断言。
- [ ] 旧基线中 Pi 或多执行器相关行为明确归类为 P0 延后，而不是虚构成 OpenCode 已覆盖；只有当前 P0 必需的 OpenCode 行为进入发布结论。
- [ ] 缺失、失败或尚未人工执行的项目明确标为阻断，不得推断或伪造通过。

## 验证要求

- 每个矩阵结论必须链接到精确自动化命令或脱敏 artifact；仅代码存在、HTTP 静态页面或受控替身不能替代原生真实验收。
- 加入一致性检查，确保所有旧六票条目恰好映射一次、所有 P0 工单 04–11 均有覆盖、所有失败均有分类。

## 不在本票

- 不修改或关闭旧 GitHub #9–#14。
- 不执行真实外部凭据会话；工单 14 负责真实 OpenCode 原生 UI 验收。
