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
| Source binding | The Adapter embeds the audited source with `include_str!`; the source last changed in `e8c445d6a81d90851ac03d6aac7a4f11b6b749a3` and its current `git hash-object` is `15d6908cc30e45f8812a87c591e58799d2f7ae69`. |
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
rg -n 'HALO_PI_EXTENSION_ID|HALO_PI_EXTENSION_VERSION|HALO_PI_EXTENSION_PERMISSIONS|include_str!|--no-extensions|--extension' "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/lib.rs"
cargo tree --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter
cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p bitfun-pi-rpc-adapter extension_decision_is_redacted_one_shot_and_duplicate_request_fails_closed
```

The expected SHA-256 is the value recorded above. A mismatch, missing source,
new runtime dependency, or unreviewed license is a blocking difference.

The machine-readable inventory is
`docs/architecture/pi-first-party-extension-inventory.json`. It records the
fixed source version, source commit/tag, Git object hash, SHA-256, load
arguments, tool/event surface, host permissions, direct/transitive dependency
boundary, license evidence, and update responsibility. The audit CLI is
`product/Halo Studio/scripts/pi-extension-audit.mjs`; it is intentionally
fail-closed and returns exit code `1` while the release gate is blocked.
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

## Audit CLI contract

The audit CLI is a read-only evidence checker, not a runtime gate that starts
Pi. It only reads the manifest, repository files, Git objects, and explicitly
provided read-only evidence trees; it does not write files, install packages,
open network connections, read credentials, send prompts, or execute a Pi
binary. It returns exit code `0` only for a complete passing fixture and exit
code `1` for blocked evidence, invalid arguments, or an audit exception.

The contract tests cover the real CLI process (`--help`, blocked/pass `--json`,
unknown and missing arguments), rooted Windows path redaction, dynamic
extension imports/host capabilities, extensionless runtime inputs, structured
fail-closed extension metadata, and host closure/release-file evidence. The
current source contains 50 audit contract tests; the release matrix must be
updated only from the command's actual exit code and test count.

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
