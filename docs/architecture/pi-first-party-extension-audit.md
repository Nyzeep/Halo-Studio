# Halo first-party Pi extension audit

Status: audit record for issue 13's Pi extension and license-gate rehearsal; release approval remains
blocked until the recorded source, dependency, permission, and license evidence
is reviewed against the exact product tree that will be released.

This record covers the only Halo-owned extension permitted on the Halo P0 path.
Pi inline built-ins are a separate host capability inventory and are not
silently counted as Halo first-party extensions. This record does not authorize
Pi's default permissions, project extensions, user extensions, Pi packages,
Provider extensions, or runtime downloads.

## Fixed artifact

| Field | Audit record |
| --- | --- |
| Extension ID | `halo-workbench-permission-gate` |
| Fixed version | `1.0.0` (`HALO_PI_EXTENSION_VERSION`) |
| Source file | `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts` |
| Source binding | The Adapter embeds the audited source with `include_str!`; the source last changed in `e8c445d6a81d90851ac03d6aac7a4f11b6b749a3`, whose commit tree is `f50918b6bdebc6067f409f248cc9182ff5bcdec3`; the current `git hash-object` is `15d6908cc30e45f8812a87c591e58799d2f7ae69`. |
| SHA-256 | `A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B` |
| Load boundary | The Adapter copies the verified source to its own task/process temporary directory and loads only the exact path with `--no-extensions --extension <exact-path>`. It must not load from the project `.pi` tree or the user-wide Pi directory. |
| Cleanup | The task/process temporary extension directory is removed during normal stop, abort, EOF, failure, and application exit cleanup. A cleanup failure is a failed gate, not a successful release result. |

## Behavior and host permissions

- The extension registers `tool_call` before tool execution and may call
  `ctx.ui.confirm` to request a one-time developer decision.
- The only decision is a task-, session-, generation-, turn-, and single
  redacted-tool-call binding. Allow and deny are one-shot. A raw Pi session ID,
  entry ID, tool-call ID, complete parameters, command output, and extension
  payload never leave the Adapter.
- Halo accepts only the matching `extension_ui_response` ID. Deny, timeout,
  response-ID mismatch, duplicate or cross-task reuse, protocol error,
  `extension_error`, extension crash, and response-send failure all fail
  closed.
- The extension has no direct Halo credential, file, Git, process, or network
  API. It executes inside the Pi process and therefore inherits the launching
  user's host permissions; it is not a sandbox. Workspace trust, task state,
  redaction, decision authority, and lifecycle remain outside Pi in Halo
  Workbench Runtime.

## Dependencies and supply-chain boundary

- `@earendil-works/pi-coding-agent` may appear only as a TypeScript type import;
  it is not a runtime dependency of the extension and must not add a package
  download to Cargo or PNPM lockfiles.
- The runtime loader is supplied by the user's installed Pi. Halo must not
  download from npm, Git, a project `.pi/extensions` directory, or an arbitrary
  extension path at runtime. `<PI_REFERENCE_ROOT>` is read-only reference material and
  is not a source, dependency, or build input.
- Pi 0.83.0 always injects the hidden `llama.cpp` extension as an inline built-in;
  `--no-extensions` closes project/user discovery but does not remove that
  built-in. The inventory records its network, token-file, credential and model
  state capabilities and keeps it `releaseEligible: false` until a separately
  pinned, license-complete host boundary is approved.
- Any dependency, transitive dependency, package source, or generated bundle
  change requires a new fixed version, hash, inventory, and acceptance review.

## Update responsibility and license boundary

- Halo Studio maintainers own updates to this extension. Any source or runtime
  behavior change increments the fixed version, records a new SHA-256 and
  source commit, reruns the extension and Workbench contract tests, and repeats
  the license review before the `--extension` path can be released.
- The Halo repository `LICENSE` and applicable distribution notices are the
  authority for this source. The Pi executable, Pi Provider packages, and
  `@earendil-works/pi-coding-agent` license must not be inferred from this
  extension record. Pi is not bundled by the P0 product; a future bundled
  distribution requires a separate license, attribution, and complete-text
  review.
- Missing source provenance, hash, dependency evidence, host-permission
  analysis, or license text keeps the release gate `blocked`.

## Reproducible audit commands

Run from the repository root. These commands inspect the local artifact only;
they do not start Pi, send a prompt, or read real credentials.

