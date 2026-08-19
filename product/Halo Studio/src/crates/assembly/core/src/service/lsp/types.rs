//! Compatibility re-exports for LSP protocol and plugin manifest DTOs.
//!
//! The shared contract owner is `halo-core-types`; this legacy path remains
//! for downstream callers that import through `halo_core::service::lsp`.

pub use halo_core_types::lsp::*;
