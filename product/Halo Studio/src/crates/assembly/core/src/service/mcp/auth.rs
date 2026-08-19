//! OAuth support for remote MCP servers.
//!
//! The owner implementation lives in `halo-services-integrations`. This
//! module keeps the legacy core entrypoints and injects the product data directory.

use async_trait::async_trait;
use rmcp::transport::auth::{AuthorizationManager, CredentialStore, StoredCredentials};
use std::path::PathBuf;

use crate::infrastructure::try_get_path_manager_arc;
use crate::service::mcp::server::MCPServerConfig;
use crate::util::errors::{HaloError, HaloResult};

pub use halo_services_integrations::mcp::auth::{
    MCPRemoteOAuthSessionSnapshot, MCPRemoteOAuthStatus, PreparedMCPRemoteOAuthAuthorization,
};

fn oauth_data_dir() -> HaloResult<PathBuf> {
    Ok(try_get_path_manager_arc()?.user_data_dir())
}

pub fn map_auth_error(error: impl ToString) -> HaloError {
    HaloError::MCPError(format!("OAuth error: {}", error.to_string()))
}

#[deprecated(
    since = "0.2.12",
    note = "use halo_services_integrations::mcp::auth::MCPRemoteOAuthCredentialVault with an injected data directory"
)]
pub struct MCPRemoteOAuthCredentialVault {
    inner: halo_services_integrations::mcp::auth::MCPRemoteOAuthCredentialVault,
}

#[allow(deprecated)]
impl MCPRemoteOAuthCredentialVault {
    pub fn new() -> HaloResult<Self> {
        Ok(Self {
            inner: halo_services_integrations::mcp::auth::MCPRemoteOAuthCredentialVault::new(
                oauth_data_dir()?,
            ),
        })
    }

    pub async fn load(&self, server_id: &str) -> anyhow::Result<Option<StoredCredentials>> {
        self.inner.load(server_id).await
    }

    pub async fn store(
        &self,
        server_id: &str,
        credentials: &StoredCredentials,
    ) -> anyhow::Result<()> {
        self.inner.store(server_id, credentials).await
    }

    pub async fn clear(&self, server_id: &str) -> anyhow::Result<()> {
        self.inner.clear(server_id).await
    }
}

#[derive(Clone)]
#[deprecated(
    since = "0.2.12",
    note = "use halo_services_integrations::mcp::auth::MCPRemoteOAuthCredentialStore with an injected data directory"
)]
pub struct MCPRemoteOAuthCredentialStore {
    server_id: String,
}

#[allow(deprecated)]
impl MCPRemoteOAuthCredentialStore {
    pub fn new(server_id: impl Into<String>) -> Self {
        Self {
            server_id: server_id.into(),
        }
    }
}

#[allow(deprecated)]
#[async_trait]
impl CredentialStore for MCPRemoteOAuthCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, rmcp::transport::auth::AuthError> {
        MCPRemoteOAuthCredentialVault::new()
            .map_err(|error| rmcp::transport::auth::AuthError::InternalError(error.to_string()))?
            .load(&self.server_id)
            .await
            .map_err(|error| rmcp::transport::auth::AuthError::InternalError(error.to_string()))
    }

    async fn save(
        &self,
        credentials: StoredCredentials,
    ) -> Result<(), rmcp::transport::auth::AuthError> {
        MCPRemoteOAuthCredentialVault::new()
            .map_err(|error| rmcp::transport::auth::AuthError::InternalError(error.to_string()))?
            .store(&self.server_id, &credentials)
            .await
            .map_err(|error| rmcp::transport::auth::AuthError::InternalError(error.to_string()))
    }

    async fn clear(&self) -> Result<(), rmcp::transport::auth::AuthError> {
        MCPRemoteOAuthCredentialVault::new()
            .map_err(|error| rmcp::transport::auth::AuthError::InternalError(error.to_string()))?
            .clear(&self.server_id)
            .await
            .map_err(|error| rmcp::transport::auth::AuthError::InternalError(error.to_string()))
    }
}

pub async fn has_stored_oauth_credentials(server_id: &str) -> HaloResult<bool> {
    halo_services_integrations::mcp::auth::has_stored_oauth_credentials(
        oauth_data_dir()?,
        server_id,
    )
    .await
    .map_err(map_auth_error)
}

pub async fn clear_stored_oauth_credentials(server_id: &str) -> HaloResult<()> {
    halo_services_integrations::mcp::auth::clear_stored_oauth_credentials(
        oauth_data_dir()?,
        server_id,
    )
    .await
    .map_err(map_auth_error)
}

pub async fn build_authorization_manager(
    server_id: &str,
    server_url: &str,
) -> HaloResult<(AuthorizationManager, bool)> {
    halo_services_integrations::mcp::auth::build_authorization_manager(
        oauth_data_dir()?,
        server_id,
        server_url,
    )
    .await
    .map_err(map_auth_error)
}

pub async fn prepare_remote_oauth_authorization(
    config: &MCPServerConfig,
) -> HaloResult<PreparedMCPRemoteOAuthAuthorization> {
    halo_services_integrations::mcp::auth::prepare_remote_oauth_authorization(
        oauth_data_dir()?,
        config,
    )
    .await
    .map_err(map_auth_error)
}
