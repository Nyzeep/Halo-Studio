# Halo Studio 2.0 重构实施交接说明（2026-09-05）

> 读者：仓库所有者（中文）与后续接手的 agent（英文版见 [halo-2-handoff-20260905.en.md](halo-2-handoff-20260905.en.md)）
> 状态：M1–M5 已完成并合入 main（merge commit `b800cad32`，lockfile 补丁 `672a73991`）；M6 挂人工验收门槛
> 本文档是**交接事实清单**：完成了什么、在哪、怎么验证、什么不能动、还剩什么

## 1. 这次做了什么（四项锁定前提）

1. **执行基座**：Halo Workbench Runtime 保持 Rust 单一权威；新增 `halo-dsh-adapter`（DSH acp 主通道）与 `halo-pi-rpc-adapter` 同级双受管 Adapter，统一收敛到 `ManagedExecutorPort`；恢复执行器选择与任务级交接（ADR-0078，supersede ADR-0072）。
2. **前端**：niri 条带空间模型 + DMS MD3 token 语言整体重写（工作区轨 + 任务条带 + Overview + 手势 P0 集）；技术栈延续 React 18 + Vite + zustand + Tauri v2（ADR-0076/0077，supersede ADR-0027/0018）。
3. **事实模型**：「不虚构历史」总原则（ADR-0080，修订 0075）——attempt 独立记录、取消落地已交付前缀 + interrupted、committed 粒度、单一脱敏闸门。
4. **迁移**：旧 P0 验收链封版为 tag `migration-baseline-20260905`；BitFun 上游树冻结只读（守卫拦截），新基座首个真实验收后整体删除（ADR-0079，票 #58 未关）。

