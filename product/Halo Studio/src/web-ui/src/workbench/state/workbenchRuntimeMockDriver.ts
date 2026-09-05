/**
 * Mock runtime driver for the strip workbench (M5, issue #57).
 *
 * Fills WorkbenchRuntimeStore with an in-memory event stream shaped exactly
 * like the future Tauri event seam: the driver only emits
 * WorkbenchProjectionEvents through the host handle — it never touches store
 * state directly. Swapping in the real Halo Workbench Runtime driver
 * (ADR-0065/0075) is therefore a driver replacement, not a store rewrite.
 *
 * The seed mirrors the #43 prototype's two workspaces / task states so the
 * interaction model can be compared 1:1 against the primary source.
 */

import type {
  WorkbenchProjectionEvent,
  WorkbenchProjectionEventInput,
  WorkbenchTask,
  WorkbenchWorkspace,
} from './workbenchProjectionTypes';
import type { WorkbenchRuntimeDriver, WorkbenchRuntimeDriverHost } from './workbenchRuntimeStoreDriver';

const MOCK_FINGERPRINT = 'a'.repeat(64);
const BASE_MS = 1_760_000_000_000;

function at(offsetSeconds: number): number {
  return BASE_MS + offsetSeconds * 1000;
}

let seedCounter = 0;
function seedId(prefix: string): string {
  seedCounter += 1;
  return `${prefix}-seed-${seedCounter}`;
}

function task(input: {
  title: string;
  executor: WorkbenchTask['executor'];
  phase: WorkbenchTask['phase'];
  messages: WorkbenchTask['messages'];
  activities?: WorkbenchTask['activities'];
  pendingOperation?: WorkbenchTask['pendingOperation'];
  deliveryReview?: WorkbenchTask['deliveryReview'];
}): WorkbenchTask {
  const taskId = seedId('task');
  return {
    taskId,
    sessionId: `session-${taskId}`,
    title: input.title,
    executor: input.executor,
    mode: 'managed',
    phase: input.phase,
    messages: input.messages,
    activities: input.activities ?? [],
    pendingOperation: input.pendingOperation ?? null,
    deliveryReview: input.deliveryReview ?? null,
    error: null,
    updatedAtMs: at(0),
  };
}

