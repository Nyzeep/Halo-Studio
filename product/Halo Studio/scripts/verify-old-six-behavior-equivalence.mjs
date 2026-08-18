import { readFileSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PRODUCT_ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const REPOSITORY_ROOT = resolve(PRODUCT_ROOT, '..', '..');
const MATRIX_PATH = [
  'docs',
  'verification',
  'issue-12-old-six-behavior-equivalence.json',
];
const REPORT_PATH = [
  'docs',
  'verification',
  'issue-12-old-six-behavior-equivalence.md',
];
const NATIVE_UI_STATUS_ARTIFACT_PATH = [
  'docs',
  'verification',
  'issue-12-real-native-ui-acceptance-status.json',
];
const NATIVE_UI_STATUS_ARTIFACT_LOCATOR = NATIVE_UI_STATUS_ARTIFACT_PATH.join('/');
const NATIVE_UI_STATUS_ARTIFACT_TYPE = 'issue-14-real-native-ui-acceptance-status';
const NATIVE_UI_STATUS_OWNER_ISSUE = '14';
// 历史证据守卫：以下 locator 与命令保留更名前的路径/crate 名，以匹配
// docs/verification/** 中不可篡改的历史证据；当前源码路径见 NATIVE_UI_STATUS_SOURCE_PATH。
const NATIVE_UI_STATUS_SOURCE_LOCATOR = 'docs/requirements/bitfun-tauri-product-migration/issues/14-complete-real-pi-rpc-native-ui-acceptance.md';
const NATIVE_UI_STATUS_SOURCE_PATH = 'docs/requirements/halo-tauri-product-migration/issues/14-complete-real-pi-rpc-native-ui-acceptance.md';
const NATIVE_UI_STATUS_RECORDED_BY_ISSUE = '12';
const EXPECTED_LEGACY_ISSUES = [9, 10, 11, 12, 13, 14];
const EXPECTED_P0_ISSUES = ['04', '05', '06', '07', '08', '09', '10', '11'];
const EXPECTED_P0_BY_LEGACY_ISSUE = new Map([
  [9, ['04', '05']],
  [10, ['06', '07']],
  [11, ['08']],
  [12, ['09']],
  [13, ['10']],
  [14, ['11']],
]);
const MATRIX_CONTRACT_TEST_COMMAND = 'node --test "product/Halo Studio/scripts/verify-old-six-behavior-equivalence.test.mjs"';
const WEB_RUNTIME_TEST_COMMAND = 'pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts';
const WEB_SESSION_SCENE_TEST_COMMAND = 'pnpm --dir "product/Halo Studio/src/web-ui" run test:run src/app/scenes/session/WorkbenchSessionScene.test.tsx';
const PI_RPC_ADAPTER_TEST_COMMAND = 'cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter';
const WORKBENCH_RUNTIME_TEST_COMMAND = 'cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts';
const NATIVE_DESKTOP_CONTRACT_COMMAND = 'cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts';
const EXPECTED_VERIFICATION_COMMANDS = [
  MATRIX_CONTRACT_TEST_COMMAND,
  'pnpm --dir "product/Halo Studio" run check:repo-hygiene',
  'pnpm --dir "product/Halo Studio" run type-check:web',
  WEB_RUNTIME_TEST_COMMAND,
  WEB_SESSION_SCENE_TEST_COMMAND,
  PI_RPC_ADAPTER_TEST_COMMAND,
  WORKBENCH_RUNTIME_TEST_COMMAND,
  NATIVE_DESKTOP_CONTRACT_COMMAND,
  'pnpm --dir "product/Halo Studio" run desktop:build:fast',
  "rg -n 'GitHub #9|GitHub #10|GitHub #11|GitHub #12|GitHub #13|GitHub #14' docs/requirements/bitfun-tauri-product-migration docs/verification",
  'git diff --check',
];
const HISTORICAL_EVIDENCE_KINDS = new Set(['historical-github-issue', 'historical-baseline']);
const CURRENT_EVIDENCE_KINDS = new Set([
  'pi-rpc-adapter-contract',
  'pi-rpc-extension-contract',
  'public-runtime-contract',
  'tauri-command-event-contract',
  'tauri-configuration-contract',
  'tauri-snapshot-event-contract',
  'web-formal-path-contract',
  'web-infrastructure-contract',
  'web-gap-contract',
  'web-delivery-review-contract',
]);
const EXPECTED_EXCLUDED_EVIDENCE_AUTHORITIES = [
  'legacy-sidecar-jsonl',
  'legacy-opencode-http-sse',
  'legacy-opencode-runtime',
  'pi-internal-source',
  'pi-tui',
  'unix-cbor-pi-server',
  'multi-executor-product-design',
  'raw-session-entry-or-tool-call-identifiers',
  'static-http-page',
  'controlled-fixture-as-real-native-acceptance',
];
const EXECUTION_STATUSES = new Set(['passed', 'blocked', 'failed', 'not-run']);
const CONCLUSION_EVIDENCE_KINDS = new Set(['deidentified-status-artifact']);
const CURRENT_EVIDENCE_LOCATOR_PREFIX = 'product/Halo Studio/';
const EVIDENCE_LOCATOR_FILES_BY_COMMAND = new Map([
  [
    WEB_RUNTIME_TEST_COMMAND,
    new Set([
      'product/Halo Studio/src/web-ui/src/infrastructure/workbench-runtime/client.test.ts',
      'product/Halo Studio/src/web-ui/src/infrastructure/workbench-runtime/formalPath.contract.test.ts',
    ]),
  ],
  [
    WEB_SESSION_SCENE_TEST_COMMAND,
    new Set([
      'product/Halo Studio/src/web-ui/src/app/scenes/session/WorkbenchSessionScene.test.tsx',
    ]),
  ],
  [
    PI_RPC_ADAPTER_TEST_COMMAND,
    new Set([
      'product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_configuration_contract.rs',
      'product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs',
    ]),
  ],
  [
    WORKBENCH_RUNTIME_TEST_COMMAND,
    new Set([
      'product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs',
    ]),
  ],
  [
    NATIVE_DESKTOP_CONTRACT_COMMAND,
    new Set([
      'product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs',
    ]),
  ],
]);
const SENSITIVE_TEXT_PATTERN = /\b(?:authorization|bearer|api[_-]?key|password|secret|token)\b|\bsk-[a-z0-9_-]+/i;

function fail(message) {
  throw new Error(`ISSUE_12_EQUIVALENCE_INVALID: ${message}`);
}

function isObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function requireString(value, label) {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`${label} must be a non-empty string`);
  }
}