决策链：决策地图 [#32](https://github.com/Nyzeep/Halo-Studio/issues/32)（14/14 决策票，Decisions so far 是全部决议索引）→ 规格 [#52](https://github.com/Nyzeep/Halo-Studio/issues/52)（`docs/requirements/halo-2-rewrite/00-spec.md`，M1–M6 可测试验收）→ 实现票 #53–#58（#53–#57 已关，#58 挂门槛）→ [PR #59](https://github.com/Nyzeep/Halo-Studio/pull/59) 已合并。

研究输入（主源带出处，后续 agent **先读这些再动代码**）：
- `docs/architecture/niri-interaction-research-20260905.md`（niri 交互模型与转译判断；**niri GPL-3.0，只可范式借鉴不可复制代码**）
- `docs/architecture/dms-design-language-research-20260905.md`（DMS 设计语言；**MIT 可移植**）
- `docs/architecture/dsh-upstream-state-research-20260905.md`（DSH 架构与 A–E 候选）
- `docs/architecture/dsh-adapter-protocol-research-20260905.md`（ACP/sdk 协议细节，含 DSH 源码文件:行号）
- `docs/architecture/pi-upstream-capabilities-research-20260905.md`（pi RPC 全貌与提取点）

## 2. 落地物地图（改哪里之前先看这里）

### Rust 基座（`product/Halo Studio/src/crates/`）

| 位置 | 内容 |
|---|---|
| `contracts/runtime-ports/src/managed_executor.rs` | **`ManagedExecutorPort`** trait（prompt/follow_up/abort/get_entries/决议流/事件投影/订阅）+ `ManagedExecutorCapabilityProfile`（steer/queue_events/approval_channel/entry_read 等如实 flags）+ approval 封闭枚举（`allowed-once|rejected|cancelled|unavailable`，默认 unavailable fail-closed）+ sandbox 契约层（模式枚举 + `SandboxEnforcement: full\|partial` 如实上报）+ `normalize_managed_event_summary` **单一脱敏闸门** + `ManagedExecutorKind {PiRpc, Dsh}` 封闭集 |
| `execution/agent-runtime/src/managed_event_facts.rs` | 事实模型 v2（`MANAGED_EVENT_FACT_SCHEMA_VERSION = 2`，旧事实保持可读）：三类核心 kind（用户消息摘要/Agent 回复摘要/工具活动）+ 新增 `AttemptFailed`（独立计数，不进模型可见重建）+ `TaskInterrupted`（取消落地已交付前缀）；流式帧不落事实 |
| `adapters/pi-rpc-adapter/src/managed_executor.rs` | `PiRpcManagedExecutor` 薄封装（实现统一端口）；能力档案从 `PiRpcPort::readiness()` 事实派生（未探测全 false） |
| `adapters/pi-rpc-adapter/src/lib.rs` | `SUPPORTED_PI_RPC_PROFILES` 含 **0.85.0**；`steer`（仅 0.85 且 turn 运行中）；`queue_update` 投影；`PiRpcInstallSource` 钉版（`@earendil-works` 放行 / `@mariozechner` 拒绝）；`PI_RPC_CONSUMED_COMMAND_TYPES` + 单一出口 chokepoint（`bash`/`abort_bash` 结构性到不了 stdin） |
| `adapters/dsh-adapter/`（新 crate） | `DshAdapter`（每任务一受控进程、Windows Job Object、取消阶梯）+ `acp.rs`（JSON-RPC 客户端、requestPermission→统一决议、未知 update 过滤）+ `profile.rs`（锚 0.1.3-alpha.1，acp+sdk 双通道）+ `credentials.rs`（CredentialRef env 注入、`DSH_HOME` 隔离、`.env` 非通道）+ `managed_executor.rs`（实现端口；sdk 金丝雀降级 `approval_channel=false` 事件不断链） |
| `execution/agent-runtime/src/halo_workbench.rs` | `dispatch_managed_executor_action()`：managed 会话的 prompt/follow_up/abort 走统一端口；`create_session(executor_override)` 一次性绑定进会话+任务基线（serde default 向后兼容）；`install_managed_executor()` / `available_managed_executors()` / workspace 默认执行器 |
| `assembly/core/src/halo_workbench.rs` + `Cargo.toml` | 组合根装配 `PiRpcManagedExecutor`；`halo-dsh-adapter` 经 optional feature **`dsh-executor`**（默认关）接入 |

### 前端（`product/Halo Studio/src/web-ui/src/`）

| 位置 | 内容 |
|---|---|
| `tokens/` | **视觉唯一入口**：`tokens.css`（MD3 角色命名 20 颜色角色 × `data-theme` 双主题、三档圆角/五档间距/四档字号 × `--font-scale`、动效三档）+ `theme.ts`/`themeStore.ts` |
| `workbench/state/` | 双 store 严格分离：`workbenchRuntimeStore`（事实事件投影、序号环、stale-seq 守卫）vs `workbenchUiStore`（焦点/Overview/手势瞬态）；`workbenchUiBoundary.ts` 三重边界断言 |
| `workbench/components/` | `WorkbenchShell`（键盘全集 ←→/n/o/1..9/Esc + Ctrl/⌘+K）、`WorkspaceRail`（底部恒「新建」）、`TaskStrip`（列固定宽、新列焦点右插**零重排**）、`TaskColumn`（会话流 + 活动 chips + 操作请求卡统一渲染 + 交付审查）、`Overview`（分组 + 分页）、`CommandPalette`（执行器选择仅创建处）、`WorkbenchSurfaces`（Git/设置容器占位） |
| `workbench/workbenchGate.ts` | feature gate：默认新工作台；`sessionStorage['halo:workbench-view']='legacy'` 回退旧视图 |
| `app/layout/AppLayout.tsx` | 条件挂载（`isHaloLocalCodingScope() && isStripWorkbenchEnabled()`），lazy import |
| `scripts/check-style-tokens.mjs` | 禁裸值检查（颜色/间距/圆角必须走 token；存量 342 文件豁免、只减不增） |

### 治理与守卫

- `scripts/check-repo-hygiene.mjs`：**冻结路径守卫**——`product/Halo Studio/vendor/`、`halo-scope.json`、`MiniApp/`、`BitFun-latest/` 出现 diff/未跟踪文件即红（ADR-0079）
- `scripts/core-boundaries/`：公共 API 审计清单制（新 pub 符号必须在 `public-api-rules.mjs` 登记）、依赖分层（apps 允许 apps 组合、vendor/installer 不入分层）、禁入规则（adapter 契约测试目录已入 allowPaths）
- 迁移基线 tag `migration-baseline-20260905`：行为等价对照物，**不可重写/删除**

## 3. 怎么验证（agent 每次改动的标准回路）

```powershell
# Rust（在 product/Halo Studio 下）
cargo test -p halo-runtime-ports -p halo-agent-runtime -p halo-services-core -p halo-pi-rpc-adapter -p halo-dsh-adapter
node scripts/check-core-boundaries.mjs      # 公共 API/分层；新 pub 符号先登记
node scripts/check-repo-hygiene.mjs         # 冻结守卫 + 卫生

# 前端（在 product/Halo Studio/src/web-ui 下）
npm run test:run        # 2433 测试
npm run type-check
npm run lint            # 0 错（2 条存量警告属既有文件）；内含 check-style-tokens

# 新工作台预览
npm run dev             # http://localhost:1422（mock driver 驱动；无 sessionStorage legacy 键即新视图）
```

当前基线数字：Rust ≥826（M1 时点四 crate）+ dsh 26 + pi 扩展后 695（三 crate）；前端 2433。**任何回落都是回归。**

## 4. 不能动的事（agent 红线）

1. **冻结路径**：`vendor/`、`halo-scope.json`、`MiniApp/`、`BitFun-latest/`——守卫会红；删除它们是 M6（#58）的专属动作。
2. **迁移基线 tag** 不可重写；历史证据文件（`docs/verification/`）不回改。
3. **基线契约测试不回退**：pi_configuration_contract 11、pi_rpc_contract 51、managed_executor_contracts 12（pi）+ 15（dsh）、workbench_runtime_contracts 58。
4. **公共 API 纪律**：crates 新增 pub 符号必须同步 `public-api-rules.mjs` 允许清单（带 owner/consumer/verification 元数据）——否则 check-core-boundaries 红。
5. **样式禁裸值**：新样式必须走 token；存量 .scss 只减不增（M6 前清零）。
6. **词汇表权威**：`CONTEXT.md` 的「主执行器」「受管执行器」「执行器交接」已按双 Adapter 决议改写——新术语走 `/domain-modeling` 流程，实现细节不进 CONTEXT.md。
7. **一次性决议不可放宽**（ADR-0012）；两执行器决议统一渲染为同一「Agent 操作请求」卡。
8. **niri GPL-3.0**：交互范式可借鉴，源码零复制；DMS MIT 可移植但保留归属（ADR-0052）。

## 5. 还没做的事（按优先级）

1. **M6（#58，人工门槛）**：真实双 Adapter 受管任务主链验收（创建→决议→交付审查→接受/拒绝全程新 UI）通过后 → 整体删除 `vendor/`、`halo-scope.json`、`MiniApp/`、`BitFun-latest/` 兜底 → sass 依赖清零 → 守卫收敛为防回归断言。**放行前不得动工。**
2. **UI 视觉重设计**：用户将用 Gemini Studio 重做视觉。交接要点：**只改 `tokens/` 与组件 CSS Modules，不碰 `.tsx` 逻辑与 `workbench/` 结构**；2433 测试 + lint 是回归网。
3. **M5 未尽项**（票 #57 报告）：Git 面板/设置真实内容（容器占位）、会话内命令执行链（ADR-0030）、真实 Tauri driver 替换 mock（`workbenchRuntimeStoreDriver.ts` 是接缝）、真实 dsh 二进制验收（`$/cancelRequest` 参数形状待校正）、条带虚拟化（规格显式延后）。
4. **P1 提取点**（#42 决议）：运行中模型/思考级切换、`fork`/`new_session(parentSession)` 归因链、compaction、session_stats/export（须过脱敏）、图片附件。
5. **已知存量问题**：Halo-Installer 依赖解析脱离产品 vendor 流（依赖分层检查已跳过它，构建流待 M6 一并整理）；web-ui 2 条存量 lint 警告（`pi-configuration/client.ts`、`infrastructure/workbench-runtime/store.ts`）。

## 6. 给后续 agent 的工作方式建议

- **先读研究文档再读代码**——它们是带出处的主源摘要，能省掉大量探索 token（本次一个探索型 agent 烧了 21M token 的教训）。
- 单 agent 串行 + 明确工具调用预算；大范围并行会撞用户配额上限。
- 实现切片的分工模板见 #53–#57 票体（必读清单/硬约束/验收/「不要 commit」交主会话验收模式）。
- 决策类问题走决策地图模式（票 + 决议评论 + 地图索引），硬难逆转的落 ADR。
