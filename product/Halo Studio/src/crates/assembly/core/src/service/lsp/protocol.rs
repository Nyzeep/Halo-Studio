//! Compatibility re-exports for LSP protocol encoding and decoding.
//!
//! The reusable protocol helpers live in `halo-services-core`.

pub use halo_services_core::lsp::protocol::{
    create_notification, create_request, extract_result, read_message, write_message,
};
