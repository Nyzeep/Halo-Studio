//! Halo Workbench Runtime product assembly.
//!
//! P0 deliberately selects exactly one production adapter identity. This
//! module only wires existing owners and narrow projections; runtime state and
//! policy remain in `halo-agent-runtime`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use halo_agent_runtime::halo_workbench::{
    HaloWorkbenchInterruptionHistoryPort, HaloWorkbenchRuntime, HaloWorkbenchSessionMode,
    HaloWorkbenchSessionPhase, HaloWorkbenchSessionSnapshot,
};
use halo_pi_rpc_adapter::{
    JsonFilePiRuntimeConfigurationRepository, PiRpcAdapter, PiRpcConfig, PiRpcManagedExecutor,
    PiRuntimeConfigurationService,
};
use halo_runtime_ports::{
    ManagedExecutorKind, PiCredentialSecret, PiCredentialStorePort, PiProviderReadiness,
    PiProviderReadinessPort, PiRpcPort, PiRuntimeConfigurationManagementPort, PortError,
    PortErrorKind, PortResult, WorkbenchDeliveryAttribution, WorkbenchDeliveryAttributionKind,
    WorkbenchDeliveryEvidence, WorkbenchDeliveryEvidencePort, WorkbenchDeliveryEvidenceRequest,
    WorkbenchDeliveryFingerprint, WorkbenchDeliveryFingerprintRequest, WorkbenchTaskBaseline,
    WorkbenchTaskBaselinePort, WorkbenchTaskBaselineRequest, WorkbenchWorkspaceFacts,
    WorkbenchWorkspaceFactsPort, WorkbenchWorkspaceFactsRequest, WorkbenchWorkspaceTrustRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::product_runtime::SystemProductClock;
use crate::service::git::{GitDiffParams, GitFileStatus, GitService, GitStatus};
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

    async fn confirm_managed_trust(
        &self,
        request: WorkbenchWorkspaceTrustRequest,
    ) -> PortResult<WorkbenchWorkspaceFacts> {
        let facts = self
            .inspect(WorkbenchWorkspaceFactsRequest {
                workspace_id: request.workspace_id.clone(),
                root: request.root.clone(),
            })
            .await?;
        if !facts.git_repository {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "managed execution requires a Git workspace",
            ));
        }
        self.workspace_service
            .confirm_halo_workbench_workspace_trust(&request.workspace_id, &request.root)
            .await
            .map_err(|_| {
                PortError::new(
                    PortErrorKind::PermissionDenied,
                    "workspace trust could not be persisted",
                )
            })?;
        let facts = self
            .inspect(WorkbenchWorkspaceFactsRequest {
                workspace_id: request.workspace_id,
                root: request.root,
            })
            .await?;
        Ok(facts)
    }
}

#[derive(Clone, Copy, Default)]
struct CoreWorkbenchTaskBaseline;

const MAX_BASELINE_CHANGED_FILES: usize = 4096;
const MAX_BASELINE_CONTENT_BYTES: u64 = 64 * 1024 * 1024;

#[async_trait]
impl WorkbenchTaskBaselinePort for CoreWorkbenchTaskBaseline {
    async fn capture(
        &self,
        request: WorkbenchTaskBaselineRequest,
    ) -> PortResult<WorkbenchTaskBaseline> {
        let (canonical_root, _) = canonicalize_local_workspace_root(&request.canonical_root)
            .map_err(|_| {
                PortError::new(PortErrorKind::NotAvailable, "workspace root is unavailable")
            })?;
        let head = GitService::resolve_revision(&canonical_root, "HEAD")
            .await
            .map_err(|_| {
                PortError::new(PortErrorKind::Backend, "Git HEAD could not be resolved")
            })?;
        let status = GitService::get_status(&canonical_root).await.map_err(|_| {
            PortError::new(PortErrorKind::Backend, "Git status could not be captured")
        })?;
        let working_tree_fingerprint =
            capture_working_tree_fingerprint(&canonical_root, &head, &status).map_err(|_| {
                PortError::new(
                    PortErrorKind::Backend,
                    "Git working-tree fingerprint could not be captured",
                )
            })?;
        let mut existing_changed_files = status
            .staged
            .iter()
            .chain(status.unstaged.iter())
            .map(|file| file.path.clone())
            .chain(status.untracked.iter().cloned())
            .chain(status.conflicts.iter().cloned())
            .collect::<Vec<_>>();
        existing_changed_files.sort();
        existing_changed_files.dedup();
        Ok(WorkbenchTaskBaseline {
            head,
            canonical_root,
            existing_changed_files,
            working_tree_fingerprint,
            captured_at_ms: chrono::Utc::now().timestamp_millis(),
        })
    }
}