function requireArray(value, label) {
  if (!Array.isArray(value) || value.length === 0) {
    fail(`${label} must be a non-empty array`);
  }
}

function validateRedactedText(value, label) {
  if (SENSITIVE_TEXT_PATTERN.test(value)) {
    fail(`${label} contains sensitive text`);
  }
}

function sortedUnique(values) {
  return [...new Set(values)].sort((left, right) => String(left).localeCompare(String(right)));
}

function assertExactSet(actual, expected, label) {
  const sortedActual = sortedUnique(actual);
  const sortedExpected = sortedUnique(expected);
  if (JSON.stringify(sortedActual) !== JSON.stringify(sortedExpected)) {
    fail(`${label} must be exactly ${sortedExpected.join(', ')}`);
  }
}

function validateEvidence(evidence, label) {
  requireArray(evidence, label);
  for (const [index, item] of evidence.entries()) {
    if (!isObject(item)) fail(`${label}[${index}] must be an object`);
    requireString(item.kind, `${label}[${index}].kind`);
    requireString(item.locator, `${label}[${index}].locator`);
  }
}

function validateHistoricalEvidence(evidence, label, legacyIssue) {
  validateEvidence(evidence, label);
  for (const [index, item] of evidence.entries()) {
    if (!HISTORICAL_EVIDENCE_KINDS.has(item.kind)) {
      fail(`${label}[${index}].kind must be an accepted historical evidence kind`);
    }
  }
  const githubEvidence = evidence.filter((item) => item.kind === 'historical-github-issue');
  const expectedLocator = `https://github.com/Nyzeep/Halo-Studio/issues/${legacyIssue}`;
  if (githubEvidence.length !== 1 || githubEvidence[0].locator !== expectedLocator) {
    fail(`${label} must contain exactly one canonical GitHub #${legacyIssue} locator`);
  }
}

