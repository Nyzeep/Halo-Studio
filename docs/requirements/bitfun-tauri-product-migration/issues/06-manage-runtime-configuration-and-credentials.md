# 06 - 管理 OpenCode Provider、模型与系统凭据

**What to build:** 本地开发者可以在 Halo 工作台配置 OpenCode 使用的 Provider、Base URL、模型、思考级别和启动选项，将密钥一次性写入系统凭据存储，并让标准模式与受管模式共享同一份非敏感配置权威源。

**Blocked by:** 04 - 建立 Halo Workbench Runtime 公共契约.

**Status:** ready-for-agent

## 验收标准

- [ ] 配置 Interface 只持久化 `provider_id`、可选 `base_url`、`model_id`、思考级别、允许的启动选项和 `credential_ref`；不持久化凭据明文。
- [ ] 凭据录入通过隔离 command 一次性写入 Windows Credential Manager 并返回引用；系统存储不可用、引用缺失或读取失败时失败关闭。
- [ ] OpenCode `/provider` 能力结果用于校验 Provider/模型是否可用；Halo 不复制 OpenCode Provider 注册表或 Provider SDK 源码。
- [ ] 凭据只在启动或认证瞬间从系统存储读取，并通过受控子进程环境或 OpenCode 支持的认证入口临时注入；普通配置读取永不返回明文。
- [ ] 标准和受管模式读取同一个 Halo 配置权威源，但可投影到各自隔离的 OpenCode profile，不形成两套漂移设置。
- [ ] 不自动改写用户全局 `opencode.json`；任何原生配置文件改动必须是独立配置事务，含 Diff 预览、确认、冲突检测和回滚。
- [ ] 前端状态、Tauri payload、事件、日志、备份、错误、OpenCode 启动输出和交付历史不包含密钥、Authorization、完整 Base URL 查询凭据或可还原认证信息。

## 验证要求

- 自动化覆盖录入、读取引用、更新、删除、缺失引用、系统存储失败、Provider/模型不匹配、Base URL 校验和统一脱敏。
- 使用随机 canary 扫描前端状态、序列化 payload、应用数据、日志和错误；测试后删除合成凭据并证明无残留。
- OpenCode Adapter 只接收启动所需的临时 secret 值，Debug/Display/错误实现必须隐藏该值。

## 不在本票

- 不登录真实外部 Provider，不发送真实模型请求；真实凭据和外部写入须在工单 14 单独授权。
- 不支持 Pi、BitFun 内置 Code Agent 或明文 JSON 配置回退。