function buildSeedWorkspaces(): WorkbenchWorkspace[] {
  const loginFix = task({
    title: '登录态刷新循环修复',
    executor: 'pi-rpc',
    phase: 'running',
    messages: [
      { role: 'user', content: '刷新后偶发跳回登录页，先复现再修。' },
      { role: 'assistant', content: '已定位：token 刷新与路由守卫存在竞态，准备改写守卫逻辑。' },
      { role: 'assistant', content: '守卫已改为单飞（single-flight）刷新，正在跑回归。' },
    ],
    activities: [
      { activityId: 'act-1', kind: 'tool', label: 'repo 扫描', status: 'completed', isError: false },
      { activityId: 'act-2', kind: 'tool', label: '编辑 auth/guard.ts', status: 'completed', isError: false },
      { activityId: 'act-3', kind: 'tool', label: 'vitest --watch', status: 'started', isError: false },
    ],
  });

  const e2eIsolation = task({
    title: 'e2e 测试账号隔离',
    executor: 'pi-rpc',
    phase: 'waitingDeveloper',
    messages: [
      { role: 'user', content: 'e2e 每次都互相污染数据，帮我隔离。' },
      { role: 'assistant', content: '已给出两种隔离方案（独立租户 / 事务回滚），需要你拍板方案。' },
    ],
    activities: [
      { activityId: 'act-4', kind: 'tool', label: '复现脚本', status: 'completed', isError: false },
      { activityId: 'act-5', kind: 'tool', label: 'e2e:auth', status: 'failed', isError: true },
    ],
    pendingOperation: {
      operationId: seedId('op'),
      taskId: '',
      sessionId: '',
      kind: 'permission',
      phase: 'awaitingDecision',
      toolName: '编辑 tests/e2e/tenant.ts',
      arguments: '创建专用测试租户配置（一次性批准）',
      riskLevel: 'standard',
    },
  });

  const pagination = task({
    title: '任务列表分页参数',
    executor: 'dsh-acp',
    phase: 'reviewing',
    messages: [
      { role: 'user', content: '分页参数语义不统一，page/offset 混用。' },
      { role: 'assistant', content: '统一为 cursor 分页，改动 3 个文件，附带迁移说明。' },
    ],
    activities: [
      { activityId: 'act-6', kind: 'tool', label: 'repo 扫描', status: 'completed', isError: false },
      { activityId: 'act-7', kind: 'tool', label: '编辑 api/pagination.ts', status: 'completed', isError: false },
      { activityId: 'act-8', kind: 'tool', label: '单测', status: 'completed', isError: false },
    ],
    deliveryReview: {
      evidence: {
        capturedAtMs: at(120),
        head: '9f2c1ab7e4d2',
        workingTreeFingerprint: MOCK_FINGERPRINT,
        changedFiles: ['src/api/pagination.ts', 'src/api/routes.ts', 'docs/pagination.md'],
        diffPreview: 'diff 3 文件 · +412 −138（证据快照已脱敏）',
        attribution: [
          { path: 'src/api/pagination.ts', kind: 'taskModification' },
          { path: 'src/api/routes.ts', kind: 'taskModification' },
        ],
      },
      summary: '统一 cursor 分页语义并迁移调用点。',
      verificationResults: '单测 42/42 通过；类型检查通过。',
      runConclusion: '交付就绪，等待开发者审查。',
      decision: null,
    },
  });

  const depUpgrade = task({
    title: '依赖季度升级',
    executor: 'pi-rpc',
    phase: 'ended',
    messages: [
      { role: 'user', content: '按季度例行升级依赖。' },
      { role: 'assistant', content: '全部升级完成，回归通过，已合入。' },
    ],
    activities: [
      { activityId: 'act-9', kind: 'tool', label: 'bump deps', status: 'completed', isError: false },
      { activityId: 'act-10', kind: 'tool', label: '回归测试', status: 'completed', isError: false },
    ],
    deliveryReview: {
      evidence: {
        capturedAtMs: at(300),
        head: 'b41aa90cf2d1',
        workingTreeFingerprint: MOCK_FINGERPRINT,
        changedFiles: ['package.json', 'package-lock.json'],
        diffPreview: 'diff 2 文件 · +96 −104（证据快照已脱敏）',
        attribution: [
          { path: 'package.json', kind: 'taskModification' },
        ],
      },
      summary: '季度依赖例行升级，回归全绿。',
      verificationResults: '回归 118/118 通过。',
      runConclusion: '开发者已接受交付。',
      decision: 'accepted',
    },
  });

  const spike = task({
    title: '条带工作台技术 spike',
    executor: 'dsh-acp',
    phase: 'running',
    messages: [
      { role: 'user', content: '验证 overflow-x 条带在 WebView 里的滚动性能。' },
      { role: 'assistant', content: '7 列 × 每列 50 条消息的合成负载下，滚动帧率稳定。' },
    ],
    activities: [
      { activityId: 'act-11', kind: 'tool', label: '构建合成负载', status: 'completed', isError: false },
      { activityId: 'act-12', kind: 'tool', label: '帧率采样', status: 'started', isError: false },
    ],
  });

  const tokenLayer = task({
    title: 'DMS token 层落地',
    executor: 'pi-rpc',
    phase: 'reviewing',
    messages: [
      { role: 'user', content: '把 MD3 色彩角色落成 CSS custom properties。' },
      { role: 'assistant', content: 'token 层 + 禁止裸值的 lint 规则草案已就绪。' },
    ],
    activities: [
      { activityId: 'act-13', kind: 'tool', label: 'tokens.css', status: 'completed', isError: false },
      { activityId: 'act-14', kind: 'tool', label: 'stylelint 规则', status: 'completed', isError: false },
    ],
    deliveryReview: {
      evidence: {
        capturedAtMs: at(90),
        head: '77c0dd41e9b0',
        workingTreeFingerprint: MOCK_FINGERPRINT,
        changedFiles: ['src/tokens/tokens.css', 'src/tokens/theme.ts'],
        diffPreview: 'diff 5 文件 · +230 −40（证据快照已脱敏）',
        attribution: [
          { path: 'src/tokens/tokens.css', kind: 'taskModification' },
        ],
      },
      summary: 'MD3 角色命名 token 层 + 双主题契约。',
      verificationResults: 'token 契约测试通过。',
      runConclusion: '交付就绪，等待开发者审查。',
      decision: null,
    },
  });

  const gestures = task({
    title: '触摸板手势取舍',
    executor: 'dsh-acp',
    phase: 'waitingDeveloper',
    messages: [
      { role: 'user', content: '触摸板横滚要不要接三指轻扫切换工作区？' },
      { role: 'assistant', content: '三方案对比表已生成，涉及与系统手势的冲突面，需要你选。' },
    ],
    activities: [
      { activityId: 'act-15', kind: 'tool', label: '手势方案调研', status: 'completed', isError: false },
    ],
  });

  const ws1Tasks = [loginFix, e2eIsolation, pagination, depUpgrade];
  const ws2Tasks = [spike, tokenLayer, gestures];
  e2eIsolation.pendingOperation = {
    ...e2eIsolation.pendingOperation!,
    taskId: e2eIsolation.taskId,
    sessionId: e2eIsolation.sessionId ?? '',
  };

  return [
    {
      workspaceId: 'ws-halo-studio',
      displayName: 'halo-studio',
      rootPath: 'D:/workspace/halo-studio',
      branch: 'main',
      trusted: true,
      gitRepository: true,
      taskOrder: ws1Tasks.map(t => t.taskId),
      tasks: Object.fromEntries(ws1Tasks.map(t => [t.taskId, t])),
    },
    {
      workspaceId: 'ws-strip-spike',
      displayName: 'strip-spike',
      rootPath: 'D:/workspace/strip-spike',
      branch: 'Nyzeep/strip-spike',
      trusted: true,
      gitRepository: true,
      taskOrder: ws2Tasks.map(t => t.taskId),
      tasks: Object.fromEntries(ws2Tasks.map(t => [t.taskId, t])),
    },
  ];
}