function validateExecutionStatus(status, label) {
  requireString(status, `${label}.status`);
  if (!EXECUTION_STATUSES.has(status)) {
    fail(`${label}.status must be a known execution status`);
  }
}

function validateExecutionOutcome(outcome, label) {
  validateExecutionStatus(outcome.status, label);
  if (outcome.status !== 'passed') {
    requireString(outcome.classification, `${label}.classification`);
  }
}

function validateCurrentEvidence(evidence, label, verificationByCommand, repositoryRoot) {
  validateEvidence(evidence, label);
  for (const [index, item] of evidence.entries()) {
    if (!CURRENT_EVIDENCE_KINDS.has(item.kind)) {
      fail(`${label}[${index}].kind must be an accepted current evidence kind`);
    }
    if (!item.locator.startsWith(CURRENT_EVIDENCE_LOCATOR_PREFIX) || !item.locator.includes('::')) {
      fail(`${label}[${index}].locator must be a Halo product locator with a test anchor`);
    }
    const [relativeLocator, anchor] = item.locator.split('::', 2);
    requireString(anchor, `${label}[${index}].locator test anchor`);
    const productRoot = resolve(repositoryRoot, 'product', 'Halo Studio');
    const absoluteLocator = resolve(repositoryRoot, relativeLocator);
    const productRelativeLocator = relative(productRoot, absoluteLocator);
    if (productRelativeLocator.startsWith('..') || isAbsolute(productRelativeLocator)) {
      fail(`${label}[${index}].locator must stay inside the Halo product tree`);
    }
    let source;
    try {
      source = readFileSync(absoluteLocator, 'utf8');
    } catch (error) {
      fail(`${label}[${index}].locator file cannot be read: ${error.message}`);
    }
    if (!source.includes(anchor)) {
      fail(`${label}[${index}].locator must name an existing test anchor`);
    }
    requireString(item.command, `${label}[${index}].command`);
    validateExecutionOutcome(item, `${label}[${index}]`);
    const verification = verificationByCommand.get(item.command);
    if (!verification) {
      fail(`${label}[${index}] must reference verification provenance for an exact command`);
    }
    if (
      item.status !== verification.status
      || (item.status !== 'passed' && item.classification !== verification.classification)
    ) {
      fail(`${label}[${index}] must match verification provenance for ${item.command}`);
    }
    const allowedLocatorFiles = EVIDENCE_LOCATOR_FILES_BY_COMMAND.get(item.command);
    if (!allowedLocatorFiles?.has(relativeLocator)) {
      fail(`${label}[${index}].locator must be exercised by its exact command`);
    }
  }
}

function validatePiRpcAdapterEvidence(evidence, label, verificationByCommand, repositoryRoot) {
  validateCurrentEvidence(evidence, label, verificationByCommand, repositoryRoot);
  if (!evidence.some((item) => (
    item.command === PI_RPC_ADAPTER_TEST_COMMAND && item.kind.startsWith('pi-rpc-')
  ))) {
    fail(`${label} must include a Pi RPC Adapter contract`);
  }
}

function validateKnownBehaviorGaps(entry) {
  if (entry.legacyIssue !== 13) return;

  const sceneTestLocator = 'product/Halo Studio/src/web-ui/src/app/scenes/session/WorkbenchSessionScene.test.tsx';
  const hasFollowUpGap = entry.desktopPathEvidence.some((item) => (
    item.kind === 'web-gap-contract'
    && item.command === WEB_SESSION_SCENE_TEST_COMMAND
    && item.classification === 'managed-follow-up-ui-missing'
    && item.locator === `${sceneTestLocator}::leaves a settled managed task waiting without exposing follow-up controls`
  ));
  const hasDeliveryReview = entry.desktopPathEvidence.some((item) => (
    item.kind === 'web-delivery-review-contract'
    && item.command === WEB_SESSION_SCENE_TEST_COMMAND
    && item.locator === `${sceneTestLocator}::renders a read-only delivery review and dispatches accept and reject decisions`
  ));
  const hasFollowUpBlocker = entry.conclusion.blockers.some(
    (blocker) => blocker.classification === 'managed-follow-up-ui-missing',
  );
  if (!hasFollowUpGap || !hasDeliveryReview || !hasFollowUpBlocker) {
    fail('GitHub #13 must record its managed follow-up UI gap and delivery review coverage');
  }
}