#[derive(Clone, Copy, Default)]
struct CoreWorkbenchDeliveryEvidence;

const MAX_DELIVERY_DIFF_PREVIEW_BYTES: usize = 64 * 1024;

impl CoreWorkbenchDeliveryEvidence {
    async fn capture_fingerprint_impl(
        canonical_root: &Path,
    ) -> PortResult<(String, WorkbenchDeliveryFingerprint)> {
        let head = GitService::resolve_revision(canonical_root, "HEAD")
            .await
            .map_err(|_| {
                PortError::new(PortErrorKind::Backend, "Git HEAD could not be resolved")
            })?;
        let status = GitService::get_status(canonical_root).await.map_err(|_| {
            PortError::new(PortErrorKind::Backend, "Git status could not be captured")
        })?;
        let working_tree_fingerprint =
            capture_working_tree_fingerprint(canonical_root, &head, &status).map_err(|_| {
                PortError::new(
                    PortErrorKind::Backend,
                    "Git working-tree fingerprint could not be captured",
                )
            })?;
        let changed_files = collect_changed_files(&status);
        Ok((
            head.clone(),
            WorkbenchDeliveryFingerprint {
                head,
                changed_files,
                working_tree_fingerprint,
                captured_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        ))
    }
}

#[async_trait]
impl WorkbenchDeliveryEvidencePort for CoreWorkbenchDeliveryEvidence {
    async fn capture_fingerprint(
        &self,
        request: WorkbenchDeliveryFingerprintRequest,
    ) -> PortResult<WorkbenchDeliveryFingerprint> {
        let (canonical_root, _) = canonicalize_local_workspace_root(&request.canonical_root)
            .map_err(|_| {
                PortError::new(PortErrorKind::NotAvailable, "workspace root is unavailable")
            })?;
        let (_, fingerprint) = Self::capture_fingerprint_impl(&canonical_root).await?;
        Ok(fingerprint)
    }

