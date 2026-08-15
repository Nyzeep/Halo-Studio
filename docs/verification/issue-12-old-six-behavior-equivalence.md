# 工单 12：旧六项行为等价性证据矩阵

<!-- Generated from issue-12-old-six-behavior-equivalence.json by product/Halo Studio/scripts/verify-old-six-behavior-equivalence.mjs. -->

**Status:** `blocked`

本文件是当前 Pi RPC Tauri 产品的前向证据矩阵。GitHub #9-#14 和归档材料只读且仅作为历史可迁移能力输入；它们不定义当前 P0，也没有被改写、关闭或重新验收。

## 结论边界

发布结论保持 `blocked`。工单 `14` 的真实 Pi RPC 原生 UI 验收为 `not-run`，分类为 `real-native-ui-not-run`：Only Issue 14 may record an authorized real Pi RPC session through the Halo native Tauri UI; this Issue 12 work uses no real credential, Pi RPC process, or model request.

**真实原生验收结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

自动化证据只证明公开 Runtime、PiRpcPort、Tauri command/event 和 Web infrastructure contract。受控 fixture、历史 OpenCode runtime/HTTP/SSE、旧 Sidecar JSONL、Pi TUI、Unix/CBOR PiServer、多执行器产品设想、Pi 内部源码、原始 session/entry/toolCall 标识及静态页面均为历史或范围外材料，不能替代真实原生 UI 结论。

## 本轮验证记录

| 命令 | 状态 | 退出码 | 分类 | 摘要 |
| --- | --- | ---: | --- | --- |
| `node --test "product/Halo Studio/scripts/verify-old-six-behavior-equivalence.test.mjs"` | `passed` | 0 |  | 24 matrix contract tests passed. |
| `pnpm --dir "product/Halo Studio" run check:repo-hygiene` | `passed` | 0 |  | Repository hygiene and the Issue 12 matrix verifier passed. |
| `pnpm --dir "product/Halo Studio" run type-check:web` | `passed` | 0 |  | Web TypeScript type check passed. |
| `pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts` | `passed` | 0 |  | The package runner executed 363 test files and 2,396 tests successfully. |
| `pnpm --dir "product/Halo Studio/src/web-ui" run test:run src/app/scenes/session/WorkbenchSessionScene.test.tsx` | `passed` | 0 |  | The native Workbench session scene contract passed 12 tests. |
| `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter` | `passed` | 0 |  | 9 Pi configuration contracts and 43 Pi RPC adapter contracts passed with controlled fixtures. |
| `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts` | `passed` | 0 |  | 46 public Workbench Runtime contracts passed with the injected PiRpcPort fixture. |
| `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts` | `blocked` | 1 | `desktop-mobile-web-resource-missing` | The bitfun-desktop build script exited 1 because the required src/mobile-web/dist resource is absent; Cargo then remained waiting on sherpa-onnx-sys until the already-failed command was manually stopped, so no desktop contract test ran. |
| `pnpm --dir "product/Halo Studio" run desktop:build:fast` | `blocked` | 1 | `vendor-checksum-mismatch` | Frontend build completed, then Cargo rejected vendor/cargo/allocator-api2/src/nightly.rs because its declared checksum differs from the file; vendor and lockfiles were not changed. |
| `rg -n 'GitHub #9|GitHub #10|GitHub #11|GitHub #12|GitHub #13|GitHub #14' docs/requirements/bitfun-tauri-product-migration docs/verification` | `passed` | 0 |  | The reference scan emitted matches across the historical range; the matrix contract independently enforces one canonical GitHub locator for each issue #9-#14. |
| `git diff --check` | `passed` | 0 |  | The Issue 12 change has no whitespace errors. |

## 主测试 Seam

主要程序化 seam 是 Halo Workbench Runtime 的公开 Tauri snapshot/intent command 和单一有序 event stream；Pi 传输只通过 PiRpcPort 进入 Runtime，前端只通过 workbench-runtime infrastructure client 访问该投影。

## 总览矩阵

