# Halo Studio Pi 与 OpenCode 核心重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 在 Windows 上交付一个可启动、可测试的全新 Electron 核心，只管理 Pi 与 OpenCode，并建立后续编辑器、会话与配置中心所需的安全边界。

**架构：** 根目录改为 npm workspaces；Renderer 仅通过按业务域划分的 Preload API 调用 Electron Main。Main 独占 Workspace、Trust、SQLite、凭据、配置写入和受管进程；Pi 通过 JSONL RPC，OpenCode 通过锁定的 `opencode-ai@1.18.4` 本地 Server、Basic Auth、健康检查与 SSE 接入。

**技术栈：** Electron 33、React 18、Vite 6、TypeScript 5.7、Vitest、Testing Library、Zod、better-sqlite3、jsonc-parser、diff、write-file-atomic、Lucide React。

---

## 文件结构与职责

```text
apps/desktop/
  src/main/          Electron 启动、服务组合、窗口和 IPC 注册
  src/preload/       最小 contextBridge API，不暴露通用 invoke
  src/renderer/      VS Code 风格双域外壳和瞬时视图状态
  tests/             Main/Preload/Renderer 集成测试
packages/contracts/  Zod Schema、共享类型、IPC 与事件契约
packages/core/       Workspace、真实路径、Trust、环境白名单、日志脱敏
packages/storage/    SQLite migration、仓储接口、凭据保险库
packages/config/     目标注册、JSONC patch、Diff、原子写入和安全回滚
packages/agent-pi/   Pi 探测、JSONL transport、readiness 与生命周期
packages/agent-opencode/ OpenCode 受管运行时、认证、健康与最小 SSE
packages/ui/         工作台布局、主题 token 和可复用基础组件
scripts/             仓库卫生与发布前检查
docs/architecture/   当前有效架构说明
```

`packages/editor` 在第二子项目引入；第一子项目不安装 Monaco，也不创建假的编辑器 API。调试终端和 `/` 命令只保留契约，不实现 PTY 或命令执行。

### Task 1: 清退旧实现并建立仓库卫生基线

**Files:**
- Create: `.gitattributes`
- Create: `scripts/assert-repository.mjs`
- Rewrite: `.gitignore`
- Rewrite: `package.json`
- Rewrite: `README.md`
- Delete: `Cargo.toml`, `Cargo.lock`, `crates/`, `apps/desktop/halo_desktop/`, `apps/desktop/tests/`, `apps/desktop/pyproject.toml`, `apps/desktop/requirements.txt`
- Delete: `plugins/`, `src/`, `index.html`, `postcss.config.js`, `tailwind.config.ts`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`, `vitest.config.ts`
- Delete: `docs/2026-07-22-mission-request-consolidated.md`, `docs/2026-07-22-project-handoff-summary.md`, `docs/architecture/2026-07-22-native-agent-workspace-rebuild.md`
- Delete: `docs/superpowers/specs/2026-07-21-halo-studio-design.md` and every plan dated before this plan

- [ ] **Step 1: 写仓库卫生检查并确认它先失败**

创建 `scripts/assert-repository.mjs`，检查 Git 跟踪文件中没有参考目录、旧运行时目录和旧品牌清单：

```js
import { execFileSync } from "node:child_process";

const tracked = execFileSync("git", ["ls-files"], { encoding: "utf8" })
  .split(/\r?\n/u)
  .filter(Boolean);
