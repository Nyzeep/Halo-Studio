# 07 - 在 Tauri 运行时探测并启动 OpenCode 1.x

**What to build:** Halo Workbench Runtime 的 OpenCode Server Adapter 可以探测用户本机 OpenCode 1.x、受控启动 `opencode serve`、完成真实健康与能力检查，并把就绪或失败状态投影到工单 04 的 Interface。版本、能力、认证或健康检查失败均如实显示，不回退模拟协议。

**Blocked by:** 04 - 建立 Halo Workbench Runtime 公共契约；06 - 管理 OpenCode Provider、模型与系统凭据.

**Status:** ready-for-agent

## 兼容性档案

- P0 档案名为 `opencode-server-1.x`；版本检查只是入口，真正放行依赖所需能力和语义。
- 必需能力至少包括：`/global/health`、`/provider`、`/session`、`/session/:id/prompt_async`、`/event`、permission/question、`/session/:id/abort` 和 `/global/dispose`。
- OpenCode 版本间的工作区路由（例如 `x-opencode-directory`）、请求/事件载荷和 endpoint 细节封装在 Adapter 实现中，不进入 Halo 公共 Interface。
- 新主版本必须建立新档案；未知 1.x 若能力探测不完整也必须失败关闭。

## 验收标准

- [ ] probe 使用用户可见的本机 `opencode` 可执行文件，记录可公开版本与能力结论；不下载、打包、升级或从 `D:\opencode-dev` 运行源码。
- [ ] start 只绑定 `127.0.0.1` 随机端口，每次生成新的 Basic 认证材料，使用受控环境和明确 OpenCode 数据目录；端口与认证不进入公开状态。
- [ ] 标准会话使用 Halo 管理的可持久 OpenCode profile；受管任务使用隔离 profile，任务结束、取消或中断后清理原始会话状态，符合标准/受管保留策略分离。
- [ ] readiness 必须通过带认证的真实 `/global/health` 和 Provider/Session/事件能力探测；仅观察 stdout 监听行不算就绪。
- [ ] ready、failed、stopping 和恢复建议经 Halo Workbench Runtime 投影；OpenCode stderr 先脱敏、限长，再进入诊断。
- [ ] stop 先调用 `/global/dispose`，在受控宽限期后回收子进程；失败或超时必须有确定强制清理结果。
- [ ] 工作区切换、信任撤销、应用退出和并发 start/stop 不留下孤儿 OpenCode 进程或可复用认证材料。
- [ ] 生产路径不存在旧 JSONL、ACP 或模拟执行器的静默回退；若保留 BitFun ACP 能力，只能属于范围外标准生态，不能成为本票受管执行路径。

## 验证要求

- 受控替身测试覆盖兼容通过、不支持版本、能力缺失、认证失败、健康失败、事件流断开、dispose 失败、强制回收和敏感字段脱敏。
- Windows 集成测试绑定真实子进程并断言只监听回环、错误密码返回未授权、每次认证不同、退出后端口和进程释放。
- 至少执行一次已安装 OpenCode 1.x 的真实 probe/start/health/provider/dispose 资格验证；只记录版本、档案和脱敏结论。

## 不在本票

- 不创建真实标准或受管 Agent 回合；工单 05 和 08 分别负责。
- 不复制 OpenCode Provider/Core/Session/Agent 源码，不修改用户全局 PATH 或 OpenCode 安装。