```powershell
$extension = "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts"
Get-FileHash -Algorithm SHA256 $extension
git hash-object -- $extension
git rev-parse e8c445d6a81d90851ac03d6aac7a4f11b6b749a3^{tree}
rg -n 'HALO_PI_EXTENSION_ID|HALO_PI_EXTENSION_VERSION|HALO_PI_EXTENSION_PERMISSIONS|include_str!|--no-extensions|--extension' "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/lib.rs"
cargo tree --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter extension_decision_is_redacted_one_shot_and_duplicate_request_fails_closed
```

The expected SHA-256 is the value recorded above. A mismatch, missing source,
new runtime dependency, or unreviewed license is a blocking difference.

The machine-readable inventory is
`docs/architecture/pi-first-party-extension-inventory.json`. It records the
fixed source version, source commit/tree/blob, SHA-256, load arguments,
tool/event surface, host permissions, direct/transitive dependency
boundary, license evidence, and update responsibility. The release-gate
module in `product/Halo Studio/scripts/pi-extension-audit.mjs` consumes that
inventory and its source, candidate, dependency, load, license, and release
evidence. Its public result has only two decisions: `blocked` or `eligible`.
It also retains structured `findings`, safe `evidenceLocators`, and the
declared plus freshly derived `blockingReasons`. The CLI is the only
machine-verifiable entry point: it emits this result as JSON and returns exit
code `1` for `blocked` evidence.
For a fresh check of the external candidate tree, set the local read-only
checkout through `HALO_BITFUN_REFERENCE_ROOT`; the committed evidence stores
only the `readonly-evidence://bitfun-latest` locator and never a machine path.

The current inventory remains blocked because the read-only Pi host tree has no
Git commit/tag, its package closure is not a Halo lockfile dependency or Halo
release artifact, its inline `llama.cpp` built-in is not release-eligible, the
workspace member boundary is duplicated, and no exact desktop distribution
artifact has yet been recorded for the license/notice inclusion check. The Pi
package license is not inferred from its name; any future bundled Pi
distribution requires its own license, attribution, source provenance, and
complete dependency review.

## Audit CLI and release-gate seam contract

`auditReleaseGate({ manifestPath, repoRoot })` is a read-only evidence checker,
not a runtime gate that starts Pi. It only reads the manifest, repository
files, Git objects, and explicitly provided read-only evidence trees; it does
not write files, install packages, open network connections, read credentials,
send prompts, or execute a Pi binary. A complete fixture returns
`{ status: "eligible", findings: [], blockingReasons: [] }`; incomplete,
declared-blocked, invalid, or exceptional evidence returns
`status: "blocked"` with structured findings and safe locators. The CLI delegates to
this seam; a declared blocker remains `blocked` even if an input record says
`passed`. Markdown records explain the result but do not maintain a second
release state.

The contract tests cover the release-gate seam and real CLI process (`--help`,
blocked/eligible `--json`, unknown and missing arguments), rooted Windows path
redaction, dynamic
extension imports/host capabilities, extensionless runtime inputs, computed
`globalThis`/`window` property access (including aliases and optional computed
calls), including aliases bound from `const g = globalThis`, structured fail-closed
extension metadata, and host closure/release-file evidence. Host license evidence and release files are canonicalized against all
extension-owned license evidence, `distributionFiles`, and
`releaseArtifactEvidence` paths, so reuse is blocked. The current source
contains 71 audit contract tests; upstream evidence also requires recorded
`HEAD^` and clean-status command results. The release matrix must be updated
only from the command's actual exit code and test count.

## Pi host built-in boundary

The candidate host is Pi `0.83.0`. Pi's `builtInExtensions` includes hidden
`llama.cpp` factories that are loaded even when `--no-extensions` is present;
the flag disables discovery paths, not inline factories. The read-only evidence
is `<PI_REFERENCE_ROOT>/packages/coding-agent/src/extensions/index.ts`,
`main.ts`, `resource-loader.ts`, and the `llama` source files recorded in the
machine-readable inventory.

The observed built-in can register a provider and model command, access a
configured llama.cpp HTTP/HTTPS server and Hugging Face endpoints, read
`LLAMA_API_KEY`, `LLAMA_BASE_URL`, `HF_TOKEN` and token files, and persist model
state. It has no exact Git source commit/tag in the host tree and no Halo
release license/notice closure. It is therefore explicitly excluded from the
Halo first-party inventory and blocks the release gate.

## Migration verification command set

The following are the issue-13 documentation gate commands. They must use a
fake Pi child process or protocol fixture; they must not start a real
`pi --mode rpc`, send a prompt, or read credentials.