export interface MockWorkbenchRuntimeDriverOptions {
  /**
   * When true, a scripted live event stream plays after start() (timers).
   * The singleton uses this for the demo experience; tests create isolated
   * stores with simulate off and drive events by hand.
   */
  simulate?: boolean;
}

interface ScriptedStep {
  delayMs: number;
  build: (sequence: number) => WorkbenchProjectionEvent;
}

/**
 * Purely additive scripted stream (messages / activities only) so it can
 * never contradict decisions the developer made meanwhile.
 */
function buildScript(workspace1: WorkbenchWorkspace, workspace2: WorkbenchWorkspace): ScriptedStep[] {
  const runningPi = workspace1.taskOrder[0];
  const runningDsh = workspace2.taskOrder[0];
  return [
    {
      delayMs: 1600,
      build: sequence => ({
        sequence,
        occurredAtMs: Date.now(),
        kind: 'sessionMessageAppended',
        summary: 'Workbench assistant message was updated',
        workspaceId: workspace1.workspaceId,
        taskId: runningPi,
        message: { role: 'assistant', content: '单飞刷新守卫的回归已经跑过一半，目前零失败。' },
      }),
    },
    {
      delayMs: 3200,
      build: sequence => ({
        sequence,
        occurredAtMs: Date.now(),
        kind: 'sessionActivityUpdated',
        summary: 'Workbench tool activity was updated',
        workspaceId: workspace1.workspaceId,
        taskId: runningPi,
        activity: { activityId: 'act-3', kind: 'tool', label: 'vitest --watch', status: 'completed', isError: false },
      }),
    },
    {
      delayMs: 4800,
      build: sequence => ({
        sequence,
        occurredAtMs: Date.now(),
        kind: 'sessionActivityUpdated',
        summary: 'Workbench tool activity was updated',
        workspaceId: workspace1.workspaceId,
        taskId: runningPi,
        activity: { activityId: 'act-16', kind: 'tool', label: '回归测试', status: 'started', isError: false },
      }),
    },
    {
      delayMs: 6400,
      build: sequence => ({
        sequence,
        occurredAtMs: Date.now(),
        kind: 'sessionMessageAppended',
        summary: 'Workbench assistant message was updated',
        workspaceId: workspace2.workspaceId,
        taskId: runningDsh,
        message: { role: 'assistant', content: '帧率采样进入第二轮，正在收集长任务列表下的数据。' },
      }),
    },
  ];
}

export function createMockWorkbenchRuntimeDriver(
  options: MockWorkbenchRuntimeDriverOptions = {},
): WorkbenchRuntimeDriver {
  const simulate = options.simulate ?? false;
  let timerIds: ReturnType<typeof setTimeout>[] = [];
  let sequence = 0;

  return {
    kind: 'mock-event-stream',
    start(host: WorkbenchRuntimeDriverHost): void {
      sequence = 0;
      const emit = (event: WorkbenchProjectionEventInput) => {
        sequence += 1;
        host.ingestEvent({ ...event, sequence } as WorkbenchProjectionEventInput & { sequence: number });
      };

      const workspaces = buildSeedWorkspaces();
      for (const workspace of workspaces) {
        emit({
          occurredAtMs: at(0),
          kind: 'workspaceOpened',
          summary: `Workbench workspace ${workspace.displayName} was opened`,
          workspace,
        });
      }
      emit({
        occurredAtMs: at(1),
        kind: 'runtimeStateChanged',
        summary: 'Workbench Runtime is ready (mock event stream)',
        runtimePhase: 'ready',
      });

      if (!simulate) return;
      const script = buildScript(workspaces[0], workspaces[1]);
      for (const step of script) {
        timerIds.push(setTimeout(() => {
          sequence += 1;
          host.ingestEvent(step.build(sequence));
        }, step.delayMs));
      }
    },
    stop(): void {
      for (const timerId of timerIds) clearTimeout(timerId);
      timerIds = [];
    },
  };
}
