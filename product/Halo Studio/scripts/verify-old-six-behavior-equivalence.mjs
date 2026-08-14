import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
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
const NATIVE_DESKTOP_CONTRACT_COMMAND = 'cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts';

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

function validateCurrentEvidence(evidence, label) {
  validateEvidence(evidence, label);
  for (const [index, item] of evidence.entries()) {
    requireString(item.command, `${label}[${index}].command`);
    validateExecutionOutcome(item, `${label}[${index}]`);
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
    requireString(result.summary, `verification[${index}].summary`);
  }
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

export function validateOldSixBehaviorEquivalence(matrix) {
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
  requireArray(matrix.excludedEvidenceAuthorities, 'excludedEvidenceAuthorities');
  assertExactSet(
    matrix.excludedEvidenceAuthorities,
    EXPECTED_EXCLUDED_EVIDENCE_AUTHORITIES,
    'excluded evidence authorities',
  );
  validateVerification(matrix.verification);

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
    validateEvidence(entry.legacyEvidence, `GitHub #${entry.legacyIssue}.legacyEvidence`);
    requireArray(entry.p0Issues, `GitHub #${entry.legacyIssue}.p0Issues`);
    coveredP0Issues.push(...entry.p0Issues);
    requireString(entry.runtimeInterface, `GitHub #${entry.legacyIssue}.runtimeInterface`);
    validateCurrentEvidence(
      entry.piRpcAdapterEvidence,
      `GitHub #${entry.legacyIssue}.piRpcAdapterEvidence`,
    );
    validateCurrentEvidence(
      entry.desktopPathEvidence,
      `GitHub #${entry.legacyIssue}.desktopPathEvidence`,
    );
    validateNativeDesktopPath(
      entry.desktopPathEvidence,
      `GitHub #${entry.legacyIssue}.desktopPathEvidence`,
    );

    if (!isObject(entry.conclusion) || entry.conclusion.status !== 'blocked') {
      fail(`GitHub #${entry.legacyIssue}.conclusion must be blocked`);
    }
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

export function renderOldSixBehaviorEquivalence(matrix) {
  validateOldSixBehaviorEquivalence(matrix);

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
    '所有旧六票恰好映射一次，所有 P0 工单 04-11 均有覆盖。任一失败、环境阻断或未运行项都必须带分类；校验由 `pnpm --dir "product/Halo Studio" run check:repo-hygiene` 执行。',
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
  const summary = validateOldSixBehaviorEquivalence(matrix);
  let report;
  try {
    report = readFileSync(reportFile, 'utf8');
  } catch (error) {
    fail(`cannot read ${REPORT_PATH.join('/')}: ${error.message}`);
  }
  if (report !== renderOldSixBehaviorEquivalence(matrix)) {
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