const forbiddenPrefixes = [
  "用于参考的几个项目的代码/",
  "crates/",
  "plugins/agents/",
  "apps/desktop/halo_desktop/",
];
const violations = tracked.filter((file) =>
  forbiddenPrefixes.some((prefix) => file.startsWith(prefix)),
);
if (violations.length > 0) {
  console.error(`禁止跟踪的文件:\n${violations.join("\n")}`);
  process.exit(1);
}
```

Run: `node scripts/assert-repository.mjs`

Expected: FAIL，并列出 `crates/`、`plugins/agents/` 或 Python/QML 文件。

- [ ] **Step 2: 固定忽略规则与行尾**

`.gitignore` 至少包含：

```gitignore
用于参考的几个项目的代码/
.superpowers/
.worktrees/
node_modules/
dist/
out/
coverage/
target/
.venv/
*.db
*.db-shm
*.db-wal
*.sqlite
*.sqlite3
*.log
.halo-runtime/
.halo-user-data/
runtime-cache/
config-backups/
.env
.env.*
!.env.example
```

`.gitattributes` 固定文本为 LF，Windows 脚本例外：

```gitattributes
* text=auto eol=lf
*.bat text eol=crlf
*.cmd text eol=crlf
*.ps1 text eol=crlf
*.png binary
*.jpg binary
*.ico binary
*.woff2 binary
```

- [ ] **Step 3: 删除旧实现和失效文档**

使用 Git 感知的批量删除清退上方 `Delete` 清单；不创建 `legacy/`，不移动旧实现。保留并只读使用 `docs/superpowers/specs/2026-07-22-pi-opencode-core-rebuild-design.md`。

Run: `git ls-files "crates/**" "plugins/**" "src/**" "apps/desktop/halo_desktop/**"`

Expected: 无输出。

- [ ] **Step 4: 建立空的 npm workspace 根**

根 `package.json` 使用明确脚本：

```json
{
  "name": "halo-studio",
  "version": "0.2.0",
  "private": true,
  "type": "module",
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "dev": "npm run dev --workspace @halo-studio/desktop",
    "build": "npm run build --workspaces --if-present",
    "typecheck": "npm run typecheck --workspaces --if-present",
    "test": "npm run test --workspaces --if-present",
    "check:repository": "node scripts/assert-repository.mjs",
    "verify": "npm run check:repository && npm run typecheck && npm test && npm run build"
  },
  "engines": { "node": ">=20.18.0", "npm": ">=10.8.0" }
}
```

README 只陈述当前已授权范围、参考目录只读边界、`npm ci`/`npm test`/`npm run build` 命令以及“完整编辑、聊天、命令、终端、MCP 与同步尚未交付”，不使用旧产品截图或四 Agent 文案。

- [ ] **Step 5: 运行卫生检查并提交**

Run: `node scripts/assert-repository.mjs`

Expected: PASS，退出码 0。

Run: `git status --short --ignored | Select-String '用于参考的几个项目的代码'`

Expected: 参考目录只显示为 `!!` 或在该 worktree 中不存在，不能显示为 `??`。

```bash
git add -A
git commit -m "重构: 清退旧四 Agent 实现并建立仓库卫生基线"
```

### Task 2: 建立共享契约与能力模型

**Files:**
- Create: `packages/contracts/package.json`
- Create: `packages/contracts/tsconfig.json`
- Create: `packages/contracts/src/agent.ts`
- Create: `packages/contracts/src/error.ts`
- Create: `packages/contracts/src/events.ts`
- Create: `packages/contracts/src/ipc.ts`
- Create: `packages/contracts/src/commands.ts`
- Create: `packages/contracts/src/index.ts`
- Test: `packages/contracts/src/contracts.test.ts`

- [ ] **Step 1: 写失败的契约测试**

测试只接受 `pi | opencode`，能力不是布尔值，IPC 错误可序列化：

```ts
import { describe, expect, it } from "vitest";
import { agentKindSchema, capabilitySchema, ipcEnvelopeSchema } from "./index.js";

