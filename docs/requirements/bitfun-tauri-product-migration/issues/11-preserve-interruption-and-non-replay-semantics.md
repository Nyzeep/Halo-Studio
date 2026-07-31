# 11 - 保持 OpenCode 中断如实化与不重放语义

**What to build:** Halo 工作台、Workbench Runtime、OpenCode 进程或 SSE 连接意外退出后，未结束任务如实显示为中断；重启不会自动重连旧 Session、重发 Prompt、重放操作请求或重复工作区写入。

**Blocked by:** 10 - 追问、显式结束并审查 OpenCode 交付.

**Status:** ready-for-agent

## 验收标准

- [ ] 应用关闭、Runtime 崩溃、OpenCode 子进程退出、健康失败和事件流永久失联都会把非终态受管任务归类为 `interrupted`，不得伪装为完成或等待开发者。
- [ ] 用户主动取消先调用当前 Session 的 OpenCode `/abort`，再在宽限期内等待原生停止；超时才强制回收子进程，并记录 native/forced 取消方式。
- [ ] 取消、关闭或中断后的进程级清理调用 `/global/dispose` 并回收 OpenCode 子进程；任何迟到事件不得追加到已终态证据。
- [ ] 重启后不自动恢复原生 Session，不重发首轮或追问，不重放 permission/question 决议，也不重复文件写入。
- [ ] 已产生的工作区改动、任务基线和可审查证据按事实保留；受管 OpenCode 临时 profile、活动消息、认证材料和敏感运行态按隐私边界清理。
- [ ] 用户可以从原生 UI 看见脱敏中断原因和明确的“重新开始新运行/保持现状/进入审查”可选处置；任何新运行产生新证据版本。
- [ ] 标准 OpenCode 会话历史与受管中断隔离，受管清理不得删除标准模式历史。

## 验证要求

- 集成测试在首轮前、Prompt 进行中、等待操作请求、等待开发者、追问和结束取证期间分别强杀应用/OpenCode，验证状态、进程、Git 和不重放。
- 测试断言重启零自动 OpenCode 请求、零重复写入、零旧认证复用和零原始远程标识持久化。
- Windows 进程测试证明无残留 `opencode` 子进程、端口监听和受管临时 profile。

## 不在本票

- 不自动续跑、自动重试、自动恢复或静默创建替代 Session。
- 不把网络瞬断的无限重连策略引入 P0；有限连接恢复必须保持不重发语义并由兼容性档案覆盖。
