//! Halo Workbench Runtime product assembly.
//!
//! P0 deliberately selects exactly one production adapter identity. This
//! module only wires existing owners and narrow projections; runtime state and
//! policy remain in `bitfun-agent-runtime`.

use std::sync::Arc;

use async_trait::async_trait;
use bitfun_agent_runtime::halo_workbench::HaloWorkbenchRuntime;
use bitfun_pi_rpc_adapter::PiRpcAdapter;
use bitfun_runtime_ports::{
    PiProviderReadiness, PiProviderReadinessPort, PiRpcPort, PortError, PortErrorKind, PortResult,
    WorkbenchWorkspaceFacts, WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest,
};

use crate::product_runtime::SystemProductClock;
use crate::service::remote_ssh::{canonicalize_local_workspace_root, local_workspace_roots_equal};
use crate::service::workspace::{
    is_halo_workbench_workspace_trusted, WorkspaceInfo, WorkspaceKind, WorkspaceService,
};

#[derive(Clone)]
struct CoreWorkbenchWorkspaceFacts {
    workspace_service: Arc<WorkspaceService>,
}

impl CoreWorkbenchWorkspaceFacts {
    fn new(workspace_service: Arc<WorkspaceService>) -> Self {
        Self { workspace_service }
    }
}

#[async_trait]
impl WorkbenchWorkspaceFactsPort for CoreWorkbenchWorkspaceFacts {
    async fn inspect(
        &self,
        request: WorkbenchWorkspaceFactsRequest,
    ) -> PortResult<WorkbenchWorkspaceFacts> {
        let workspace = self
            .workspace_service
            .get_workspace(&request.workspace_id)
            .await
            .ok_or_else(|| {
                PortError::new(PortErrorKind::NotFound, "workspace is not registered")
            })?;

        project_workspace_facts(&workspace, request)
    }
}

fn project_workspace_facts(
    workspace: &WorkspaceInfo,
    request: WorkbenchWorkspaceFactsRequest,
) -> PortResult<WorkbenchWorkspaceFacts> {
    if workspace.workspace_kind == WorkspaceKind::Remote {
        return Err(PortError::new(
            PortErrorKind::PermissionDenied,
            "remote workspaces are unsupported by Halo Workbench Runtime",
        ));
    }

    let (canonical_root, _) =
        canonicalize_local_workspace_root(&workspace.root_path).map_err(|_| {
            PortError::new(PortErrorKind::NotAvailable, "workspace root is unavailable")
        })?;
    let (requested_root, _) = canonicalize_local_workspace_root(&request.root).map_err(|_| {
        PortError::new(
            PortErrorKind::InvalidRequest,
            "requested workspace root is unavailable",
        )
    })?;

    if !local_workspace_roots_equal(&canonical_root, &requested_root) {
        return Err(PortError::new(
            PortErrorKind::InvalidRequest,
            "workspace request does not match the registered root",
        ));
    }

    let git_repository = workspace
        .statistics
        .as_ref()
        .and_then(|statistics| statistics.git_info.as_ref())
        .is_some_and(|git| git.is_git_repo);

    Ok(WorkbenchWorkspaceFacts {
        workspace_id: workspace.id.clone(),
        canonical_root,
        trusted: is_halo_workbench_workspace_trusted(workspace),
        git_repository,
    })
}

#[derive(Debug, Clone, Copy, Default)]
/// Probe has already established executable compatibility; provider auth and
/// model availability remain Pi-native and are reported on the first prompt.
/// This gate must not claim that a version-only check is RPC readiness: the
/// selected adapter's `Start` performs the full `get_state`/`get_entries`
/// handshake before it emits `Ready`.
struct PiRpcReadinessGate;

#[async_trait]
impl PiProviderReadinessPort for PiRpcReadinessGate {
    async fn check(&self) -> PortResult<PiProviderReadiness> {
        Ok(PiProviderReadiness { available: true })
    }
}

fn selected_pi_rpc() -> Arc<dyn PiRpcPort> {
    Arc::new(PiRpcAdapter::new())
}

