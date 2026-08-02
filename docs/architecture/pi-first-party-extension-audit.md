# Halo first-party Pi extension audit

Status: audit record for the issue-04 Pi RPC migration; release approval remains
blocked until the recorded source, dependency, permission, and license evidence
is reviewed against the exact product tree that will be released.

This record covers the only extension permitted on the Halo P0 path. It does
not authorize Pi's default permissions, project extensions, user extensions,
Pi packages, Provider extensions, or runtime downloads.

## Fixed artifact

| Field | Audit record |
| --- | --- |
| Extension ID | `halo-workbench-permission-gate` |
| Fixed version | `1.0.0` (`HALO_PI_EXTENSION_VERSION`) |
| Source file | `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts` |
| Source binding | The Adapter embeds the audited source with `include_str!`; the release candidate must record the source commit and `git hash-object` result. This migration is not committed, so no commit hash is asserted here. |
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
  extension path at runtime. `D:\pi-main` is read-only reference material and
  is not a source, dependency, or build input.
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

## Migration verification command set

The following are the issue-04 documentation gate commands. They must use a
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