| 旧 GitHub issue | 可观察行为 | 当前 P0 工单 | Runtime Interface | Pi RPC Adapter 证据 | 原生桌面路径 | 当前结论 |
| --- | --- | --- | --- | --- | --- | --- |
| GitHub #9 | Trusted acceptance workspace, bounded managed-task lifecycle, redacted activity, explicit human review, and no automatic Git delivery. | #04, #05 | Halo Workbench Runtime public Tauri snapshot/intent commands and one ordered event stream; no direct Pi transport or legacy session owner enters the renderer. | See evidence below | See evidence below | `blocked` |
| GitHub #10 | Non-secret launch configuration, credential-reference boundary, compatibility-gated executable startup, and truthful recovery guidance. | #06, #07 | Halo projects only non-sensitive configuration and Pi readiness facts through the Workbench Runtime interface; credential material stays behind the system credential port and adapter launch boundary. | See evidence below | See evidence below | `blocked` |
| GitHub #11 | A first task message establishes one managed session, projects redacted replies and activity, and settles only into waiting for the developer. | #08 | Managed session intents and snapshots expose a Halo-local session state, redacted messages, ordered activity, and waitingDeveloper without Pi session or entry identifiers. | See evidence below | See evidence below | `blocked` |
| GitHub #12 | A current task-scoped permission decision is one-time, matched to the request, redacted, and fails closed on duplicate or invalid resolution. | #09 | A Workbench Runtime pending operation projects a task-local redacted summary and accepts only one allow or deny intent after adapter confirmation. | See evidence below | See evidence below | `blocked` |
| GitHub #13 | The developer can follow up in the same task, explicitly end it, inspect frozen read-only evidence, and record a decision without automatic Git changes. | #10 | The public intent surface admits follow-up only from waitingDeveloper, separates finish-and-review from abort, and projects immutable review evidence for an explicit accept or reject decision. | See evidence below | See evidence below | `blocked` |
| GitHub #14 | Unexpected exits become interrupted without reconnect, prompt replay, operation replay, or duplicate writes; retained review facts remain truthful. | #11 | Interrupted is a first-class public session outcome whose restart path creates new work rather than restoring private Pi state or replaying active intents. | See evidence below | See evidence below | `blocked` |

## 逐项证据

### GitHub #9: Specification: real OpenCode managed task session (P0)

**旧证据（仅历史输入）**

- `historical-github-issue`: `historical`
  - locator: `https://github.com/Nyzeep/Halo-Studio/issues/9`
- `historical-baseline`: `historical`
  - locator: `docs/verification/migratable-capability-baseline/traceability.md`

**当前 Halo Runtime Interface**

Halo Workbench Runtime public Tauri snapshot/intent commands and one ordered event stream; no direct Pi transport or legacy session owner enters the renderer.

**当前 Pi RPC Adapter 证据**

- `public-runtime-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::managed_task_requires_confirmation_and_records_existing_git_baseline_before_starting`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts`
- `pi-rpc-adapter-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::configured_task_session_projects_authority_after_non_secret_readiness`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`

**当前原生桌面路径**

- `tauri-command-event-contract`: `blocked`
  - locator: `product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs::tauri_exposes_two_commands_and_one_ordered_event_stream`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts`
  - classification: `desktop-mobile-web-resource-missing`

**当前结论:** `blocked`

**结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

- `real-native-ui-not-run`: Automated contract evidence cannot replace the authorized real Pi RPC native UI acceptance owned by Issue 14.

### GitHub #10: Managed OpenCode 1.x compatible startup

**旧证据（仅历史输入）**

- `historical-github-issue`: `historical`
  - locator: `https://github.com/Nyzeep/Halo-Studio/issues/10`
- `historical-baseline`: `historical`
  - locator: `docs/verification/migratable-capability-baseline/traceability.md`

**当前 Halo Runtime Interface**

Halo projects only non-sensitive configuration and Pi readiness facts through the Workbench Runtime interface; credential material stays behind the system credential port and adapter launch boundary.

**当前 Pi RPC Adapter 证据**

- `pi-rpc-adapter-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::version_probe_uses_private_config_and_cleans_it_on_success_or_failure`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`
- `pi-rpc-adapter-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::start_fails_closed_when_a_required_readiness_capability_is_missing`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`

