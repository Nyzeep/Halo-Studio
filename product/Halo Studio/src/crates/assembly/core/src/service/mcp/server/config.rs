//! MCP server configuration types.

use crate::util::errors::HaloError;

use halo_services_integrations::mcp::server::MCPServerConfigValidationError;
pub use halo_services_integrations::mcp::server::{
    MCPServerConfig, MCPServerOAuthConfig, MCPServerTransport, MCPServerXaaConfig,
};

impl From<MCPServerConfigValidationError> for HaloError {
    fn from(error: MCPServerConfigValidationError) -> Self {
        Self::Configuration(error.to_string())
    }
}