    async fn capture(
        &self,
        request: WorkbenchDeliveryEvidenceRequest,
    ) -> PortResult<WorkbenchDeliveryEvidence> {
        let (canonical_root, _) = canonicalize_local_workspace_root(&request.canonical_root)
            .map_err(|_| {
                PortError::new(PortErrorKind::NotAvailable, "workspace root is unavailable")
            })?;
        let (head, fingerprint) = Self::capture_fingerprint_impl(&canonical_root).await?;
        let diff_preview = capture_bounded_diff_preview(&canonical_root)
            .await
            .map_err(|_| {
                PortError::new(
                    PortErrorKind::Backend,
                    "Git diff preview could not be captured",
                )
            })?;
        let attribution = attribute_changes(&request, fingerprint.changed_files.as_slice());
        Ok(WorkbenchDeliveryEvidence {
            captured_at_ms: fingerprint.captured_at_ms,
            head,
            working_tree_fingerprint: fingerprint.working_tree_fingerprint,
            changed_files: fingerprint.changed_files,
            diff_preview,
            attribution,
        })
    }
}

fn collect_changed_files(status: &GitStatus) -> Vec<String> {
    let mut changed_files = status
        .staged
        .iter()
        .chain(status.unstaged.iter())
        .map(|file| file.path.clone())
        .chain(status.untracked.iter().cloned())
        .chain(status.conflicts.iter().cloned())
        .collect::<Vec<_>>();
    changed_files.sort();
    changed_files.dedup();
    changed_files.truncate(MAX_BASELINE_CHANGED_FILES);
    changed_files
}

async fn capture_bounded_diff_preview(root: &Path) -> Result<String, String> {
    let diff = GitService::get_diff(
        root,
        &GitDiffParams {
            source: Some("HEAD".to_string()),
            target: None,
            files: None,
            staged: None,
            stat: None,
            review_safe: Some(true),
        },
    )
    .await
    .map_err(|error| format!("capture delivery diff preview: {error}"))?;
    Ok(truncate_utf8(&diff, MAX_DELIVERY_DIFF_PREVIEW_BYTES))
}

fn attribute_changes(
    request: &WorkbenchDeliveryEvidenceRequest,
    final_changed_files: &[String],
) -> Vec<WorkbenchDeliveryAttribution> {
    let baseline: BTreeSet<&str> = request
        .baseline
        .existing_changed_files
        .iter()
        .map(String::as_str)
        .collect();
    let settled: Option<BTreeSet<&str>> = request.settled.as_ref().map(|fingerprint| {
        fingerprint
            .changed_files
            .iter()
            .map(String::as_str)
            .collect()
    });
    final_changed_files
        .iter()
        .map(|file| {
            let kind = if baseline.contains(file.as_str()) {
                WorkbenchDeliveryAttributionKind::ExistingUserModification
            } else if settled
                .as_ref()
                .is_some_and(|set| !set.contains(file.as_str()))
            {
                WorkbenchDeliveryAttributionKind::ManualIntervention
            } else {
                WorkbenchDeliveryAttributionKind::TaskModification
            };
            WorkbenchDeliveryAttribution {
                path: file.clone(),
                kind,
            }
        })
        .collect()
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}
fn capture_working_tree_fingerprint(
    root: &Path,
    head: &str,
    status: &GitStatus,
) -> Result<String, String> {
    let mut states = BTreeMap::<String, BTreeSet<String>>::new();
    for file in &status.staged {
        add_git_status_state(&mut states, file, "staged");
    }
    for file in &status.unstaged {
        add_git_status_state(&mut states, file, "unstaged");
    }
    for path in &status.untracked {
        states
            .entry(path.clone())
            .or_default()
            .insert("untracked".to_string());
    }
    for path in &status.conflicts {
        states
            .entry(path.clone())
            .or_default()
            .insert("conflict".to_string());
    }

    if states.len() > MAX_BASELINE_CHANGED_FILES {
        return Err("Git working-tree change count exceeds the baseline limit".to_string());
    }

    let repository = git2::Repository::open(root)
        .map_err(|error| format!("open Git repository for baseline: {error}"))?;
    let index = repository
        .index()
        .map_err(|error| format!("read Git index for baseline: {error}"))?;
    let mut content_budget = 0_u64;
    let mut records = Vec::with_capacity(states.len());

    for (path, path_states) in states {
        let relative_path = Path::new(&path);
        if path.ends_with('/') {
            return Err(format!(
                "untracked directory cannot be bounded safely: {path}"
            ));
        }

        let mut record = Vec::new();
        hash_baseline_part(&mut record, b"path", path.as_bytes());
        for state in &path_states {
            hash_baseline_part(&mut record, b"state", state.as_bytes());
        }

        if path_states.contains("staged") || path_states.contains("conflict") {
            append_index_fingerprint(&mut record, &index, relative_path)?;
        }

        if path_states.contains("staged")
            || path_states.contains("unstaged")
            || path_states.contains("untracked")
            || path_states.contains("conflict")
        {
            let worktree_digest = hash_worktree_path(root, relative_path, &mut content_budget)?;
            hash_baseline_part(&mut record, b"worktree", worktree_digest.as_bytes());
        }
        records.push(record);
    }

    let mut digest = sha2::Sha256::new();
    hash_digest_part(&mut digest, b"halo-workbench-baseline-v1");
    hash_digest_part(&mut digest, head.as_bytes());
    for record in records {
        hash_digest_part(&mut digest, &record);
    }
    Ok(hex::encode(digest.finalize()))
}

fn add_git_status_state(
    states: &mut BTreeMap<String, BTreeSet<String>>,
    file: &GitFileStatus,
    phase: &str,
) {
    let entry = states.entry(file.path.clone()).or_default();
    entry.insert(phase.to_string());
    entry.insert(format!(
        "status:{phase}:{}:{}:{}",
        file.status,
        file.index_status.as_deref().unwrap_or("-"),
        file.workdir_status.as_deref().unwrap_or("-")
    ));
}

fn append_index_fingerprint(
    record: &mut Vec<u8>,
    index: &git2::Index,
    path: &Path,
) -> Result<(), String> {
    if let Some(entry) = index.get_path(path, 0) {
        hash_baseline_part(record, b"index-oid", entry.id.to_string().as_bytes());
        hash_baseline_part(record, b"index-mode", entry.mode.to_string().as_bytes());
    } else {
        hash_baseline_part(record, b"index-oid", b"missing-stage-zero");
    }

    if index.has_conflicts() {
        if let Ok(conflict) = index.conflict_get(path) {
            for (stage, entry) in [
                (b"ancestor".as_slice(), conflict.ancestor),
                (b"ours".as_slice(), conflict.our),
                (b"theirs".as_slice(), conflict.their),
            ] {
                if let Some(entry) = entry {
                    hash_baseline_part(record, stage, entry.id.to_string().as_bytes());
                    hash_baseline_part(record, b"conflict-mode", entry.mode.to_string().as_bytes());
                } else {
                    hash_baseline_part(record, stage, b"missing");
                }
            }
        }
    }
    Ok(())
}

fn hash_worktree_path(
    root: &Path,
    relative_path: &Path,
    content_budget: &mut u64,
) -> Result<String, String> {
    let path = root.join(relative_path);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(hex::encode(sha2::Sha256::digest(b"absent\0")));
        }
        Err(error) => return Err(format!("read working-tree metadata: {error}")),
    };
    let mut digest = sha2::Sha256::new();
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(&path)
            .map_err(|error| format!("read working-tree symlink: {error}"))?;
        digest.update(b"symlink\0");
        digest.update(target.to_string_lossy().as_bytes());
    } else if metadata.is_file() {
        let size = metadata.len();
        *content_budget = content_budget
            .checked_add(size)
            .ok_or_else(|| "working-tree content budget overflowed".to_string())?;
        if *content_budget > MAX_BASELINE_CONTENT_BYTES {
            return Err("working-tree content exceeds the baseline limit".to_string());
        }
        digest.update(b"file\0");
        let mut file = std::fs::File::open(&path)
            .map_err(|error| format!("open working-tree file: {error}"))?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("read working-tree file: {error}"))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    } else if metadata.is_dir() {
        return Err(format!(
            "working-tree path is a directory and cannot be bounded safely: {}",
            relative_path.display()
        ));
    } else {
        digest.update(b"other\0");
    }
    Ok(hex::encode(digest.finalize()))
}