function validateNativeDesktopPath(evidence, label) {
  if (!evidence.some((item) => (
    item.command === NATIVE_DESKTOP_CONTRACT_COMMAND && item.kind.startsWith('tauri-')
  ))) {
    fail(`${label} must include a native Tauri desktop path`);
  }
}

function validateVerification(verification) {
  requireArray(verification, 'verification');
  for (const [index, result] of verification.entries()) {
    if (!isObject(result)) fail(`verification[${index}] must be an object`);
    requireString(result.command, `verification[${index}].command`);
    validateExecutionOutcome(result, `verification[${index}]`);
    if (!Number.isInteger(result.exitCode)) {
      fail(`verification[${index}].exitCode must be an integer`);
    }
    if (result.status === 'passed' && result.exitCode !== 0) {
      fail(`verification[${index}] passed results must have exitCode 0`);
    }
    if (result.status !== 'passed' && result.exitCode === 0) {
      fail(`verification[${index}] non-passing results must have a non-zero exitCode`);
    }
    requireString(result.summary, `verification[${index}].summary`);
  }
  const commands = verification.map((result) => result.command);
  if (new Set(commands).size !== commands.length) {
    fail('verification commands must not repeat');
  }
  assertExactSet(commands, EXPECTED_VERIFICATION_COMMANDS, 'verification commands');
  return new Map(verification.map((result) => [result.command, result]));
}

function validateConclusionEvidence(evidence, label) {
  validateEvidence(evidence, label);
  for (const [index, item] of evidence.entries()) {
    if (!CONCLUSION_EVIDENCE_KINDS.has(item.kind)) {
      fail(`${label}[${index}].kind must be an accepted conclusion evidence kind`);
    }
    validateExecutionOutcome(item, `${label}[${index}]`);
  }
}

function validateNativeUiConclusionEvidence(evidence, label) {
  validateConclusionEvidence(evidence, label);
  if (evidence.length !== 1) {
    fail(`${label} must contain exactly one native UI status artifact`);
  }
  const [artifact] = evidence;
  if (
    artifact.kind !== 'deidentified-status-artifact'
    || artifact.locator !== NATIVE_UI_STATUS_ARTIFACT_LOCATOR
    || artifact.status !== 'not-run'
    || artifact.classification !== 'real-native-ui-not-run'
  ) {
    fail(`${label} must reference the deidentified Issue 14 status artifact`);
  }
}

export function validateNativeUiStatusArtifact(
  artifact,
  { repositoryRoot = REPOSITORY_ROOT } = {},
) {
  if (!isObject(artifact)) fail('native UI status artifact must be an object');
  if (artifact.schemaVersion !== 1) fail('native UI status artifact schemaVersion must be 1');
  if (artifact.issue !== NATIVE_UI_STATUS_OWNER_ISSUE) {
    fail('native UI status artifact must point to Issue 14');
  }
  if (artifact.artifactType !== NATIVE_UI_STATUS_ARTIFACT_TYPE) {
    fail('native UI status artifact must use the Issue 14 artifact type');
  }
  if (artifact.ownerIssue !== NATIVE_UI_STATUS_OWNER_ISSUE) {
    fail('native UI status artifact must be owned by Issue 14');
  }
  if (artifact.sourceLocator !== NATIVE_UI_STATUS_SOURCE_LOCATOR) {
    fail('native UI status artifact must use the canonical Issue 14 source locator');
  }
  if (artifact.recordedByIssue !== NATIVE_UI_STATUS_RECORDED_BY_ISSUE) {
    fail('native UI status artifact must identify the Issue 12 recording context');
  }
  let source;
  try {
    source = readFileSync(resolve(repositoryRoot, NATIVE_UI_STATUS_SOURCE_PATH), 'utf8');
  } catch (error) {
    fail(`native UI status artifact source cannot be read: ${error.message}`);
  }
  if (!source.includes('# 14 - 完成真实 Pi RPC 原生 UI 验收')) {
    fail('native UI status artifact source must be the canonical Issue 14 specification');
  }
  if (artifact.status !== 'not-run') fail('native UI status artifact must remain not-run');
  if (artifact.classification !== 'real-native-ui-not-run') {
    fail('native UI status artifact classification must remain real-native-ui-not-run');
  }
  requireString(artifact.reason, 'native UI status artifact reason');
  validateRedactedText(artifact.reason, 'native UI status artifact reason');
}