/// Builds the sole P0 Halo Workbench Runtime composition.
///
/// The selected adapter is fixed to the Pi RPC P0 implementation;
/// there is intentionally no selector or fallback chain.
pub fn build_halo_workbench_runtime(
    workspace_service: Arc<WorkspaceService>,
) -> HaloWorkbenchRuntime {
    HaloWorkbenchRuntime::new(
        selected_pi_rpc(),
        Arc::new(CoreWorkbenchWorkspaceFacts::new(workspace_service)),
        Arc::new(PiRpcReadinessGate),
        Arc::new(SystemProductClock),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use bitfun_runtime_ports::{
        PiProviderReadinessPort, PiRpcCommand, PiRpcFailureKind, PiRpcPort, PiRpcReply,
        PiRpcWorkspace, PortErrorKind, WorkbenchWorkspaceFactsRequest,
    };
    use chrono::Utc;

    use super::{project_workspace_facts, PiRpcReadinessGate};
    use crate::service::workspace::{
        GitInfo, WorkspaceInfo, WorkspaceKind, WorkspaceStatistics, WorkspaceStatus, WorkspaceType,
    };

    fn workspace(root_path: PathBuf, workspace_kind: WorkspaceKind) -> WorkspaceInfo {
        WorkspaceInfo {
            id: "workspace-1".to_string(),
            name: "Workspace".to_string(),
            root_path,
            workspace_type: WorkspaceType::RustProject,
            workspace_kind,
            assistant_id: None,
            status: WorkspaceStatus::Active,
            languages: vec!["Rust".to_string()],
            opened_at: Utc::now(),
            last_accessed: Utc::now(),
            description: None,
            tags: Vec::new(),
            statistics: None,
            identity: None,
            worktree: None,
            related_paths: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn registered_local_git_workspace_projects_trusted_canonical_facts() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let mut workspace = workspace(root.path().to_path_buf(), WorkspaceKind::Normal);
        workspace.statistics = Some(WorkspaceStatistics {
            total_files: 0,
            total_directories: 0,
            total_size_bytes: 0,
            file_extensions: HashMap::new(),
            last_modified: None,
            git_info: Some(GitInfo {
                is_git_repo: true,
                current_branch: None,
                remote_url: None,
                has_uncommitted_changes: false,
                total_commits: None,
            }),
        });

        let facts = project_workspace_facts(
            &workspace,
            WorkbenchWorkspaceFactsRequest {
                workspace_id: workspace.id.clone(),
                root: root.path().to_path_buf(),
            },
        )
        .unwrap();

        assert_eq!(facts.workspace_id, workspace.id);
        assert_eq!(
            facts.canonical_root,
            dunce::canonicalize(root.path()).unwrap()
        );
        assert!(facts.trusted);
        assert!(facts.git_repository);
    }

    #[test]
    fn workspace_projection_rejects_root_substitution_and_remote_workspaces() {
        let registered = tempfile::tempdir().unwrap();
        let substituted = tempfile::tempdir().unwrap();
        let local = workspace(registered.path().to_path_buf(), WorkspaceKind::Normal);

        let mismatch = project_workspace_facts(
            &local,
            WorkbenchWorkspaceFactsRequest {
                workspace_id: local.id.clone(),
                root: substituted.path().to_path_buf(),
            },
        )
        .unwrap_err();
        assert_eq!(mismatch.kind, PortErrorKind::InvalidRequest);

        let remote = workspace(PathBuf::from("/remote/workspace"), WorkspaceKind::Remote);
        let remote_error = project_workspace_facts(
            &remote,
            WorkbenchWorkspaceFactsRequest {
                workspace_id: remote.id.clone(),
                root: remote.root_path.clone(),
            },
        )
        .unwrap_err();
        assert_eq!(remote_error.kind, PortErrorKind::PermissionDenied);
    }

    #[test]
    fn workspace_projection_respects_owner_trust_fact() {
        let root = tempfile::tempdir().unwrap();
        let mut local = workspace(root.path().to_path_buf(), WorkspaceKind::Normal);
        local.metadata.insert(
            "haloWorkbenchTrusted".to_string(),
            serde_json::Value::Bool(false),
        );

        let facts = project_workspace_facts(
            &local,
            WorkbenchWorkspaceFactsRequest {
                workspace_id: local.id.clone(),
                root: root.path().to_path_buf(),
            },
        )
        .unwrap();
        assert!(!facts.trusted);
    }

    #[tokio::test]
    async fn p0_selection_is_fixed_and_provider_readiness_defers_to_pi() {
        let adapter: Arc<dyn PiRpcPort> = Arc::new(PiRpcAdapter::with_config(
            bitfun_pi_rpc_adapter::PiRpcConfig {
                executable: Some(PathBuf::from("C:/does-not-exist/pi.exe")),
                ..bitfun_pi_rpc_adapter::PiRpcConfig::default()
            },
        ));
        let reply = adapter
            .execute(PiRpcCommand::Probe {
                generation: 7,
                workspace: PiRpcWorkspace {
                    workspace_id: "workspace-1".to_string(),
                    canonical_root: PathBuf::from("C:/workspace"),
                },
            })
            .await
            .unwrap();
        assert_eq!(
            reply,
            PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::NotInstalled,
            }
        );

        let provider_readiness = PiRpcReadinessGate.check().await.unwrap();
        assert!(provider_readiness.available);
    }
}
