//! Compatibility re-exports for LSP plugin registry rules.
//!
//! The pure registry owner is `halo-services-core`; this legacy path remains
//! for downstream callers that import through `halo_core::service::lsp`.

pub use halo_services_core::lsp::{
    LspPluginRegistryError, LspSupportedExtensions, PluginRegistry,
};
