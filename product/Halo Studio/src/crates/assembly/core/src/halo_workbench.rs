//! Halo Workbench Runtime product assembly.
//!
//! P0 deliberately selects exactly one production adapter identity. This
//! module only wires existing owners and narrow projections; runtime state and
//! policy remain in `bitfun-agent-runtime`.

use std::sync::Arc;

use async_trait::async_trait;
use bitfun_agent_runtime::halo_workbench::HaloWorkbenchRuntime;
use bitfun_pi_rpc_adapter::{
    JsonFilePiRuntimeConfigurationRepository, PiRpcAdapter, PiRpcConfig,
    PiRuntimeConfigurationService,
};
use bitfun_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiProviderReadiness, PiProviderReadinessPort,
    PiRpcPort, PiRuntimeConfigurationManagementPort, PortError, PortErrorKind, PortResult,
    WorkbenchWorkspaceFacts, WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest,
};
use sha2::{Digest, Sha256};

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

/// System-vault implementation for Halo Pi credentials. The existing
/// subscription credential store already owns the platform keychain,
/// Credential Manager and Secret Service setup; this adapter gives Pi an
/// opaque, provider-bound namespace without making Pi's `auth.json` the
/// authority.
#[derive(Debug, Clone, Copy, Default)]
pub struct PiSystemCredentialStore;

pub const PI_CREDENTIAL_REF_PREFIX: &str = "halo-pi-credential-v1-";

fn valid_pi_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value
            .chars()
            .all(|character| !character.is_control() && character != '\\')
}

fn provider_binding(provider_id: &str) -> String {
    let digest = Sha256::digest(provider_id.as_bytes());
    format!("{digest:x}")
}

fn credential_ref_for(provider_id: &str) -> String {
    format!(
        "{PI_CREDENTIAL_REF_PREFIX}{}-{}",
        provider_binding(provider_id),
        uuid::Uuid::new_v4()
    )
}

fn credential_ref_belongs_to_provider(provider_id: &str, credential_ref: &str) -> bool {
    credential_ref
        .strip_prefix(PI_CREDENTIAL_REF_PREFIX)
        .and_then(|suffix| suffix.strip_prefix(&provider_binding(provider_id)))
        .is_some_and(|suffix| suffix.starts_with('-'))
}

#[async_trait]
impl PiCredentialStorePort for PiSystemCredentialStore {
    async fn write(&self, provider_id: &str, secret: PiCredentialSecret) -> PortResult<String> {
        if !valid_pi_provider_id(provider_id) {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi provider is required",
            ));
        }
        let value = secret.into_string();
        if value.is_empty() {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi credential must not be empty",
            ));
        }
        let credential_ref = credential_ref_for(provider_id);
        bitfun_ai_adapters::subscription_auth::store::write_pi_credential(&credential_ref, &value)
            .await
            .map_err(|_| {
                PortError::new(
                    PortErrorKind::Backend,
                    "system credential store is unavailable",
                )
            })?;
        Ok(credential_ref)
    }

    async fn read(
        &self,
        provider_id: &str,
        credential_ref: &str,
    ) -> PortResult<PiCredentialSecret> {
        if !valid_pi_provider_id(provider_id)
            || !credential_ref_belongs_to_provider(provider_id, credential_ref)
        {
            return Err(PortError::new(
                PortErrorKind::PermissionDenied,
                "Pi credential provider does not match configuration",
            ));
        }
        let value =
            bitfun_ai_adapters::subscription_auth::store::read_pi_credential(credential_ref)
                .await
                .map_err(|_| {
                    PortError::new(
                        PortErrorKind::Backend,
                        "system credential store is unavailable",
                    )
                })?
                .ok_or_else(|| {
                    PortError::new(
                        PortErrorKind::NotFound,
                        "Pi credential reference is missing",
                    )
                })?;
        Ok(PiCredentialSecret::new(value))
    }

    async fn delete(&self, provider_id: &str, credential_ref: &str) -> PortResult<()> {
        if !valid_pi_provider_id(provider_id)
            || !credential_ref_belongs_to_provider(provider_id, credential_ref)
        {
            return Err(PortError::new(
                PortErrorKind::PermissionDenied,
                "Pi credential provider does not match configuration",
            ));
        }
        bitfun_ai_adapters::subscription_auth::store::delete_pi_credential(credential_ref)
            .await
            .map_err(|_| {
                PortError::new(
                    PortErrorKind::Backend,
                    "system credential store is unavailable",
                )
            })
    }
}

struct PiRpcConfiguredReadinessGate {
    configuration: Arc<dyn PiProviderReadinessPort>,
}

#[async_trait]
impl PiProviderReadinessPort for PiRpcConfiguredReadinessGate {
    async fn check(&self) -> PortResult<PiProviderReadiness> {
        self.configuration.check().await
    }
}

fn selected_pi_rpc(
    configuration: Arc<PiRuntimeConfigurationService>,
    credential_store: Arc<PiSystemCredentialStore>,
) -> Arc<dyn PiRpcPort> {
    Arc::new(PiRpcAdapter::with_config(PiRpcConfig {
        runtime_configuration: Some(configuration.clone()),
        credential_store: Some(credential_store),
        ..PiRpcConfig::default()
    }))
}