describe("共享契约", () => {
  it("拒绝已弃用 Agent", () => {
    expect(agentKindSchema.safeParse("pi").success).toBe(true);
    expect(agentKindSchema.safeParse("opencode").success).toBe(true);
    expect(agentKindSchema.safeParse("codex").success).toBe(false);
    expect(agentKindSchema.safeParse("claude-code").success).toBe(false);
  });

  it("能力声明包含通道和重启语义", () => {
    expect(capabilitySchema.parse({ supported: true, channel: "rpc", restartRequired: false }))
      .toEqual({ supported: true, channel: "rpc", restartRequired: false });
  });

  it("IPC 响应使用可判别包络", () => {
    expect(ipcEnvelopeSchema.safeParse({ ok: false, error: {
      code: "WorkspaceUntrusted", message: "项目尚未信任", retryable: false,
    }}).success).toBe(true);
  });
});
```

Run: `npm test --workspace @halo-studio/contracts`

Expected: FAIL，契约模块尚不存在。

- [ ] **Step 2: 实现 Agent、能力、运行时和命令契约**

`agent.ts` 必须定义 `AgentKind`、十二项能力键、`CapabilityDescriptor`、`RuntimeBinding`；通道限定为 `rpc | http | sse | cli | native | unavailable`。`commands.ts` 定义：

```ts
export interface CommandDescriptor {
  readonly name: string;
  readonly argumentHint?: string;
  readonly agentKind: AgentKind;
  readonly source: "native" | "extension" | "tui";
  readonly channel: CapabilityChannel;
  readonly allowedWhileRunning: boolean;
  readonly mutatesGlobalDefaults: boolean;
  readonly tuiOnly: boolean;
}
```

不添加通用 `AgentId` 字符串或第三个品牌枚举。

- [ ] **Step 3: 实现统一错误与事件外壳**

`AppErrorCode` 精确包含：`RuntimeUnavailable`、`VersionMismatch`、`AuthenticationFailed`、`PermissionRequired`、`WorkspaceUntrusted`、`TransportDisconnected`、`ProtocolViolation`、`ConfigConflict`、`UnsafePath`、`MigrationFailed`。`AgentEventEnvelope` 包含 `eventId/workspaceId/agentKind/sessionId?/sequence/timestamp/payload`，payload 用可判别 union 保留 `pi` 与 `opencode` 原生语义，未知 OpenCode 事件映射为采样日志而不是异常。

- [ ] **Step 4: 实现按域 IPC 契约注册表**

只定义 `workspace.pick/open/snapshot/trust`、`runtime.probe/start/stop/snapshot`、`config.preview/commit/rollback`、`storage.health`。每个 channel 同时具有 request、业务 data 和最终 response Zod Schema；response 统一为成功/失败包络：

```ts
export const ipcContracts = {
  "workspace.pick": {
    request: z.object({}),
    data: workspaceCandidateSchema.nullable(),
    response: ipcEnvelope(workspaceCandidateSchema.nullable()),
  },
  "runtime.snapshot": {
    request: z.object({ workspaceId: z.string().uuid().optional() }),
    data: z.array(runtimeBindingSchema),
    response: ipcEnvelope(z.array(runtimeBindingSchema)),
  },
} as const;
```

本阶段不定义 `shell.exec`、任意路径读写、SQL 查询、`terminal.write` 或会话 prompt 通道。

- [ ] **Step 5: 测试、类型检查和提交**

Run: `npm test --workspace @halo-studio/contracts`

Expected: PASS。

Run: `npm run typecheck --workspace @halo-studio/contracts`

Expected: PASS。

```bash
git add packages/contracts package.json package-lock.json
git commit -m "功能: 建立 Pi 与 OpenCode 共享契约和能力模型"
```

### Task 3: 实现 Workspace、真实路径、Trust 与进程环境边界

**Files:**
- Create: `packages/core/package.json`
- Create: `packages/core/tsconfig.json`
- Create: `packages/core/src/workspace.ts`
- Create: `packages/core/src/pathPolicy.ts`
- Create: `packages/core/src/trust.ts`
- Create: `packages/core/src/environment.ts`
- Create: `packages/core/src/redaction.ts`
- Create: `packages/core/src/index.ts`
- Test: `packages/core/src/workspace.test.ts`
- Test: `packages/core/src/environment.test.ts`

- [ ] **Step 1: 写路径与 Trust 失败测试**

使用临时目录创建真实目录和 junction/symlink，验证 `openWorkspace()` 保存绝对路径与 `realPath`，不存在路径失败，最近祖先的 Trust 决策生效；Windows 无创建链接权限时测试应显式 skip，不得伪造通过。

```ts
it("以最近祖先决策解析信任", async () => {
  const store = new MemoryTrustStore([
    { realPath: root, state: "trusted", decidedAt: "2026-07-22T00:00:00.000Z" },
    { realPath: child, state: "untrusted", decidedAt: "2026-07-22T00:01:00.000Z" },
  ]);
  expect(await resolveTrust(join(child, "project"), store)).toBe("untrusted");
});
```

Run: `npm test --workspace @halo-studio/core -- workspace.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现规范路径和 Workspace 服务**

`PathPolicy` 使用 `resolve`、`realpath`、`stat`，Windows 比较时统一大小写但保留展示路径。Workspace ID 由规范真实路径的 SHA-256 派生，避免把本机绝对路径暴露为业务 ID。`openWorkspace` 返回 `{id, rootPath, realPath, trustState}`，没有默认 `D:\Halo Studio`。

- [ ] **Step 3: 实现 Trust Store 接口和未信任启动策略**

```ts
export interface TrustStore {
  listDecisions(): Promise<readonly TrustDecision[]>;
  setDecision(realPath: string, state: "trusted" | "untrusted"): Promise<void>;
}

export function runtimeTrustPolicy(kind: AgentKind, state: TrustState) {
  if (kind === "pi") return state === "trusted"
    ? { args: ["--approve"], env: {}, loadProjectResources: true }
    : { args: ["--no-approve", "--no-context-files"], env: {}, loadProjectResources: false };
  return state === "trusted"
    ? { args: [], env: {}, loadProjectResources: true }
    : { args: [], env: { OPENCODE_DISABLE_PROJECT_CONFIG: "1" }, loadProjectResources: false };
}
```

Trust 文案不得称为系统沙箱。

- [ ] **Step 4: 测试并实现环境白名单和日志脱敏**

白名单只复制 PATH、HOME/USERPROFILE、TEMP/TMP、Locale、显式代理变量和 Profile 授权的 provider 变量；拒绝无条件展开 `process.env`。日志脱敏递归屏蔽键名包含 `authorization|api.?key|token|secret|password|cookie` 的值，并限制字符串长度。

Run: `npm test --workspace @halo-studio/core`

Expected: PASS，包含“未授权宿主变量不会进入子进程”和“敏感值不会进入日志”。

- [ ] **Step 5: 提交**

```bash
git add packages/core package.json package-lock.json
git commit -m "功能: 建立 Workspace 信任和运行时环境边界"
```

### Task 4: 实现 SQLite migration 与 CredentialVault

**Files:**
- Create: `packages/storage/package.json`
- Create: `packages/storage/tsconfig.json`
- Create: `packages/storage/src/database.ts`
- Create: `packages/storage/src/migrations.ts`
- Create: `packages/storage/src/repositories.ts`
- Create: `packages/storage/src/credentialVault.ts`
- Create: `packages/storage/src/index.ts`
- Test: `packages/storage/src/database.test.ts`
- Test: `packages/storage/src/credentialVault.test.ts`