fn hash_baseline_part(record: &mut Vec<u8>, label: &[u8], value: &[u8]) {
    record.extend_from_slice(&(label.len() as u64).to_le_bytes());
    record.extend_from_slice(label);
    record.extend_from_slice(&(value.len() as u64).to_le_bytes());
    record.extend_from_slice(value);
}

fn hash_digest_part(digest: &mut sha2::Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value);
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
        halo_ai_adapters::subscription_auth::store::write_pi_credential(&credential_ref, &value)
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
        let value = halo_ai_adapters::subscription_auth::store::read_pi_credential(credential_ref)
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
        halo_ai_adapters::subscription_auth::store::delete_pi_credential(credential_ref)
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

const HALO_WORKBENCH_INTERRUPTION_HISTORY_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HaloWorkbenchInterruptionHistoryFile {
    schema_version: u32,
    sessions: Vec<HaloWorkbenchSessionSnapshot>,
}

struct JsonFileHaloWorkbenchInterruptionHistory {
    path: PathBuf,
}

impl JsonFileHaloWorkbenchInterruptionHistory {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension("bak")
    }

    fn read_file(&self, path: &Path) -> PortResult<Option<HaloWorkbenchInterruptionHistoryFile>> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                return Err(PortError::new(
                    PortErrorKind::Backend,
                    "Workbench interruption history could not be read",
                ));
            }
        };
        serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "stored Workbench interruption history is invalid",
            )
        })
    }
}

impl HaloWorkbenchInterruptionHistoryPort for JsonFileHaloWorkbenchInterruptionHistory {
    fn load_interrupted_sessions(&self) -> PortResult<Vec<HaloWorkbenchSessionSnapshot>> {
        let history = match self.read_file(&self.path)? {
            Some(history) => history,
            None => match self.read_file(&self.backup_path())? {
                Some(history) => history,
                None => return Ok(Vec::new()),
            },
        };
        if history.schema_version != HALO_WORKBENCH_INTERRUPTION_HISTORY_SCHEMA_VERSION {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "stored Workbench interruption history has an unsupported schema",
            ));
        }
        Ok(history.sessions)
    }

    fn replace_interrupted_sessions(
        &self,
        sessions: Vec<HaloWorkbenchSessionSnapshot>,
    ) -> PortResult<()> {
        if sessions.iter().any(|session| {
            session.mode != HaloWorkbenchSessionMode::Managed
                || session.phase != HaloWorkbenchSessionPhase::Interrupted
                || !session.messages.is_empty()
                || !session.activities.is_empty()
        }) {
            return Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "Workbench interruption history must contain only interrupted managed sessions",
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            PortError::new(
                PortErrorKind::Backend,
                "Workbench interruption history path has no parent",
            )
        })?;
        fs::create_dir_all(parent).map_err(|_| {
            PortError::new(
                PortErrorKind::Backend,
                "Workbench interruption history could not be prepared",
            )
        })?;
        let bytes = serde_json::to_vec(&HaloWorkbenchInterruptionHistoryFile {
            schema_version: HALO_WORKBENCH_INTERRUPTION_HISTORY_SCHEMA_VERSION,
            sessions,
        })
        .map_err(|_| {
            PortError::new(
                PortErrorKind::Backend,
                "Workbench interruption history could not be encoded",
            )
        })?;
        let temporary = parent.join(format!(
            ".halo-workbench-interruption-{}.tmp",
            Uuid::new_v4()
        ));
        fs::write(&temporary, bytes).map_err(|_| {
            PortError::new(
                PortErrorKind::Backend,
                "Workbench interruption history could not be written",
            )
        })?;
        replace_interruption_history_file(&temporary, &self.path)
    }
}

