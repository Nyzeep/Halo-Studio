//! Thin Tauri projection of the Halo Workbench Runtime Interface.

use halo_agent_runtime::halo_workbench::{
    HaloWorkbenchError, HaloWorkbenchIntent, HaloWorkbenchIntentReceipt,
    HaloWorkbenchIntentRequest, HaloWorkbenchSnapshot,
};
use halo_core::service::workspace::WorkspaceKind;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::api::AppState;
use crate::runtime::DesktopRuntimeContext;

pub const HALO_WORKBENCH_EVENT: &str = "halo-workbench://event";

const REMOTE_WORKSPACE_UNSUPPORTED: &str = "remote_workspace_unsupported";
const SWITCH_TO_LOCAL_WORKSPACE: &str = "switch_to_local_workspace";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchSnapshotRequest {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HaloWorkbenchSurfaceError {
    pub code: &'static str,
    pub summary: &'static str,
    pub recovery_action: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum HaloWorkbenchCommandError {
    Runtime(HaloWorkbenchError),
    Surface(HaloWorkbenchSurfaceError),
}

impl From<HaloWorkbenchError> for HaloWorkbenchCommandError {
    fn from(error: HaloWorkbenchError) -> Self {
        Self::Runtime(error)
    }
}

fn remote_workspace_error() -> HaloWorkbenchCommandError {
    HaloWorkbenchCommandError::Surface(HaloWorkbenchSurfaceError {
        code: REMOTE_WORKSPACE_UNSUPPORTED,
        summary: "Halo Workbench Runtime is not available for remote workspaces",
        recovery_action: SWITCH_TO_LOCAL_WORKSPACE,
    })
}

async fn ensure_active_workspace_is_local(
    state: &AppState,
) -> Result<(), HaloWorkbenchCommandError> {
    if state
        .workspace_service
        .get_current_workspace()
        .await
        .is_some_and(|workspace| workspace.workspace_kind == WorkspaceKind::Remote)
    {
        return Err(remote_workspace_error());
    }
    Ok(())
}

fn requested_workspace_id(request: &HaloWorkbenchIntentRequest) -> Option<&str> {
    match &request.intent {
        HaloWorkbenchIntent::OpenWorkspace { workspace } => Some(&workspace.workspace_id),
        _ => None,
    }
}

async fn ensure_intent_workspace_is_local(
    state: &AppState,
    request: &HaloWorkbenchIntentRequest,
) -> Result<(), HaloWorkbenchCommandError> {
    ensure_active_workspace_is_local(state).await?;
    let Some(workspace_id) = requested_workspace_id(request) else {
        return Ok(());
    };
    if state
        .workspace_service
        .get_workspace(workspace_id)
        .await
        .is_some_and(|workspace| workspace.workspace_kind == WorkspaceKind::Remote)
    {
        return Err(remote_workspace_error());
    }
    Ok(())
}

#[tauri::command]
pub async fn halo_workbench_runtime_snapshot(
    state: State<'_, AppState>,
    runtime: State<'_, DesktopRuntimeContext>,
    _request: HaloWorkbenchSnapshotRequest,
) -> Result<HaloWorkbenchSnapshot, HaloWorkbenchCommandError> {
    ensure_active_workspace_is_local(&state).await?;
    Ok(runtime.halo_workbench().snapshot())
}

#[tauri::command]
pub async fn halo_workbench_runtime_submit_intent(
    state: State<'_, AppState>,
    runtime: State<'_, DesktopRuntimeContext>,
    request: HaloWorkbenchIntentRequest,
) -> Result<HaloWorkbenchIntentReceipt, HaloWorkbenchCommandError> {
    ensure_intent_workspace_is_local(&state, &request).await?;
    runtime
        .halo_workbench()
        .submit(request)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_workspace_error_has_stable_recovery_fields() {
        let value = serde_json::to_value(remote_workspace_error()).expect("error serializes");
        assert_eq!(value["code"], REMOTE_WORKSPACE_UNSUPPORTED);
        assert_eq!(value["recoveryAction"], SWITCH_TO_LOCAL_WORKSPACE);
        assert!(!value.to_string().contains("connectionId"));
        assert!(!value.to_string().contains("remotePath"));
    }
}