**当前原生桌面路径**

- `tauri-configuration-contract`: `blocked`
  - locator: `product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs::pi_credential_response_and_errors_are_stable_and_redacted`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts`
  - classification: `desktop-mobile-web-resource-missing`

**当前结论:** `blocked`

**结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

- `real-native-ui-not-run`: Controlled protocol fixtures prove the fail-closed seam but cannot prove a real locally installed Pi through Halo native UI.

### GitHub #11: Initial managed task session and waiting developer

**旧证据（仅历史输入）**

- `historical-github-issue`: `historical`
  - locator: `https://github.com/Nyzeep/Halo-Studio/issues/11`
- `historical-baseline`: `historical`
  - locator: `docs/verification/migratable-capability-baseline/traceability.md`

**当前 Halo Runtime Interface**

Managed session intents and snapshots expose a Halo-local session state, redacted messages, ordered activity, and waitingDeveloper without Pi session or entry identifiers.

**当前 Pi RPC Adapter 证据**

- `public-runtime-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::managed_first_turn_projects_redacted_activity_and_fences_late_events`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts`
- `pi-rpc-adapter-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::port_projects_crlf_tail_unicode_message_and_tool_events_without_raw_ids`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`

**当前原生桌面路径**

- `web-infrastructure-contract`: `passed`
  - locator: `product/Halo Studio/src/web-ui/src/infrastructure/workbench-runtime/client.test.ts::exposes the runtime through two commands and one ordered event stream`
  - command: `pnpm --dir "product/Halo Studio/src/web-ui" run test:run -- src/infrastructure/workbench-runtime/client.test.ts src/infrastructure/workbench-runtime/formalPath.contract.test.ts`
- `tauri-command-event-contract`: `blocked`
  - locator: `product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs::tauri_exposes_two_commands_and_one_ordered_event_stream`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts`
  - classification: `desktop-mobile-web-resource-missing`

**当前结论:** `blocked`

**结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

- `real-native-ui-not-run`: The controlled first-turn fixture is not a real Pi model response or native UI acceptance.

### GitHub #12: One-time Agent action requests

**旧证据（仅历史输入）**

- `historical-github-issue`: `historical`
  - locator: `https://github.com/Nyzeep/Halo-Studio/issues/12`
- `historical-baseline`: `historical`
  - locator: `docs/verification/migratable-capability-baseline/traceability.md#12-一次性-agent-操作请求`

**当前 Halo Runtime Interface**

A Workbench Runtime pending operation projects a task-local redacted summary and accepts only one allow or deny intent after adapter confirmation.

**当前 Pi RPC Adapter 证据**

- `pi-rpc-extension-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::extension_decision_is_redacted_one_shot_and_duplicate_request_fails_closed`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`

**当前原生桌面路径**

- `public-runtime-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::operation_decision_remains_pending_until_the_adapter_confirms_it`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts`
- `tauri-snapshot-event-contract`: `blocked`
  - locator: `product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs::snapshot_and_event_wire_shapes_are_camel_case_and_redacted`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts`
  - classification: `desktop-mobile-web-resource-missing`

**当前结论:** `blocked`

**结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

- `real-native-ui-not-run`: The first-party extension fixture is evidence of the controlled contract, not an authorized real tool gate in Halo native UI.

### GitHub #13: Follow-up, explicit finish, and delivery review

**旧证据（仅历史输入）**

- `historical-github-issue`: `historical`
  - locator: `https://github.com/Nyzeep/Halo-Studio/issues/13`
- `historical-baseline`: `historical`
  - locator: `docs/verification/migratable-capability-baseline/traceability.md#13-追问显式结束与交付审查`

**当前 Halo Runtime Interface**

The public intent surface admits follow-up only from waitingDeveloper, separates finish-and-review from abort, and projects immutable review evidence for an explicit accept or reject decision.

**当前 Pi RPC Adapter 证据**