pub struct HaloWorkbenchRuntimeComponents {
    pub runtime: HaloWorkbenchRuntime,
    pub configuration: Arc<dyn PiRuntimeConfigurationManagementPort>,
    pub credential_store: Arc<dyn PiCredentialStorePort>,
}

fn pi_configuration_path() -> Result<std::path::PathBuf, String> {
    let path_manager = crate::infrastructure::try_get_path_manager_arc()
        .map_err(|error| format!("Pi configuration path is unavailable: {error}"))?;
    Ok(path_manager
        .user_config_dir()
        .join("pi-runtime-configuration.json"))
}

/// Builds the runtime and the single configuration authority used by both
/// standard and managed Pi sessions.
pub fn build_halo_workbench_runtime_components(
    workspace_service: Arc<WorkspaceService>,
) -> Result<HaloWorkbenchRuntimeComponents, String> {
    let credential_store = Arc::new(PiSystemCredentialStore);
    let repository = Arc::new(JsonFilePiRuntimeConfigurationRepository::new(
        pi_configuration_path()?,
    ));
    let configuration = Arc::new(
        PiRuntimeConfigurationService::new_without_capabilities(repository)
            .with_credential_store(credential_store.clone()),
    );
    let adapter = selected_pi_rpc(configuration.clone(), credential_store.clone());
    let runtime = HaloWorkbenchRuntime::new(
        adapter,
        Arc::new(CoreWorkbenchWorkspaceFacts::new(workspace_service)),
        Arc::new(PiRpcConfiguredReadinessGate {
            configuration: configuration.clone(),
        }),
        Arc::new(SystemProductClock),
    );
    let configuration: Arc<dyn PiRuntimeConfigurationManagementPort> = configuration;
    Ok(HaloWorkbenchRuntimeComponents {
        runtime,
        configuration,
        credential_store,
    })
}

/// Builds the sole P0 Halo Workbench Runtime composition.
///
/// The selected adapter is fixed to the Pi RPC P0 implementation;
/// there is intentionally no selector or fallback chain.
pub fn build_halo_workbench_runtime(
    workspace_service: Arc<WorkspaceService>,
) -> Result<HaloWorkbenchRuntime, String> {
    Ok(build_halo_workbench_runtime_components(workspace_service)?.runtime)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, OnceLock};

    use bitfun_runtime_ports::{
        PiCredentialSecret, PiCredentialStorePort, PiRpcCommand, PiRpcFailureKind, PiRpcPort,
        PiRpcReply, PiRpcWorkspace, PortErrorKind, WorkbenchWorkspaceFactsRequest,
    };
    use chrono::Utc;

    use super::{project_workspace_facts, PiRpcAdapter, PiSystemCredentialStore};
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

    static PI_SYSTEM_STORE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[tokio::test]
    async fn system_pi_credentials_are_provider_bound_and_delete_without_reading_secret() {
        let _lock = PI_SYSTEM_STORE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("Pi system credential test lock");
        let root = tempfile::tempdir().expect("test credential root");
        bitfun_ai_adapters::subscription_auth::set_store_path_for_test(
            root.path().join("subscription_auth.json"),
        );
        let store = PiSystemCredentialStore;
        let reference = store
            .write("openai", PiCredentialSecret::new("synthetic-system-secret"))
            .await
            .expect("synthetic credential write");
        assert!(reference.starts_with(super::PI_CREDENTIAL_REF_PREFIX));
        assert!(!reference.contains("synthetic-system-secret"));

        let secret = store
            .read("openai", &reference)
            .await
            .expect("synthetic credential read")
            .into_string();
        assert_eq!(secret, "synthetic-system-secret");
        assert_eq!(
            store
                .read("anthropic", &reference)
                .await
                .expect_err("provider mismatch must fail closed")
                .kind,
            PortErrorKind::PermissionDenied
        );

        store
            .delete("openai", &reference)
            .await
            .expect("synthetic credential delete");
        assert_eq!(
            store
                .read("openai", &reference)
                .await
                .expect_err("deleted credential must be missing")
                .kind,
            PortErrorKind::NotFound
        );
    }

    #[tokio::test]
    async fn system_pi_credentials_use_a_dedicated_store_namespace() {
        let _lock = PI_SYSTEM_STORE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("Pi system credential test lock");
        let root = tempfile::tempdir().expect("test credential root");
        let subscription_metadata = root.path().join("subscription_auth.json");
        bitfun_ai_adapters::subscription_auth::set_store_path_for_test(
            subscription_metadata.clone(),
        );

        let store = PiSystemCredentialStore;
        store
            .write("openai", PiCredentialSecret::new("synthetic-pi-secret"))
            .await
            .expect("synthetic Pi credential write");

        assert!(
            !subscription_metadata.exists(),
            "Pi credentials must not create or mutate the subscription metadata store"
        );
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
    async fn p0_selection_is_fixed_to_the_pi_rpc_adapter() {
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
    }
}