- [ ] **Step 1: 写 migration 失败测试**

临时 SQLite 必须只创建 `schema_migrations/workspaces/runtime_bindings/profiles/credential_refs/config_backups/audit_events`，重复启动幂等，迁移失败进入只读恢复状态：

```ts
expect(database.health()).toEqual({ mode: "read-write", schemaVersion: 1 });
expect(tableNames(database.raw())).toEqual([
  "audit_events", "config_backups", "credential_refs", "profiles",
  "runtime_bindings", "schema_migrations", "workspaces",
]);
```

Run: `npm test --workspace @halo-studio/storage -- database.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现只前进 migration 与仓储边界**

使用 `better-sqlite3` 事务执行编号 migration。数据库业务接口不导出原始 SQL 句柄；测试专用检查函数从测试模块导入。迁移异常包装为 `MigrationFailed`，重新以只读模式打开，并提供 `health()` 和诊断导出数据，不自动降级或删表。

- [ ] **Step 3: 写 CredentialVault 失败关闭测试**

```ts
it("系统保护不可用时不写明文", async () => {
  const vault = new FileCredentialVault(tempDir, unavailableProtector);
  await expect(vault.store("provider:key", "plaintext")).rejects.toMatchObject({
    code: "AuthenticationFailed",
  });
  expect(await readdir(tempDir)).toHaveLength(0);
});
```

并测试 `store/get/delete/isAvailable`、文件只包含密文、SQLite 只保存引用 ID。

- [ ] **Step 4: 实现凭据保险库**

```ts
export interface SecretProtector {
  isAvailable(): boolean;
  protect(value: Buffer): Buffer;
  unprotect(value: Buffer): Buffer;
}

export interface CredentialVault {
  store(reference: string, value: string): Promise<void>;
  get(reference: string): Promise<string | null>;
  delete(reference: string): Promise<void>;
  isAvailable(): boolean;
}
```

使用同目录临时文件、`0600` 权限和原子替换保存密文；保护器不可用时所有写入失败。生产保护器在 Electron Main 中适配 `safeStorage`，Renderer 永远不接触解密值。

- [ ] **Step 5: 测试并提交**

Run: `npm test --workspace @halo-studio/storage`

Expected: PASS。

```bash
git add packages/storage package.json package-lock.json
git commit -m "功能: 建立 SQLite 迁移和凭据保险库"
```

### Task 5: 实现安全配置预览、原子写入与回滚

**Files:**
- Create: `packages/config/package.json`
- Create: `packages/config/tsconfig.json`
- Create: `packages/config/src/targetRegistry.ts`
- Create: `packages/config/src/fingerprint.ts`
- Create: `packages/config/src/jsoncPatch.ts`
- Create: `packages/config/src/unifiedDiff.ts`
- Create: `packages/config/src/atomicWrite.ts`
- Create: `packages/config/src/configTransaction.ts`
- Create: `packages/config/src/index.ts`
- Test: `packages/config/src/configTransaction.test.ts`
- Test: `packages/config/src/pathPolicy.test.ts`

- [ ] **Step 1: 写真实 Diff 和 JSONC 保留测试**

测试 `createTwoFilesPatch` 输出含 `---/+++/@@`，JSONC patch 保留注释、未知字段和原排版；预览结果屏蔽 `apiKey/token/secret/password/authorization` 值。

Run: `npm test --workspace @halo-studio/config -- configTransaction.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现目标注册与真实路径守卫**

Renderer 只能提交 Main 颁发的 `targetId`。注册表记录 `scope/owner/path/format/source/writable/allowedRoot`；目标与父目录都通过 `realpath` 比较，拒绝 `..`、大小写绕过、symlink/junction 逃逸和未声明根。不存在目标时先解析最近存在祖先，再在写入前重新验证，防止替换竞态。

- [ ] **Step 3: 实现预览事务**

```ts
export interface ConfigPreview {
  previewId: string;
  targetId: string;
  fingerprint: string;
  unifiedDiff: string;
  restartRequired: readonly AgentKind[];
}
```

`preview()` 读取原文与 SHA-256，应用 `jsonc-parser.modify/applyEdits` 的最小 patch，验证可重新解析，生成脱敏 unified diff，并把原文、新文、指纹、目标和过期时间只保存在 Main 内存。Pi 配置变更标记 `pi` 需重启；不能创建 Pi MCP 目标。

- [ ] **Step 4: 实现 commit 与 rollback**

`commit(previewId)` 重新读取并比较指纹；冲突返回 `ConfigConflict`。旧原文先通过 `EncryptedBackupStore` 写入 CredentialVault，随后同目录写临时文件、刷盘、原子替换并重新解析。验证失败时从加密备份经过同一路径守卫和原子替换回滚。审计仅保存脱敏摘要、指纹和 backup reference，不保存凭据或完整配置。