#[cfg(not(windows))]
fn replace_interruption_history_file(temporary: &Path, destination: &Path) -> PortResult<()> {
    fs::rename(temporary, destination).map_err(|_| {
        PortError::new(
            PortErrorKind::Backend,
            "Workbench interruption history could not be committed",
        )
    })
}

#[cfg(windows)]
fn replace_interruption_history_file(temporary: &Path, destination: &Path) -> PortResult<()> {
    let backup = destination.with_extension("bak");
    let had_existing = match fs::metadata(destination) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => {
            let _ = fs::remove_file(temporary);
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Workbench interruption history could not be committed",
            ));
        }
    };
    if had_existing {
        match fs::remove_file(&backup) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                let _ = fs::remove_file(temporary);
                return Err(PortError::new(
                    PortErrorKind::Backend,
                    "Workbench interruption history could not be committed",
                ));
            }
        }
        if fs::rename(destination, &backup).is_err() {
            let _ = fs::remove_file(temporary);
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Workbench interruption history could not be committed",
            ));
        }
    }
    if fs::rename(temporary, destination).is_err() {
        if had_existing {
            let _ = fs::rename(&backup, destination);
        }
        let _ = fs::remove_file(temporary);
        return Err(PortError::new(
            PortErrorKind::Backend,
            "Workbench interruption history could not be committed",
        ));
    }
    match fs::remove_file(&backup) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(PortError::new(
            PortErrorKind::Backend,
            "Workbench interruption history could not be committed",
        )),
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

fn halo_workbench_managed_event_facts_path() -> Result<PathBuf, String> {
    let path_manager = crate::infrastructure::try_get_path_manager_arc()
        .map_err(|error| format!("Workbench managed facts path is unavailable: {error}"))?;
    Ok(path_manager
        .user_config_dir()
        .join("halo-workbench-managed-event-facts.json"))
}

