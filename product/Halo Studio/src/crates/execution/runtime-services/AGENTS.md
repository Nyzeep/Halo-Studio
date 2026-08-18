# runtime-services Agent Guide

Scope: this guide applies to `src/crates/execution/runtime-services`.

`halo-runtime-services` owns typed runtime service assembly. It connects
runtime-facing ports to injected providers without becoming a concrete platform
implementation layer.

## Guardrails

- Depend on `halo-runtime-ports`; avoid dependencies on `halo-core`, app
  crates, Tauri, concrete desktop adapters, or product UI.
- Builders should assemble explicit typed service bundles and capability
  availability. Do not introduce untyped maps, global mutable registries, or
  implicit service lookup.
- Marker ports may express optional capability availability only; they must not
  perform IO, own lifecycle, or hide concrete provider behavior.
- Fake/test providers may live here only when they protect port behavior without
  pulling product runtime dependencies.
- Concrete filesystem, terminal, network, Git, MCP, remote, or AI provider
  implementations belong in provider crates or app adapters.

## Verification

```bash
cargo test -p halo-runtime-services
node scripts/check-core-boundaries.mjs
```

For documentation-only changes, run `git diff --check`.