```powershell
where.exe pi
Get-Command pi -All | Select-Object Name,CommandType,Source,Path
pi --version
pnpm --dir "product/Halo Studio" run check:repo-hygiene
pnpm --dir "product/Halo Studio" run product:check
pnpm --dir "product/Halo Studio" run product:test
pnpm --dir "product/Halo Studio" run type-check:web
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-agent-runtime --test workbench_runtime_contracts
pnpm --dir "product/Halo Studio" run desktop:build:fast
git diff --check
```

In the recorded environment `where.exe pi` exits `1`, while
`Get-Command pi -All` finds the PowerShell/npm shims and `pi --version` reports
`0.83.0`. This is executable discovery evidence only, not RPC readiness; the
readiness gate remains the fake/fixture handshake for `get_state` and
`get_entries`/`since`.

## Primary-source research note

The first-party sources materially confirm the release boundary and separate
host, provider, and license provenance:

- **Explicit loading is not extension-free.** `<PI_REFERENCE_ROOT>/packages/coding-agent/src/cli/args.ts:151-155,281-282` parses `--extension`/`-e` separately and documents `--no-extensions` as “Disable extension discovery (explicit -e paths still work).” `<PI_REFERENCE_ROOT>/packages/coding-agent/src/core/resource-loader.ts:451-455,555-565` keeps only CLI-enabled paths when discovery is disabled, but `:577-581,609-615` still appends inline factories. `<PI_REFERENCE_ROOT>/packages/coding-agent/src/main.ts:521-523` supplies the host factories, and `<PI_REFERENCE_ROOT>/packages/coding-agent/src/extensions/index.ts:1-4` defines hidden inline `llama.cpp`. Thus `--no-extensions --extension <exact-path>` suppresses discovered project/user/package paths while retaining the exact explicit path and host inline built-ins.

- **Host provenance is visible, but it is not license provenance.** `<PI_REFERENCE_ROOT>/packages/coding-agent/src/core/extensions/loader.ts:446-455` gives inline factories synthetic paths such as `<inline:...>` and `<PI_REFERENCE_ROOT>/packages/coding-agent/src/core/source-info.ts:3-10,24-30` models their source/scope/origin. The built-in `<PI_REFERENCE_ROOT>/packages/coding-agent/src/extensions/llama/index.ts:42-44` registers a provider whose ID/name are `llama.cpp` in `<PI_REFERENCE_ROOT>/packages/coding-agent/src/extensions/llama/provider.ts:13,65-68`.

- **Provider IDs do not prove ownership.** `<PI_REFERENCE_ROOT>/packages/coding-agent/src/core/model-runtime.ts:32,99-103,145-148,193-217` keeps Pi-ai built-ins, native extension providers, named extension configs, and `models.json` provider IDs as separate inputs, then recomposes them. `<PI_REFERENCE_ROOT>/packages/coding-agent/src/core/provider-composer.ts:411-435` states and implements the built-in → `models.json` → extension → model-override layering. The first-party docs likewise say an extension can preserve an existing provider endpoint (`<PI_REFERENCE_ROOT>/packages/coding-agent/docs/custom-provider.md:33,119-119`), replace its model list (`:184-184`), and that `models.json` overrides apply to both built-in and extension-registered models (`<PI_REFERENCE_ROOT>/packages/coding-agent/docs/models.md:143-145,313-320,362-366`). The RPC/session formats record runtime provider/model identity (`<PI_REFERENCE_ROOT>/packages/coding-agent/docs/rpc.md:1367-1375`; `<PI_REFERENCE_ROOT>/packages/coding-agent/docs/session-format.md:85-86,218`), not a source-owner or license field.

- **License evidence remains a separate gate.** `<PI_REFERENCE_ROOT>/LICENSE:1` and the `license` fields in `<PI_REFERENCE_ROOT>/packages/coding-agent/package.json:98` and `<PI_REFERENCE_ROOT>/packages/ai/package.json:86` declare MIT for the Pi source/packages. Separately, `<PI_REFERENCE_ROOT>/packages/coding-agent/docs/llama-cpp.md:5,79` directs users to an external llama.cpp build and says the llama.cpp server performs Hugging Face model downloads. Those declarations do not establish the license/notice/source closure for a future bundled Pi artifact, the external llama.cpp executable, provider services, model repositories/GGUFs, or the complete dependency closure. The existing release conclusion remains **BLOCKED**.
