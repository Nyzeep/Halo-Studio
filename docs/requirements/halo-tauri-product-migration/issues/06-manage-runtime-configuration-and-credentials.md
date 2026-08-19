# 06 - 管理 Pi Provider、模型与系统凭据

**What to build:** 本地开发者可以在 Halo 工作台配置 Pi 使用的 Provider、Base URL、模型、思考级别和受控启动选项，将密钥一次性写入系统凭据存储，并让标准模式与受管模式共享同一份非敏感配置权威源。

**Blocked by:** 03B - 固定 P0 Pi RPC 受管执行器（废弃历史 OpenCode Server 决策）；04 - 建立 Halo Workbench Runtime 公共契约.

**Status:** ready-for-agent

## 实现边界

- Halo 配置服务是非敏感选择与 `credential_ref` 的唯一产品权威；Pi 原生配置只在受控投影或显式配置事务中使用。
- 系统凭据读取只发生在 Pi RPC 子进程创建边界内；明文不得进入 Renderer、日志、SQLite、备份、Pi `auth.json` 或提交。
- Provider/model 能力校验由 Pi 原生能力和经审计配置驱动；不复制 Pi Provider/SDK，不执行任意项目配置命令。

## 验收标准

- [ ] 配置 Interface 只持久化 `provider_id`、可选 `base_url`、`model_id`、思考级别、允许的启动选项和 `credential_ref`；不持久化凭据明文，也不把 Pi `auth.json` 作为 Halo 配置权威。
- [ ] 凭据录入通过隔离 command 一次性写入 Windows Credential Manager 并返回引用；系统存储不可用、引用缺失或读取失败时失败关闭。
- [ ] Pi 的 Provider/model 能力和可用性结果用于校验配置；Halo 不复制 Pi Provider 注册表、SDK 或模型实现。
- [ ] 凭据只在 `pi --mode rpc` 子进程创建前从系统存储读取，并通过受控环境或已验证的 Pi Provider 认证入口临时注入；不使用可观察的 `--api-key`，普通配置读取永不返回明文。
- [ ] 明确区分 Pi 配置边界：`models.json` 只作为经审计的 Provider/model 元数据输入，`settings.json`/项目 `.pi` 配置不允许在 P0 触发 package 或 extension discovery，`auth.json`/OAuth 状态不作为 Halo 凭据库，也不由 Halo 写入或展示。
- [ ] 标准和受管模式读取同一个 Halo 配置权威源，但可投影到各自隔离的 Pi config/session 目录，不形成两套漂移设置。
- [ ] 不自动改写用户全局 `models.json`、`settings.json` 或 `auth.json`；Halo 配置事务只允许明确纳入审计的非敏感 `models.json`/`settings.json` 变更，含 Diff 预览、确认、冲突检测和回滚。`auth.json`、OAuth 状态和任何凭据材料永久排除在 Halo 配置事务之外。
- [ ] 前端状态、Tauri payload、事件、日志、备份、错误、Pi 启动输出和交付历史不包含密钥、Authorization、完整 Base URL 查询凭据、模型扩展命令或可还原认证信息。

## 验证要求

- 自动化覆盖录入、读取引用、更新、删除、缺失引用、系统存储失败、Provider/模型不匹配、Base URL 校验和统一脱敏。
- 使用随机 canary 扫描前端状态、序列化 payload、应用数据、日志和错误；测试后删除合成凭据并证明无残留。
- Pi RPC Adapter 只接收启动所需的临时 secret 值，Debug/Display/错误实现必须隐藏该值；不向 Pi `auth.json` 回写 Halo 凭据。

## 精确验证命令

```powershell
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p halo-agent-runtime --test workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run type-check:web
git diff --check
```

测试使用合成 canary；不得读取真实凭据、执行真实 Provider 认证或发送模型请求。

## 不在本票

- 不登录真实外部 Provider，不发送真实模型请求；真实凭据和外部写入须在工单 14 单独授权。
- 不支持历史 OpenCode Server、Halo Studio 内置 Code Agent 或明文 JSON 配置回退。