function code(value) {
  return `\`${value}\``;
}

function formatEvidence(evidence) {
  return evidence
    .map((item) => {
      const lines = [
        `- ${code(item.kind)}: ${code(item.status ?? 'historical')}`,
        `  - locator: ${code(item.locator)}`,
      ];
      if (item.command) lines.push(`  - command: ${code(item.command)}`);
      if (item.classification) lines.push(`  - classification: ${code(item.classification)}`);
      return lines.join('\n');
    })
    .join('\n');
}

function formatBlockers(blockers) {
  return blockers
    .map((blocker) => `- ${code(blocker.classification)}: ${blocker.reason}`)
    .join('\n');
}

export function validateOldSixBehaviorEquivalence(
  matrix,
  { repositoryRoot = REPOSITORY_ROOT } = {},
) {
  if (!isObject(matrix)) fail('matrix must be an object');
  if (matrix.schemaVersion !== 1) fail('schemaVersion must be 1');
  if (matrix.releaseStatus !== 'blocked') fail('releaseStatus must remain blocked');

  if (!isObject(matrix.realNativeUiAcceptance)) {
    fail('realNativeUiAcceptance must be an object');
  }
  if (matrix.realNativeUiAcceptance.issue !== '14') {
    fail('realNativeUiAcceptance must point to Issue 14');
  }
  if (matrix.realNativeUiAcceptance.status !== 'not-run') {
    fail('realNativeUiAcceptance must stay not-run until Issue 14 records real native acceptance');
  }
  requireString(
    matrix.realNativeUiAcceptance.classification,
    'realNativeUiAcceptance.classification',
  );
  requireString(matrix.realNativeUiAcceptance.reason, 'realNativeUiAcceptance.reason');
  validateRedactedText(matrix.realNativeUiAcceptance.reason, 'realNativeUiAcceptance.reason');
  validateNativeUiConclusionEvidence(
    matrix.realNativeUiAcceptance.evidence,
    'realNativeUiAcceptance.evidence',
  );
  requireArray(matrix.excludedEvidenceAuthorities, 'excludedEvidenceAuthorities');
  assertExactSet(
    matrix.excludedEvidenceAuthorities,
    EXPECTED_EXCLUDED_EVIDENCE_AUTHORITIES,
    'excluded evidence authorities',
  );
  const verificationByCommand = validateVerification(matrix.verification);

  requireArray(matrix.entries, 'entries');
  const legacyIssues = matrix.entries.map((entry) => entry.legacyIssue);
  assertExactSet(legacyIssues, EXPECTED_LEGACY_ISSUES, 'legacy issues');
  if (new Set(legacyIssues).size !== legacyIssues.length) {
    fail('each legacy issue must have exactly one matrix entry');
  }

  const coveredP0Issues = [];
  for (const entry of matrix.entries) {
    if (!isObject(entry)) fail('matrix entries must be objects');
    requireString(entry.legacyTitle, `GitHub #${entry.legacyIssue}.legacyTitle`);
    requireString(entry.legacyBehavior, `GitHub #${entry.legacyIssue}.legacyBehavior`);
    validateHistoricalEvidence(
      entry.legacyEvidence,
      `GitHub #${entry.legacyIssue}.legacyEvidence`,
      entry.legacyIssue,
    );
    requireArray(entry.p0Issues, `GitHub #${entry.legacyIssue}.p0Issues`);
    coveredP0Issues.push(...entry.p0Issues);
    requireString(entry.runtimeInterface, `GitHub #${entry.legacyIssue}.runtimeInterface`);
    validatePiRpcAdapterEvidence(
      entry.piRpcAdapterEvidence,
      `GitHub #${entry.legacyIssue}.piRpcAdapterEvidence`,
      verificationByCommand,
      repositoryRoot,
    );
    validateCurrentEvidence(
      entry.desktopPathEvidence,
      `GitHub #${entry.legacyIssue}.desktopPathEvidence`,
      verificationByCommand,
      repositoryRoot,
    );
    validateNativeDesktopPath(
      entry.desktopPathEvidence,
      `GitHub #${entry.legacyIssue}.desktopPathEvidence`,
    );

    if (!isObject(entry.conclusion) || entry.conclusion.status !== 'blocked') {
      fail(`GitHub #${entry.legacyIssue}.conclusion must be blocked`);
    }
    validateNativeUiConclusionEvidence(
      entry.conclusion.evidence,
      `GitHub #${entry.legacyIssue}.conclusion.evidence`,
    );
    requireArray(entry.conclusion.blockers, `GitHub #${entry.legacyIssue}.conclusion.blockers`);
    for (const [index, blocker] of entry.conclusion.blockers.entries()) {
      if (!isObject(blocker)) {
        fail(`GitHub #${entry.legacyIssue}.conclusion.blockers[${index}] must be an object`);
      }
      requireString(
        blocker.classification,
        `GitHub #${entry.legacyIssue}.conclusion.blockers[${index}].classification`,
      );
      requireString(
        blocker.reason,
        `GitHub #${entry.legacyIssue}.conclusion.blockers[${index}].reason`,
      );
    }
    validateKnownBehaviorGaps(entry);
  }

  assertExactSet(coveredP0Issues, EXPECTED_P0_ISSUES, 'P0 issue coverage');
  for (const entry of matrix.entries) {
    assertExactSet(
      entry.p0Issues,
      EXPECTED_P0_BY_LEGACY_ISSUE.get(entry.legacyIssue),
      `GitHub #${entry.legacyIssue} P0 mapping`,
    );
  }

  return {
    legacyIssues: [...EXPECTED_LEGACY_ISSUES],
    p0Issues: [...EXPECTED_P0_ISSUES],
    releaseStatus: matrix.releaseStatus,
    realNativeUiStatus: matrix.realNativeUiAcceptance.status,
  };
}