- [ ] **Step 5: 覆盖冲突与回滚测试并提交**

测试外部修改冲突、替换前链接逃逸、解析失败自动回滚、回滚目标再次越界、备份未出现明文。Windows 使用含空格和 CJK 的临时路径。

Run: `npm test --workspace @halo-studio/config`

Expected: PASS。

```bash
git add packages/config package.json package-lock.json
git commit -m "功能: 建立配置 Diff 原子写入和安全回滚"
```

### Task 6: 实现 Pi JSONL transport 与运行时生命周期

**Files:**
- Create: `packages/agent-pi/package.json`
- Create: `packages/agent-pi/tsconfig.json`
- Create: `packages/agent-pi/src/schemas.ts`
- Create: `packages/agent-pi/src/jsonlTransport.ts`
- Create: `packages/agent-pi/src/detect.ts`
- Create: `packages/agent-pi/src/runtime.ts`
- Create: `packages/agent-pi/src/index.ts`
- Test: `packages/agent-pi/src/jsonlTransport.test.ts`
- Test: `packages/agent-pi/src/runtime.test.ts`

- [ ] **Step 1: 写 JSONL 契约失败测试**

覆盖 UTF-8 分片、LF/CRLF、包含 `U+2028/U+2029` 的 JSON 字符串、乱序 response、无效 JSON、stderr 噪声、超时、EOF 和异常退出。response 只按唯一 `id` 关联，事件不得抢占待处理请求。

Run: `npm test --workspace @halo-studio/agent-pi -- jsonlTransport.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现 transport**

使用 `StringDecoder("utf8")` 和仅按 `\n` 分帧的缓冲区；尾部 `\r` 单独去除。每条出站命令先经 Zod 校验。普通命令由串行队列执行，`abort/steer` 使用独立并发通道；关闭时拒绝全部 pending promise 为 `TransportDisconnected`。

- [ ] **Step 3: 写探测与 readiness 失败测试**

注入 `ProcessPort`，验证先探测 `pi`/`pi.exe --version`，不兼容时返回受管安装接口状态；启动参数始终包含 `--mode rpc`、明确 cwd、会话/模型/thinking 和 Trust 参数。readiness 必须发送带超时 `get_state`，不能等待不存在的 ready 事件。

- [ ] **Step 4: 实现生命周期与状态机**

状态限定 `unavailable/detected/starting/ready/stopping/stopped/crashed`。`prompt success` 不代表完成，普通 agent run 以 `agent_start` 开始、`agent_settled` 结束；`agent_end willRetry` 保持运行。停止先关闭 stdin 并等待，超时后终止进程。进程环境来自 core 白名单，不复制完整 `process.env`。

- [ ] **Step 5: 测试并提交**

Run: `npm test --workspace @halo-studio/agent-pi`

Expected: PASS。

```bash
git add packages/agent-pi package.json package-lock.json
git commit -m "功能: 接入 Pi JSONL RPC 探测和生命周期"
```

### Task 7: 实现锁定版本 OpenCode 受管 Server

**Files:**
- Create: `packages/agent-opencode/package.json`
- Create: `packages/agent-opencode/tsconfig.json`
- Create: `packages/agent-opencode/src/artifact.ts`
- Create: `packages/agent-opencode/src/auth.ts`
- Create: `packages/agent-opencode/src/health.ts`
- Create: `packages/agent-opencode/src/sse.ts`
- Create: `packages/agent-opencode/src/runtime.ts`
- Create: `packages/agent-opencode/src/index.ts`
- Test: `packages/agent-opencode/src/health.test.ts`
- Test: `packages/agent-opencode/src/sse.test.ts`
- Test: `packages/agent-opencode/src/runtime.test.ts`

- [ ] **Step 1: 锁定官方运行时并写生命周期失败测试**

依赖精确写为 `"opencode-ai": "1.18.4"`，禁止 `^`/`~`。测试 resolver 只接受包内 `bin/opencode.exe` 或对应平台可执行文件，拒绝 PATH 随机版本。覆盖启动悬挂、端口冲突有限重试、ready 后崩溃和停止超时强杀。

Run: `npm test --workspace @halo-studio/agent-opencode -- runtime.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现 loopback、随机 Basic Auth 和版本握手**

启动参数固定 `serve --hostname 127.0.0.1 --port <port>`，用户名 `opencode`，密码由 `randomBytes(32)` 生成，只保留在 Main 进程内。环境注入 `OPENCODE_SERVER_USERNAME/PASSWORD` 和信任策略；XDG/Profile 变量在启动前确定。

`GET /global/health` 每次带 Basic Auth，只有 HTTP 200 且 JSON `version === "1.18.4"` 才进入 ready；401 映射 `AuthenticationFailed`，版本不同映射 `VersionMismatch`，500 继续在总超时内重试。

- [ ] **Step 3: 实现最小 SSE 连接**

