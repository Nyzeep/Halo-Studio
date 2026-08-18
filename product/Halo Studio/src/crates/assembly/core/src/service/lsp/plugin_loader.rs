//! Compatibility re-exports for LSP plugin package loading.
//!
//! The reusable package loader lives in `halo-services-core`; this legacy path
//! remains for downstream callers that import through `halo_core::service::lsp`.

pub use halo_services_core::lsp::plugin_loader::PluginLoader;
