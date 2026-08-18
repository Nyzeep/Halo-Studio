//! Compatibility adapter for MiniApp host primitive dispatch.
//!
//! Concrete fs/shell/net/os dispatch lives in `halo-services-integrations`.

use crate::miniapp::types::MiniAppPermissions;
use crate::util::errors::{HaloError, HaloResult};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub use halo_services_integrations::miniapp::host_dispatch::is_host_primitive;

pub async fn dispatch_host(
    perms: &MiniAppPermissions,
    app_id: &str,
    app_data_dir: &Path,
    workspace_dir: Option<&Path>,
    granted_paths: &[PathBuf],
    method: &str,
    params: Value,
) -> HaloResult<Value> {
    halo_services_integrations::miniapp::host_dispatch::dispatch_host(
        perms,
        app_id,
        app_data_dir,
        workspace_dir,
        granted_paths,
        method,
        params,
    )
    .await
    .map_err(map_host_dispatch_error)
}

fn map_host_dispatch_error(
    err: halo_services_integrations::miniapp::host_dispatch::MiniAppHostDispatchError,
) -> HaloError {
    use halo_services_integrations::miniapp::host_dispatch::MiniAppHostDispatchErrorKind;

    match err.kind() {
        MiniAppHostDispatchErrorKind::Parse => HaloError::parse(err.message().to_string()),
        MiniAppHostDispatchErrorKind::Validation => {
            HaloError::validation(err.message().to_string())
        }
        MiniAppHostDispatchErrorKind::Io => HaloError::io(err.message().to_string()),
        MiniAppHostDispatchErrorKind::Service => HaloError::service(err.message().to_string()),
    }
}
