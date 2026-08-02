//! Isolated Tauri projection for Halo Pi configuration and credentials.
//!
//! The credential write command is intentionally separate from configuration
//! commands. It accepts a one-shot secret and returns only an opaque reference;
//! no command in this module serializes the secret or the full base URL.

use std::fmt;

use bitfun_runtime_ports::{
    PiCredentialSecret, PiProviderReadiness, PiRuntimeConfiguration, PiRuntimeConfigurationView,
    PortError, PortErrorKind,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::runtime::DesktopRuntimeContext;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiCredentialWriteRequest {
    pub provider_id: String,
    pub secret: String,
}

impl fmt::Debug for PiCredentialWriteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PiCredentialWriteRequest")
            .field("provider_id", &self.provider_id)
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiCredentialWriteResponse {
    pub credential_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiCredentialDeleteRequest {
    pub provider_id: String,
    pub credential_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiRuntimeConfigurationRequest {
    pub configuration: PiRuntimeConfiguration,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiRuntimeConfigurationEmptyRequest {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiProviderReadinessResponse {
    pub available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiConfigurationCommandError {
    pub code: &'static str,
    pub summary: &'static str,
    pub recovery_action: &'static str,
}

fn map_error(error: PortError) -> PiConfigurationCommandError {
    match error.kind {
        PortErrorKind::InvalidRequest => PiConfigurationCommandError {
            code: "pi_configuration_invalid",
            summary: "The Pi configuration is invalid",
            recovery_action: "correct_configuration",
        },
        PortErrorKind::NotFound => PiConfigurationCommandError {
            code: "pi_configuration_missing",
            summary: "The requested Pi configuration or credential was not found",
            recovery_action: "configure_provider",
        },
        PortErrorKind::PermissionDenied => PiConfigurationCommandError {
            code: "pi_configuration_denied",
            summary: "The Pi credential does not belong to the selected provider",
            recovery_action: "configure_provider",
        },
        PortErrorKind::Backend => PiConfigurationCommandError {
            code: "pi_configuration_store_unavailable",
            summary: "The system credential or configuration store is unavailable",
            recovery_action: "retry",
        },
        _ => PiConfigurationCommandError {
            code: "pi_configuration_unavailable",
            summary: "The Pi configuration service is unavailable",
            recovery_action: "retry",
        },
    }
}

#[tauri::command]
pub async fn halo_pi_credential_write(
    runtime: State<'_, DesktopRuntimeContext>,
    request: PiCredentialWriteRequest,
) -> Result<PiCredentialWriteResponse, PiConfigurationCommandError> {
    let credential_ref = runtime
        .pi_configuration_store()
        .write(
            &request.provider_id,
            PiCredentialSecret::new(request.secret),
        )
        .await
        .map_err(map_error)?;
    Ok(PiCredentialWriteResponse { credential_ref })
}

/// Deletes one provider-bound credential reference. The secret is never
/// returned to the Renderer, and a provider/reference mismatch fails closed
/// inside the credential-store port.
#[tauri::command]
pub async fn halo_pi_credential_delete(
    runtime: State<'_, DesktopRuntimeContext>,
    request: PiCredentialDeleteRequest,
) -> Result<(), PiConfigurationCommandError> {
    runtime
        .pi_configuration_store()
        .delete(&request.provider_id, &request.credential_ref)
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn halo_pi_configuration_snapshot(
    runtime: State<'_, DesktopRuntimeContext>,
    _request: PiRuntimeConfigurationEmptyRequest,
) -> Result<Option<PiRuntimeConfigurationView>, PiConfigurationCommandError> {
    runtime
        .pi_configuration()
        .public_configuration()
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn halo_pi_configuration_create(
    runtime: State<'_, DesktopRuntimeContext>,
    request: PiRuntimeConfigurationRequest,
) -> Result<(), PiConfigurationCommandError> {
    runtime
        .pi_configuration()
        .create_configuration(request.configuration)
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn halo_pi_configuration_update(
    runtime: State<'_, DesktopRuntimeContext>,
    request: PiRuntimeConfigurationRequest,
) -> Result<(), PiConfigurationCommandError> {
    runtime
        .pi_configuration()
        .update_configuration(request.configuration)
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn halo_pi_configuration_delete(
    runtime: State<'_, DesktopRuntimeContext>,
    _request: PiRuntimeConfigurationEmptyRequest,
) -> Result<(), PiConfigurationCommandError> {
    runtime
        .pi_configuration()
        .delete_configuration()
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn halo_pi_configuration_rollback(
    runtime: State<'_, DesktopRuntimeContext>,
    _request: PiRuntimeConfigurationEmptyRequest,
) -> Result<(), PiConfigurationCommandError> {
    runtime
        .pi_configuration()
        .rollback_configuration()
        .await
        .map_err(map_error)
}

#[tauri::command]
pub async fn halo_pi_configuration_readiness(
    runtime: State<'_, DesktopRuntimeContext>,
    _request: PiRuntimeConfigurationEmptyRequest,
) -> Result<PiProviderReadinessResponse, PiConfigurationCommandError> {
    let readiness: PiProviderReadiness = runtime
        .pi_configuration()
        .check()
        .await
        .map_err(map_error)?;
    Ok(PiProviderReadinessResponse {
        available: readiness.available,
    })
}
