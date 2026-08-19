# Workbench Runtime: OpenCode → Pi RPC migration map

This is the migration audit baseline for issue 04. It preserves the existing
Halo Workbench Runtime seam and replaces the selected execution adapter behind
that seam. The table is not a claim that an item is complete; an OpenCode
adapter may be retired only after its replacement semantics, reference audit,
and tests pass.

| OpenCode file / semantic | Pi RPC file / semantic | Retirement decision |
| --- | --- | --- |
| `src/crates/adapters/opencode-server-adapter/Cargo.toml` and `src/lib.rs`: the selected fail-closed OpenCode Server adapter | `src/crates/adapters/pi-rpc-adapter/Cargo.toml`, `src/lib.rs`, and `src/framing.rs`: the sole `pi-rpc-p0` process, strict LF JSONL framing, probe/start/stop, command correlation, Pi event normalization, cleanup, and first-party extension loading | Retire the whole OpenCode Server adapter after the production path and adapter contract tests are Pi-only |
| `OpenCodeServerPort`, `OpenCodeServerCommand`, `OpenCodeServerReply`, `OpenCodeServerEvent`, `OpenCodeServerFailureKind`, workspace/session/operation types | `PiRpcPort`, `PiRpcCommand`, `PiRpcReply`, `PiRpcEvent`, `PiRpcFailureKind`, `PiRpcWorkspace`, session/operation types in `contracts/runtime-ports/src/halo_workbench.rs` | Retire all OpenCode-named Halo port types; retain only Halo-local DTOs and Pi-specific internals behind the port |
| OpenCode credential-availability port and authentication failure vocabulary | `PiProviderReadinessPort`/`PiProviderReadiness` and Pi credential boundary: Halo keeps only non-sensitive selection plus `credential_ref`; the adapter never exposes auth material | Retire OpenCode credential types; retain a fail-closed readiness port without reading real credentials in this task |
| `agent-runtime/src/halo_workbench.rs`: Workbench owner drives the OpenCode port and maps server lifecycle events | Same file and module: unchanged Workbench Runtime owner drives `PiRpcPort`, consumes `agent_settled` as the only Pi settlement signal, redacts tool-call correlation, and keeps task/trust/decision/evidence/lifecycle authority | Keep the deep Workbench Runtime module; retire only OpenCode vocabulary and fallback semantics |
| `assembly/core/src/halo_workbench.rs`: `selected_opencode_server` and OpenCode readiness composition | Same assembly module with `selected_pi_rpc`, `PiRpcAdapter`, `pi-rpc-p0`, and Pi provider readiness | Retire OpenCode production registration; keep upstream `opencode-adapter` separate as non-Halo source semantics |
| Workspace manifests, lockfile, crate rules, and adapter ownership docs that register or permit `opencode-server-adapter` | Workspace manifests, lockfile, crate rules, and adapter docs registering only `pi-rpc-adapter` for Halo P0 | Remove the OpenCode Server crate registration only after `rg` reference audit is empty |
| Desktop API/runtime and Tauri command/event projection carrying OpenCode identity or server errors | Existing Tauri command/event seam with the Pi identity, Pi stable error codes, and the same Halo-local snapshot/event shapes | Keep the command/event seam and lifecycle; retire OpenCode names and transport claims |
| Web runtime types, client, store, selectors, scenes, and formal-path tests keyed to `opencode-server-1.x` | Existing Web Workbench Runtime hook and UI keyed to `pi-rpc`, with no Pi session/entry/tool-call/raw payload leakage | Keep the UI structure and runtime hook; retire OpenCode HTTP/SSE assertions and identity |
| OpenCode HTTP/SSE, `opencode serve`, auth, health, and dispose checks in active docs/tests | Pi executable resolver (`where.exe pi`, `pi.cmd`, `pi.exe`, PowerShell/npm shim), `pi --mode rpc`, `get_state`/`get_entries` readiness, LF JSONL, extension UI, abort, and cleanup checks | Retire from active P0 checks; preserve only clearly marked historical/comparison evidence |
| Historical OpenCode implementation and ADR-0071 | ADR-0072 accepted as the active decision; ADR-0071 and legacy OpenCode-native UI material explicitly marked superseded/history | Do not delete GitHub/history evidence; do not register it as a production fallback |

## Historical OpenCode scan allowlist

OpenCode terms are permitted in active documentation only when the containing
material is explicitly historical, comparison, or superseded: ADR-0071, the
old issue-07 and issue-14 documents, the historical product-requirements file,
historical sections of `docs/testing/core-rebuild-verification.md`, and the
OpenCode rows of this migration map. They are not a production fallback.

```powershell
rg -n -i 'opencode serve|OpenCode Server|HTTP/SSE|opencode-server-adapter|agent-opencode|opencode.rs' docs "product/Halo Studio" --glob '!docs/adr/0071-use-opencode-server-as-the-p0-managed-execution-adapter.md' --glob '!docs/requirements/halo-tauri-product-migration/issues/07-probe-and-start-opencode-1x.md' --glob '!docs/requirements/halo-tauri-product-migration/issues/14-complete-real-opencode-native-ui-acceptance.md' --glob '!docs/requirements/2026-07-24-halo-studio-pi-opencode-product-requirements.md' --glob '!docs/testing/core-rebuild-verification.md' --glob '!docs/architecture/pi-rpc-workbench-migration.md'
```

Any hit outside that allowlist must be classified and removed from the active
P0 path, or the document must be explicitly marked historical before review.

## Audit gates

1. The Pi port and fake-Pi contract tests cover framing, response IDs,
   `get_state`, `get_entries`/`since`, prompt/follow-up/abort, message/tool
   events, `agent_settled`, EOF, protocol failure, and extension fail-closed
   behavior.
2. The Workbench Runtime and Tauri/Web seams continue to expose only Halo-local
   state, with trust, task state, permissions, redaction, evidence, and
   lifecycle owned by Halo.
3. Production assembly has exactly one P0 identity: `pi-rpc-p0`; no selector,
   HTTP/SSE transport, OpenCode Server adapter, Pi TUI, or PiServer fallback is
   registered.
4. Only after these gates and the verification matrix pass may the OpenCode
   Server adapter be retired by a normal patch.