fn halo_workbench_interruption_history_path() -> Result<PathBuf, String> {
    let path_manager = crate::infrastructure::try_get_path_manager_arc()
        .map_err(|error| format!("Workbench interruption history path is unavailable: {error}"))?;
    Ok(path_manager
        .user_config_dir()
        .join("halo-workbench-interruption-history.json"))
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
    let interruption_history = Arc::new(JsonFileHaloWorkbenchInterruptionHistory::new(
        halo_workbench_interruption_history_path()?,
    ));
    let managed_event_facts = Arc::new(
        halo_services_core::managed_event_facts::JsonFileManagedEventFacts::new(
            halo_workbench_managed_event_facts_path()?,
        ),
    );
    let runtime = HaloWorkbenchRuntime::try_new_with_delivery_evidence_and_fact_store_and_interruption_history(
        adapter.clone(),
        Arc::new(CoreWorkbenchWorkspaceFacts::new(workspace_service)),
        Arc::new(PiRpcConfiguredReadinessGate {
            configuration: configuration.clone(),
        }),
        Arc::new(CoreWorkbenchTaskBaseline),
        Arc::new(CoreWorkbenchDeliveryEvidence),
        managed_event_facts,
        interruption_history,
        Arc::new(SystemProductClock),
    )
    .map_err(|error| error.to_string())?;
    // ADR-0078 M3 executor selection: the sole P0 pi adapter is bound behind
    // the unified ManagedExecutorPort wrapper (capability facts derived from
    // verified readiness). The optional DSH executor joins the task-creation
    // selector only under the `dsh-executor` feature; both are fixed per task
    // with no in-session switch.
    runtime.install_managed_executor(
        ManagedExecutorKind::PiRpc,
        Arc::new(PiRpcManagedExecutor::new(adapter)),
    );
    #[cfg(feature = "dsh-executor")]
    runtime.install_managed_executor(
        ManagedExecutorKind::Dsh,
        Arc::new(halo_dsh_adapter::DshManagedExecutor::new(Arc::new(
            halo_dsh_adapter::DshAdapter::with_config(Default::default()),
        ))),
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
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Arc, Mutex, OnceLock};

    use chrono::Utc;
    use halo_agent_runtime::halo_workbench::{
        HaloWorkbenchActivityKind, HaloWorkbenchActivitySnapshot, HaloWorkbenchActivityStatus,
        HaloWorkbenchInterruptionHistoryPort, HaloWorkbenchMessageRole,
        HaloWorkbenchMessageSnapshot, HaloWorkbenchSessionSnapshot,
    };
    use halo_runtime_ports::{
        PiCredentialSecret, PiCredentialStorePort, PiRpcCommand, PiRpcFailureKind, PiRpcPort,
        PiRpcReply, PiRpcWorkspace, PortErrorKind, WorkbenchDeliveryAttributionKind,
        WorkbenchDeliveryEvidenceRequest, WorkbenchDeliveryFingerprint, WorkbenchTaskBaseline,
        WorkbenchTaskBaselinePort, WorkbenchTaskBaselineRequest, WorkbenchWorkspaceFactsRequest,
    };

    use super::{
        attribute_changes, project_workspace_facts, CoreWorkbenchTaskBaseline,
        JsonFileHaloWorkbenchInterruptionHistory, PiRpcAdapter, PiSystemCredentialStore,
    };
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

    fn reviewable_interrupted_session(session_id: &str) -> HaloWorkbenchSessionSnapshot {
        serde_json::from_value(serde_json::json!({
            "workspaceId": "workspace-1",
            "taskId": "task-1",
            "sessionId": session_id,
            "mode": "managed",
            "phase": "interrupted",
            "cancellationMode": "native",
            "baseline": null,
            "messages": [],
            "activities": [],
            "error": null,
            "deliveryReview": {
                "evidence": {
                    "capturedAtMs": 1,
                    "head": "test-head",
                    "workingTreeFingerprint": "test-fingerprint",
                    "changedFiles": ["tracked.rs"],
                    "diffPreview": "reviewed diff",
                    "attribution": []
                },
                "summary": "review summary",
                "verificationResults": "verification results",
                "runConclusion": "run conclusion",
                "decision": null
            }
        }))
        .expect("reviewable interruption history fixture")
    }

    #[test]
    fn interruption_history_keeps_frozen_review_and_rejects_active_session_content() {
        let root = tempfile::tempdir().expect("interruption history root");
        let history = JsonFileHaloWorkbenchInterruptionHistory::new(
            root.path().join("halo-workbench-interruption-history.json"),
        );
        let reviewable = reviewable_interrupted_session("session-1");

        history
            .replace_interrupted_sessions(vec![reviewable.clone()])
            .expect("frozen delivery review is durable interruption evidence");
        assert_eq!(
            history
                .load_interrupted_sessions()
                .expect("frozen delivery review is restored"),
            vec![reviewable.clone()]
        );

        let mut message_leak = reviewable.clone();
        message_leak.messages.push(HaloWorkbenchMessageSnapshot {
            role: HaloWorkbenchMessageRole::User,
            content: "must not persist active content".to_string(),
        });
        assert_eq!(
            history
                .replace_interrupted_sessions(vec![message_leak])
                .expect_err("active messages are not durable interruption facts")
                .kind,
            PortErrorKind::InvalidRequest
        );

        let mut activity_leak = reviewable;
        activity_leak
            .activities
            .push(HaloWorkbenchActivitySnapshot {
                activity_id: "activity-safe-id".to_string(),
                kind: HaloWorkbenchActivityKind::Tool,
                label: "write".to_string(),
                status: HaloWorkbenchActivityStatus::Started,
                is_error: false,
            });
        assert_eq!(
            history
                .replace_interrupted_sessions(vec![activity_leak])
                .expect_err("active activities are not durable interruption facts")
                .kind,
            PortErrorKind::InvalidRequest
        );
    }

    #[cfg(windows)]
    #[test]
    fn interruption_history_recovery_retires_stale_backup_before_the_next_write() {
        let root = tempfile::tempdir().expect("interruption history root");
        let path = root.path().join("halo-workbench-interruption-history.json");
        let backup = path.with_extension("bak");
        let history = JsonFileHaloWorkbenchInterruptionHistory::new(path.clone());
        let original = reviewable_interrupted_session("session-original");
        let recovered = reviewable_interrupted_session("session-recovered");
        let latest = reviewable_interrupted_session("session-latest");

        history
            .replace_interrupted_sessions(vec![original.clone()])
            .expect("initial interruption evidence persists");
        std::fs::rename(&path, &backup).expect("simulated crash leaves only the backup");
        assert_eq!(
            history
                .load_interrupted_sessions()
                .expect("backup restores interruption evidence"),
            vec![original]
        );

        history
            .replace_interrupted_sessions(vec![recovered.clone()])
            .expect("first write after backup recovery persists");
        assert!(
            !backup.exists(),
            "a successful replacement must retire the stale recovery backup"
        );

        history
            .replace_interrupted_sessions(vec![latest.clone()])
            .expect("later disposition writes remain durable");
        assert_eq!(
            history
                .load_interrupted_sessions()
                .expect("latest interruption history persists"),
            vec![latest]
        );
    }

    #[tokio::test]
    async fn system_pi_credentials_are_provider_bound_and_delete_without_reading_secret() {
        let _lock = PI_SYSTEM_STORE_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("Pi system credential test lock");
        let root = tempfile::tempdir().expect("test credential root");
        halo_ai_adapters::subscription_auth::set_store_path_for_test(
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
        halo_ai_adapters::subscription_auth::set_store_path_for_test(subscription_metadata.clone());

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
    fn delivery_attribution_classifies_baseline_task_and_manual_changes() {
        let request = WorkbenchDeliveryEvidenceRequest {
            workspace_id: "workspace-1".to_string(),
            canonical_root: PathBuf::from("C:/work/workspace-1"),
            baseline: WorkbenchTaskBaseline {
                head: "head".to_string(),
                canonical_root: PathBuf::from("C:/work/workspace-1"),
                existing_changed_files: vec!["pre-existing.rs".to_string()],
                working_tree_fingerprint: "a".repeat(64),
                captured_at_ms: 1,
            },
            settled: Some(WorkbenchDeliveryFingerprint {
                head: "head".to_string(),
                changed_files: vec!["pre-existing.rs".to_string(), "task-change.rs".to_string()],
                working_tree_fingerprint: "b".repeat(64),
                captured_at_ms: 2,
            }),
        };

        let attribution = attribute_changes(
            &request,
            &[
                "pre-existing.rs".to_string(),
                "task-change.rs".to_string(),
                "manual-change.rs".to_string(),
            ],
        );

        let kind_for = |path: &str| {
            attribution
                .iter()
                .find(|item| item.path == path)
                .map(|item| item.kind)
        };
        assert_eq!(
            kind_for("pre-existing.rs"),
            Some(WorkbenchDeliveryAttributionKind::ExistingUserModification)
        );
        assert_eq!(
            kind_for("task-change.rs"),
            Some(WorkbenchDeliveryAttributionKind::TaskModification)
        );
        assert_eq!(
            kind_for("manual-change.rs"),
            Some(WorkbenchDeliveryAttributionKind::ManualIntervention)
        );
    }

    #[test]
    fn registered_local_git_workspace_projects_trusted_canonical_facts() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let mut workspace = workspace(root.path().to_path_buf(), WorkspaceKind::Normal);
        workspace.metadata.insert(
            "haloWorkbenchTrusted".to_string(),
            serde_json::Value::Bool(true),
        );
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
    async fn managed_task_baseline_fingerprint_tracks_dirty_git_content_across_statuses() {
        fn git(root: &Path, args: &[&str]) {
            let output = Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .expect("Git fixture command starts");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn git_stdout(root: &Path, args: &[&str]) -> String {
            let output = Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .expect("Git fixture command starts");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .expect("Git fixture output is UTF-8")
                .trim()
                .to_string()
        }

        async fn capture(root: &Path) -> halo_runtime_ports::WorkbenchTaskBaseline {
            CoreWorkbenchTaskBaseline
                .capture(WorkbenchTaskBaselineRequest {
                    workspace_id: "baseline-fixture".to_string(),
                    canonical_root: root.to_path_buf(),
                })
                .await
                .expect("baseline is captured")
        }

        let root = tempfile::tempdir().expect("temporary Git fixture");
        git(root.path(), &["init"]);
        git(
            root.path(),
            &["config", "user.email", "baseline@example.com"],
        );
        git(root.path(), &["config", "user.name", "Baseline Fixture"]);
        std::fs::write(root.path().join("tracked.txt"), "base\n").expect("writes base file");
        git(root.path(), &["add", "--", "tracked.txt"]);
        git(root.path(), &["commit", "-m", "base"]);

        let clean = capture(root.path()).await;
        assert!(clean.existing_changed_files.is_empty());

        std::fs::write(root.path().join("tracked.txt"), "staged-one\n")
            .expect("writes staged change");
        git(root.path(), &["add", "--", "tracked.txt"]);
        let staged_one = capture(root.path()).await;

        std::fs::write(root.path().join("tracked.txt"), "staged-two\n")
            .expect("rewrites staged change");
        git(root.path(), &["add", "--", "tracked.txt"]);
        let staged_two = capture(root.path()).await;
        assert_ne!(
            staged_one.working_tree_fingerprint, staged_two.working_tree_fingerprint,
            "staged content participates in the baseline fingerprint"
        );

        std::fs::write(root.path().join("tracked.txt"), "unstaged-one\n")
            .expect("writes unstaged change");
        let unstaged_one = capture(root.path()).await;
        std::fs::write(root.path().join("tracked.txt"), "unstaged-two\n")
            .expect("rewrites unstaged change");
        let unstaged_two = capture(root.path()).await;
        assert_ne!(
            unstaged_one.working_tree_fingerprint, unstaged_two.working_tree_fingerprint,
            "unstaged content participates in the baseline fingerprint"
        );

        std::fs::write(root.path().join("untracked.txt"), "untracked-one\n")
            .expect("writes untracked file");
        let untracked_one = capture(root.path()).await;
        std::fs::write(root.path().join("untracked.txt"), "untracked-two\n")
            .expect("rewrites untracked file");
        let untracked_two = capture(root.path()).await;
        assert_ne!(
            untracked_one.working_tree_fingerprint, untracked_two.working_tree_fingerprint,
            "untracked content participates in the baseline fingerprint"
        );

        let conflict_root = tempfile::tempdir().expect("temporary conflict fixture");
        git(conflict_root.path(), &["init"]);
        git(
            conflict_root.path(),
            &["config", "user.email", "baseline@example.com"],
        );
        git(
            conflict_root.path(),
            &["config", "user.name", "Baseline Fixture"],
        );
        std::fs::write(conflict_root.path().join("tracked.txt"), "base\n")
            .expect("writes conflict base");
        git(conflict_root.path(), &["add", "--", "tracked.txt"]);
        git(conflict_root.path(), &["commit", "-m", "base"]);
        let primary_branch = git_stdout(conflict_root.path(), &["branch", "--show-current"]);
        git(conflict_root.path(), &["switch", "-c", "competing"]);
        std::fs::write(conflict_root.path().join("tracked.txt"), "competing\n")
            .expect("writes competing change");
        git(conflict_root.path(), &["add", "--", "tracked.txt"]);
        git(conflict_root.path(), &["commit", "-m", "competing"]);
        git(conflict_root.path(), &["switch", primary_branch.as_str()]);
        std::fs::write(conflict_root.path().join("tracked.txt"), "primary\n")
            .expect("writes primary change");
        git(conflict_root.path(), &["add", "--", "tracked.txt"]);
        git(conflict_root.path(), &["commit", "-m", "primary"]);
        let merge = Command::new("git")
            .current_dir(conflict_root.path())
            .args(["merge", "competing"])
            .output()
            .expect("conflict fixture merge starts");
        assert!(
            !merge.status.success(),
            "fixture merge must leave an unresolved conflict"
        );

        let conflict_one = capture(conflict_root.path()).await;
        assert!(
            conflict_one
                .existing_changed_files
                .iter()
                .any(|path| path == "tracked.txt"),
            "the baseline retains the conflicted path"
        );
        std::fs::write(
            conflict_root.path().join("tracked.txt"),
            "edited-conflict-marker\n",
        )
        .expect("edits conflicted working file");
        let conflict_two = capture(conflict_root.path()).await;
        assert_ne!(
            conflict_one.working_tree_fingerprint, conflict_two.working_tree_fingerprint,
            "conflicted working content participates in the baseline fingerprint"
        );
        assert_eq!(
            untracked_two.working_tree_fingerprint.len(),
            64,
            "only a fixed-length digest crosses the baseline port"
        );
        assert!(
            untracked_two
                .working_tree_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
            "the public baseline does not carry file content"
        );
        assert_ne!(
            clean.working_tree_fingerprint, untracked_two.working_tree_fingerprint,
            "the fingerprint changes from a clean baseline to a dirty one"
        );
    }

    #[tokio::test]
    async fn p0_selection_is_fixed_to_the_pi_rpc_adapter() {
        let adapter: Arc<dyn PiRpcPort> = Arc::new(PiRpcAdapter::with_config(
            halo_pi_rpc_adapter::PiRpcConfig {
                executable: Some(PathBuf::from("C:/does-not-exist/pi.exe")),
                ..halo_pi_rpc_adapter::PiRpcConfig::default()
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