解析 `event:`/`data:` 帧，只发布 connected、heartbeat 和断开状态。未知合法事件调用采样日志并忽略；坏 JSON 标记 `ProtocolViolation` 但不让全局进程崩溃。SSE 不实现重放，不伪造 `Last-Event-ID`。

- [ ] **Step 4: 完成生命周期状态机**

状态限定 `unavailable/installed/starting/healthy/stopping/stopped/crashed`。端口冲突最多重试 3 次；停止先请求子进程优雅退出，6 秒后强制终止。意外退出发布断开状态，不自动重放 prompt 或无限重启。

- [ ] **Step 5: 测试并提交**

Run: `npm test --workspace @halo-studio/agent-opencode`

Expected: PASS，覆盖 health 401/500/200、版本不匹配、connected/heartbeat/未知事件和停止策略。

```bash
git add packages/agent-opencode package.json package-lock.json
git commit -m "功能: 接入 OpenCode 受管 Server 和健康握手"
```

### Task 8: 建立 Electron Main、Preload 与类型化 IPC

**Files:**
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/src/main/main.ts`
- Create: `apps/desktop/src/main/window.ts`
- Create: `apps/desktop/src/main/services.ts`
- Create: `apps/desktop/src/main/ipc/registerIpc.ts`
- Create: `apps/desktop/src/main/electronSecretProtector.ts`
- Create: `apps/desktop/src/preload/preload.ts`
- Create: `apps/desktop/src/preload/global.d.ts`
- Test: `apps/desktop/tests/ipc.test.ts`
- Test: `apps/desktop/tests/security.test.ts`

- [ ] **Step 1: 写 IPC Schema 与安全窗口失败测试**

测试每个注册 handler 在调用服务前解析 request、返回前解析 response；非法请求变成 `ProtocolViolation` 包络。窗口必须 `contextIsolation: true`、`nodeIntegration: false`、`sandbox: true`，且 navigation/new-window 默认拒绝。

Run: `npm test --workspace @halo-studio/desktop -- ipc.test.ts security.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现服务组合根**

Main 在 `app.getPath("userData")` 下创建 SQLite、密文凭据和运行时目录；以 Electron `safeStorage` 实现 `SecretProtector`。服务组合顺序为 storage -> workspace/trust -> config -> runtime。OpenCode Basic Auth 密码不进入 SQLite、日志、IPC 或 Renderer。

- [ ] **Step 3: 实现类型化 IPC 注册器**

```ts
export function registerHandler<K extends keyof IpcContractMap>(
  channel: K,
  handler: (input: InputOf<K>) => Promise<DataOf<K>>,
): void {
  ipcMain.handle(channel, async (_event, raw) => {
    try {
      const input = ipcContracts[channel].request.parse(raw);
      const data = ipcContracts[channel].data.parse(await handler(input));
      return ipcContracts[channel].response.parse({ ok: true, data });
    } catch (error) {
      return ipcContracts[channel].response.parse({ ok: false, error: toPublicError(error) });
    }
  });
}
```

服务错误统一映射为 `IpcEnvelope`，不把 stack、绝对凭据路径或 secret 返回 Renderer。

- [ ] **Step 4: 实现按业务域 Preload API**

`window.halo.workspace.pick/open/snapshot/setTrust`、`runtime.probe/start/stop/snapshot`、`config.preview/commit/rollback`、`storage.health` 分别调用固定 channel。禁止暴露 `ipcRenderer`、通用 `invoke`、`fs`、`child_process`、数据库或 Shell。

- [ ] **Step 5: 测试并提交**

Run: `npm test --workspace @halo-studio/desktop`

Expected: PASS。

```bash
git add apps/desktop package.json package-lock.json
git commit -m "功能: 建立 Electron 安全启动和类型化 IPC"
```

### Task 9: 构建 VS Code 风格双域工作台外壳

**Files:**
- Create: `packages/ui/package.json`
- Create: `packages/ui/tsconfig.json`
- Create: `packages/ui/src/tokens.css`
- Create: `packages/ui/src/WorkbenchLayout.tsx`
- Create: `packages/ui/src/index.ts`
- Create: `apps/desktop/src/renderer/main.tsx`
- Create: `apps/desktop/src/renderer/App.tsx`
- Create: `apps/desktop/src/renderer/app.css`
- Create: `apps/desktop/src/renderer/useWorkspace.ts`
- Create: `apps/desktop/src/renderer/useRuntimeStatus.ts`
- Create: `apps/desktop/src/renderer/components/TitleBar.tsx`
- Create: `apps/desktop/src/renderer/components/ActivityBar.tsx`
- Create: `apps/desktop/src/renderer/components/SideBar.tsx`
- Create: `apps/desktop/src/renderer/components/EditorSurface.tsx`
- Create: `apps/desktop/src/renderer/components/AgentPanel.tsx`
- Create: `apps/desktop/src/renderer/components/BottomPanel.tsx`
- Create: `apps/desktop/src/renderer/components/StatusBar.tsx`
- Create: `apps/desktop/src/renderer/components/TrustBanner.tsx`
- Test: `apps/desktop/src/renderer/App.test.tsx`

