//! Compatibility re-exports for LSP server process lifecycle.
//!
//! The reusable LSP process owner lives in `halo-services-core`.

pub use halo_services_core::lsp::process::{
    CrashCallback, DiagnosticsCallback, LspServerProcess, ProgressCallback, TokenCreateCallback,
};