export function renderOldSixBehaviorEquivalence(
  matrix,
  { repositoryRoot = REPOSITORY_ROOT } = {},
) {
  validateOldSixBehaviorEquivalence(matrix, { repositoryRoot });

  const lines = [
    '# 工单 12：旧六项行为等价性证据矩阵',
    '',
    '<!-- Generated from issue-12-old-six-behavior-equivalence.json by product/Halo Studio/scripts/verify-old-six-behavior-equivalence.mjs. -->',
    '',
    `**Status:** ${code(matrix.releaseStatus)}`,
    '',
    '本文件是当前 Pi RPC Tauri 产品的前向证据矩阵。GitHub #9-#14 和归档材料只读且仅作为历史可迁移能力输入；它们不定义当前 P0，也没有被改写、关闭或重新验收。',
    '',
    '## 结论边界',
    '',
    `发布结论保持 ${code(matrix.releaseStatus)}。工单 ${code(matrix.realNativeUiAcceptance.issue)} 的真实 Pi RPC 原生 UI 验收为 ${code(matrix.realNativeUiAcceptance.status)}，分类为 ${code(matrix.realNativeUiAcceptance.classification)}：${matrix.realNativeUiAcceptance.reason}`,
    '',
    '**真实原生验收结论证据**',
    '',
    formatEvidence(matrix.realNativeUiAcceptance.evidence),
    '',
    '自动化证据只证明公开 Runtime、PiRpcPort、Tauri command/event 和 Web infrastructure contract。受控 fixture、历史 OpenCode runtime/HTTP/SSE、旧 Sidecar JSONL、Pi TUI、Unix/CBOR PiServer、多执行器产品设想、Pi 内部源码、原始 session/entry/toolCall 标识及静态页面均为历史或范围外材料，不能替代真实原生 UI 结论。',
    '',
    '## 本轮验证记录',
    '',
    '| 命令 | 状态 | 退出码 | 分类 | 摘要 |',
    '| --- | --- | ---: | --- | --- |',
    ...matrix.verification.map((result) => (
      `| ${code(result.command)} | ${code(result.status)} | ${result.exitCode} | ${result.classification ? code(result.classification) : ''} | ${result.summary} |`
    )),
    '',
    '## 主测试 Seam',
    '',
    '主要程序化 seam 是 Halo Workbench Runtime 的公开 Tauri snapshot/intent command 和单一有序 event stream；Pi 传输只通过 PiRpcPort 进入 Runtime，前端只通过 workbench-runtime infrastructure client 访问该投影。',
    '',
    '## 总览矩阵',
    '',
    '| 旧 GitHub issue | 可观察行为 | 当前 P0 工单 | Runtime Interface | Pi RPC Adapter 证据 | 原生桌面路径 | 当前结论 |',
    '| --- | --- | --- | --- | --- | --- | --- |',
  ];

  for (const entry of matrix.entries) {
    lines.push(
      `| GitHub #${entry.legacyIssue} | ${entry.legacyBehavior} | ${entry.p0Issues.map((issue) => `#${issue}`).join(', ')} | ${entry.runtimeInterface} | See evidence below | See evidence below | ${code(entry.conclusion.status)} |`,
    );
  }

  lines.push('', '## 逐项证据', '');
  for (const entry of matrix.entries) {
    lines.push(
      `### GitHub #${entry.legacyIssue}: ${entry.legacyTitle}`,
      '',
      '**旧证据（仅历史输入）**',
      '',
      formatEvidence(entry.legacyEvidence),
      '',
      '**当前 Halo Runtime Interface**',
      '',
      entry.runtimeInterface,
      '',
      '**当前 Pi RPC Adapter 证据**',
      '',
      formatEvidence(entry.piRpcAdapterEvidence),
      '',
      '**当前原生桌面路径**',
      '',
      formatEvidence(entry.desktopPathEvidence),
      '',
      `**当前结论:** ${code(entry.conclusion.status)}`,
      '',
      '**结论证据**',
      '',
      formatEvidence(entry.conclusion.evidence),
      '',
      formatBlockers(entry.conclusion.blockers),
      '',
    );
  }

  lines.push(
    '## 排除项',
    '',
    '下列材料或替身不构成等价断言：',
    '',
    ...matrix.excludedEvidenceAuthorities.map((authority) => `- ${code(authority)}`),
    '',
    '所有旧六票恰好映射一次，所有 P0 工单 04-11 均有覆盖。任一失败、环境阻断或未运行项都必须带分类；`pnpm --dir "product/Halo Studio" run verify:old-six-behavior-equivalence` 执行 focused contract tests 和矩阵校验，规格入口 `check:repo-hygiene` 同样串联矩阵校验。',
    '',
  );

  return lines.join('\n');
}