- [ ] **Step 1: 写外壳失败测试**

测试首屏存在标题栏/命令中心、Activity Bar、Side Bar、中央表面、Agent 辅助栏、底部 Panel 和 Status Bar；开发域与配置域共享同一个 Workspace；Pi/OpenCode 状态来自 IPC，不使用静态“在线”数据；未信任时显示就地 Trust banner。

Run: `npm test --workspace @halo-studio/desktop -- App.test.tsx`

Expected: FAIL。

- [ ] **Step 2: 实现稳定工作台网格与主题 token**

布局固定为 `48px minmax(180px, 260px) minmax(320px, 1fr) minmax(260px, 360px)`，底部 Panel 和 22px Status Bar 独立行；窄屏折叠 Side Bar/Auxiliary Bar，但按钮和文本不重叠。颜色采用中性深灰、蓝色焦点、绿色健康、橙色警告和红色错误，不使用紫蓝渐变、星空、光球或营销式卡片。

- [ ] **Step 3: 实现双域导航与真实状态**

Activity Bar 使用 Lucide 图标和 tooltip：资源、搜索、Agent、配置、历史、设置。中央开发域显示 Workspace 与编辑器占位表面；配置域显示 Pi 资源/OpenCode MCP 的导航入口但标明当前阶段不可编辑，不伪造数据。Agent Panel 同时展示两个运行时的实际 `unavailable/detected/ready/healthy/crashed` 状态。

- [ ] **Step 4: 实现 Workspace 选择与信任交互**

“打开文件夹”只能调用 `workspace.pick` 后由 Main 打开；不在 Renderer 保存第二份路径。未信任时不自动启动 runtime，点击“信任并启动”调用 `setTrust` 后刷新状态。界面文案明确“信任允许加载项目配置，不等于系统沙箱”。

- [ ] **Step 5: 测试、构建并提交**

Run: `npm test --workspace @halo-studio/desktop`

Expected: PASS。

Run: `npm run build --workspace @halo-studio/desktop`

Expected: PASS，生成 Main、Preload 和 Renderer 产物。

```bash
git add packages/ui apps/desktop package.json package-lock.json
git commit -m "功能: 构建 VS Code 风格双域工作台外壳"
```

### Task 10: 增加核心集成测试与 Windows 路径烟测

**Files:**
- Create: `apps/desktop/tests/workspace-runtime.integration.test.ts`
- Create: `apps/desktop/tests/credential-boundary.integration.test.ts`
- Create: `apps/desktop/tests/fixtures/fake-pi.mjs`
- Create: `apps/desktop/tests/fixtures/fake-opencode.mjs`
- Create: `scripts/windows-smoke.mjs`

- [ ] **Step 1: 写端到端服务组合失败测试**

在含空格和 CJK 的临时 Workspace/XDG/userData 中组合真实服务与 fake runtimes，验证未信任不会加载项目配置，信任后 Pi 完成 `get_state` readiness、OpenCode 完成认证/健康/版本握手和关闭。

Run: `npm test --workspace @halo-studio/desktop -- workspace-runtime.integration.test.ts`

Expected: FAIL。

- [ ] **Step 2: 实现可控 fake Pi**

fake 进程只实现 `--version` 与 `--mode rpc`，按输入 id 乱序回复并发 `get_state`，可通过测试环境切换无效 JSON、stderr、EOF、`agent_end willRetry` 和 `agent_settled`。生产代码不得自动发现或回退到该 fake。

- [ ] **Step 3: 实现可控 fake OpenCode**

fake 进程只绑定 `127.0.0.1`，强制 Basic Auth，提供 `/global/health` 和 `/global/event`，可切换 401/500/版本错误/heartbeat/意外退出。生产包不包含测试 fixture。

- [ ] **Step 4: 验证凭据和日志边界**

把唯一 canary secret 写入 CredentialVault 并启动两个 runtime，断言 SQLite、Renderer IPC 记录、日志、Diff、审计和备份元数据均不包含 canary；只有保险库密文文件存在且也不包含明文。

- [ ] **Step 5: 运行 Windows 烟测并提交**

Run: `node scripts/windows-smoke.mjs`

Expected: PASS，输出 Pi readiness、OpenCode health、优雅停止和临时目录清理结果。

```bash
git add apps/desktop/tests scripts/windows-smoke.mjs
git commit -m "测试: 覆盖双运行时和凭据边界集成场景"
```

### Task 11: 重写活动文档并记录第三方边界

**Files:**
- Rewrite: `README.md`
- Create: `docs/architecture/pi-opencode-core.md`
- Create: `THIRD_PARTY_NOTICES.md`
- Create: `docs/testing/core-rebuild-verification.md`

- [ ] **Step 1: 写当前架构文档**