- `public-runtime-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::prompt_settled_follow_up_and_abort_obey_non_replay_lifecycle`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts`
- `public-runtime-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::finish_and_review_freezes_evidence_and_releases_adapter_session`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts`
- `pi-rpc-adapter-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::follow_up_requires_a_prompt_and_abort_variant_crosses_the_same_seam`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`

**当前原生桌面路径**

- `web-gap-contract`: `passed`
  - locator: `product/Halo Studio/src/web-ui/src/app/scenes/session/WorkbenchSessionScene.test.tsx::leaves a settled managed task waiting without exposing follow-up controls`
  - command: `pnpm --dir "product/Halo Studio/src/web-ui" run test:run src/app/scenes/session/WorkbenchSessionScene.test.tsx`
  - classification: `managed-follow-up-ui-missing`
- `web-delivery-review-contract`: `passed`
  - locator: `product/Halo Studio/src/web-ui/src/app/scenes/session/WorkbenchSessionScene.test.tsx::renders a read-only delivery review and dispatches accept and reject decisions`
  - command: `pnpm --dir "product/Halo Studio/src/web-ui" run test:run src/app/scenes/session/WorkbenchSessionScene.test.tsx`
- `tauri-command-event-contract`: `blocked`
  - locator: `product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs::tauri_exposes_two_commands_and_one_ordered_event_stream`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts`
  - classification: `desktop-mobile-web-resource-missing`

**当前结论:** `blocked`

**结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

- `managed-follow-up-ui-missing`: The native Workbench waiting-developer view currently exposes finish-and-review but no follow-up input control; Runtime and Adapter contracts alone do not establish user-observable equivalence.
- `real-native-ui-not-run`: No authorized real Pi session has produced a disposable acceptance-workspace change and native review flow.

### GitHub #14: Interruption truthfulness and real-session release acceptance

**旧证据（仅历史输入）**

- `historical-github-issue`: `historical`
  - locator: `https://github.com/Nyzeep/Halo-Studio/issues/14`
- `historical-baseline`: `historical`
  - locator: `docs/archive/legacy-pyside-sidecar-baseline/requirements/07-real-opencode-managed-task-session-tickets/05-real-opencode-release-acceptance-checklist.md`

**当前 Halo Runtime Interface**

Interrupted is a first-class public session outcome whose restart path creates new work rather than restoring private Pi state or replaying active intents.

**当前 Pi RPC Adapter 证据**

- `public-runtime-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::restarted_runtime_does_not_replay_interrupted_managed_work_or_operations`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts`
- `pi-rpc-adapter-contract`: `passed`
  - locator: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::eof_and_protocol_failures_are_fail_closed_at_the_port`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter`

**当前原生桌面路径**

- `tauri-command-event-contract`: `blocked`
  - locator: `product/Halo Studio/src/apps/desktop/tests/halo_workbench_runtime_contracts.rs::workspace_switch_and_exit_delegate_cleanup_before_host_teardown`
  - command: `cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-desktop --test halo_workbench_runtime_contracts`
  - classification: `desktop-mobile-web-resource-missing`

**当前结论:** `blocked`

**结论证据**

- `deidentified-status-artifact`: `not-run`
  - locator: `docs/verification/issue-12-real-native-ui-acceptance-status.json`
  - classification: `real-native-ui-not-run`

- `real-native-ui-not-run`: Real interruption and cleanup acceptance through the Halo native UI remains the restricted Issue 14 responsibility.

## 排除项

下列材料或替身不构成等价断言：

- `legacy-sidecar-jsonl`
- `legacy-opencode-http-sse`
- `legacy-opencode-runtime`
- `pi-internal-source`
- `pi-tui`
- `unix-cbor-pi-server`
- `multi-executor-product-design`
- `raw-session-entry-or-tool-call-identifiers`
- `static-http-page`
- `controlled-fixture-as-real-native-acceptance`

所有旧六票恰好映射一次，所有 P0 工单 04-11 均有覆盖。任一失败、环境阻断或未运行项都必须带分类；`pnpm --dir "product/Halo Studio" run verify:old-six-behavior-equivalence` 执行 focused contract tests 和矩阵校验，规格入口 `check:repo-hygiene` 同样串联矩阵校验。