export function verifyOldSixBehaviorEquivalence({ repositoryRoot = REPOSITORY_ROOT } = {}) {
  const matrixFile = resolve(repositoryRoot, ...MATRIX_PATH);
  const reportFile = resolve(repositoryRoot, ...REPORT_PATH);
  let matrix;
  try {
    matrix = JSON.parse(readFileSync(matrixFile, 'utf8'));
  } catch (error) {
    fail(`cannot read ${MATRIX_PATH.join('/')}: ${error.message}`);
  }
  const summary = validateOldSixBehaviorEquivalence(matrix, { repositoryRoot });
  let nativeUiStatusArtifact;
  try {
    nativeUiStatusArtifact = JSON.parse(
      readFileSync(resolve(repositoryRoot, ...NATIVE_UI_STATUS_ARTIFACT_PATH), 'utf8'),
    );
  } catch (error) {
    fail(`cannot read ${NATIVE_UI_STATUS_ARTIFACT_PATH.join('/')}: ${error.message}`);
  }
  validateNativeUiStatusArtifact(nativeUiStatusArtifact, { repositoryRoot });
  let report;
  try {
    report = readFileSync(reportFile, 'utf8');
  } catch (error) {
    fail(`cannot read ${REPORT_PATH.join('/')}: ${error.message}`);
  }
  if (report !== renderOldSixBehaviorEquivalence(matrix, { repositoryRoot })) {
    fail(`${REPORT_PATH.join('/')} is not synchronized with the matrix`);
  }
  return summary;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const summary = verifyOldSixBehaviorEquivalence();
  console.log(
    `Issue 12 behavior-equivalence matrix passed (${summary.legacyIssues.length} legacy issues, ${summary.p0Issues.length} P0 issues; release ${summary.releaseStatus}).`,
  );
}