文档说明进程边界、Workspace/Trust 顺序、Pi JSONL RPC、OpenCode 锁定 Server、SQLite 事实边界、凭据流、安全配置事务和第一阶段排除项。不得出现旧四 Agent 注册表、Pi MCP、Mock PTY、Web fallback 或宇宙主题说明。

- [ ] **Step 2: 重写 README**

README 以 Pi/OpenCode 精简工作台为唯一产品描述，给出 Node/npm 前置条件、`npm ci`、`npm run dev`、`npm test`、`npm run build`、数据目录和故障诊断。明确完整 Monaco、聊天、`/` 命令执行、调试终端、MCP/资源 UI 和同步属于后续子项目。

- [ ] **Step 3: 记录第三方许可证**

列出 OpenCode `1.18.4` MIT、Pi `0.81.1` 仅作为契约参考且未复制源代码、Lucide ISC、Electron/React/Vite/SQLite 相关依赖的许可证来源。若实际复制 VS Code/Codicons 资源，必须逐文件列出处；本阶段优先使用 Lucide，不复制 VS Code 商标。

- [ ] **Step 4: 记录可复现验证命令**

`docs/testing/core-rebuild-verification.md` 记录环境、命令、预期结果、真实 Pi/OpenCode 可选烟测方式和“无凭据测试必须使用本地 fixture”。不记录真实用户名、绝对用户路径、token 或密钥。

- [ ] **Step 5: 提交**

```bash
git add README.md docs THIRD_PARTY_NOTICES.md
git commit -m "文档: 更新 Pi 与 OpenCode 核心架构和验证指南"
```

### Task 12: 全量验收、发布前检查与 develop 推送

**Files:**
- Modify: `scripts/assert-repository.mjs`
- Modify: `package.json`
- Modify: `docs/testing/core-rebuild-verification.md`

- [ ] **Step 1: 扩展仓库卫生检查**

检查运行时代码不存在 `claude-code/codex-cli`、`MockPty`、`mcp.json` 的 Pi 目标、`D:\\Halo Studio` 硬编码、Express/WebSocket fallback 和参考目录跟踪文件。规格中用于描述排除项的文字不计为违规。

- [ ] **Step 2: 执行全量类型、测试和构建**

Run: `npm run verify`

Expected: repository check、所有 workspace typecheck、全部 Vitest 和全部 build 均 PASS。

- [ ] **Step 3: 执行依赖和产物检查**

Run: `npm audit --omit=dev`

Expected: 记录实际结果；存在 high/critical 时不得宣称可发布，必须修复或写明上游阻塞与影响。

Run: `git ls-files "用于参考的几个项目的代码/**"`

Expected: 无输出。

Run: `rg -n "claude-code|codex-cli|MockPty|D:\\\\Halo Studio" apps packages scripts README.md docs/architecture docs/testing`

Expected: 无运行时代码命中；仅测试中的拒绝断言允许出现并需逐条解释。

- [ ] **Step 4: Electron 可视化烟测**

启动本地开发服务，使用 Playwright/Electron 或应用浏览器截图验证 1440x900、1024x768 和 390x844：画面非空、工作台各区域无重叠、长路径不溢出、未信任 banner 可操作、运行时状态不是静态假数据。检查 DevTools 无未处理异常。

- [ ] **Step 5: 最终审查、提交验证记录并推送 develop**

先进行完整规格审查，再进行代码质量审查；所有问题关闭后更新验证文档中的实际结果。

```bash
git add scripts package.json package-lock.json docs/testing/core-rebuild-verification.md
git commit -m "验证: 完成核心重构全量验收"
git push -u origin develop
```

Expected: 远程新增 `develop`，`main` 保持在已确认的规格提交，未创建其他长期分支。

---

## 自查结果

- **规格覆盖：** Task 1 覆盖旧实现清退和仓库卫生；Task 2 覆盖两 Agent 能力、错误、事件和 IPC；Task 3 覆盖 Workspace/Trust/路径/环境；Task 4 覆盖 SQLite 与凭据；Task 5 覆盖 Diff/写入/备份/回滚；Task 6/7 覆盖两个真实运行时；Task 8 覆盖 Main/Preload 隔离；Task 9 覆盖双域 UI；Task 10/12 覆盖协议、安全、Windows 和可视化验收；Task 11 覆盖活动文档与许可证。
- **范围控制：** 未把完整 Monaco、聊天、工具权限流、`/` 命令执行、PTY、OpenCode MCP、Pi 资源管理或云同步提前纳入第一阶段。
- **占位符检查：** 计划中的组件名称均有明确职责、文件、测试、命令和验收结果；没有把实现责任留给未定义步骤。
- **类型一致性：** 全程统一使用 `AgentKind = "pi" | "opencode"`、`Workspace`、`RuntimeBinding`、`ConfigPreview`、`CredentialVault`、`SecretProtector`、`IpcEnvelope` 与 `AgentEventEnvelope`；Renderer 不接触路径写入、进程、数据库和凭据。
