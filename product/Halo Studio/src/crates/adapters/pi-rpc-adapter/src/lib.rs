//! The Halo P0 Pi RPC execution adapter.
//!
//! This crate owns the local `pi --mode rpc` child process and translates
//! Pi's JSONL protocol into the narrow Halo Workbench Runtime port. Pi session
//! ids, entry ids, raw tool-call ids, prompts, model/provider objects,
//! credentials, command output, and raw protocol records never leave this
//! module.

mod configuration;
mod framing;

pub use bitfun_runtime_ports::PiRuntimeConfigurationView;
pub use configuration::{
    validate_runtime_configuration_shape, JsonFilePiRuntimeConfigurationRepository,
    MemoryPiCredentialStore, MemoryPiRuntimeConfigurationRepository,
    PiRuntimeConfigurationRepository, PiRuntimeConfigurationService, StaticPiProviderCapabilities,
};

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bitfun_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiProviderCapability, PiProviderCapabilityPort,
    PiProviderCapabilityRequest, PiRpcAvailabilitySummary, PiRpcCancellationMode, PiRpcCommand,
    PiRpcEvent, PiRpcFailureKind, PiRpcOperationDecision, PiRpcOperationKind,
    PiRpcOperationRiskLevel, PiRpcOperationSummary, PiRpcPort, PiRpcReply, PiRpcSessionMode,
    PiRpcVersion, PiRpcVersionEvidenceSource, PiRpcWorkspace, PiRuntimeConfiguration,
    PiRuntimeConfigurationPort, PortError, PortErrorKind, PortResult, PI_RPC_ADAPTER_IDENTITY,
};
use bitfun_services_core::process_tree::ProcessTreeChild;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::{broadcast, oneshot, Mutex, Notify};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::framing::{decode_jsonl_record, encode_jsonl};

const EVENT_CAPACITY: usize = 128;
const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_ABORT_GRACE_PERIOD: Duration = Duration::from_secs(3);
const MAX_ASSISTANT_DELTA_BYTES: usize = 8 * 1024;
const MAX_TOOL_NAME_BYTES: usize = 128;
const MAX_TOOL_ARGUMENTS_BYTES: usize = 512;
const NO_FAILURE_REASON: u8 = 0;
const SESSION_TERMINAL_OPEN: u8 = 0;
const SESSION_TERMINAL_CANCELLING: u8 = 1;
const SESSION_TERMINAL_FAILED: u8 = 2;
// The compatibility profile is intentionally explicit. A successful
// `--version` process exit is not evidence that its RPC schema is known.
const SUPPORTED_PI_RPC_PROFILES: &[(&str, PiRpcVersion)] = &[
    ("0.81.1", PiRpcVersion::V0_81_1),
    ("0.83.0", PiRpcVersion::V0_83_0),
];
const SUPPORTED_ASSISTANT_MESSAGE_EVENT_TYPES: &[&str] = &[
    "start",
    "text_start",
    "text_delta",
    "text_end",
    "thinking_start",
    "thinking_delta",
    "thinking_end",
    "toolcall_start",
    "toolcall_delta",
    "toolcall_end",
    "done",
    "error",
];
const SAFE_CHILD_ENVIRONMENT: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SystemRoot",
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "HOME",
    "APPDATA",
    "LOCALAPPDATA",
    "ALLUSERSPROFILE",
    "PROGRAMDATA",
    "COMSPEC",
];

const HALO_PERMISSION_EXTENSION_SOURCE: &str = include_str!("halo_permission_gate.ts");
const HALO_PI_CREDENTIAL_ENV: &str = "HALO_PI_CREDENTIAL";
const HALO_PI_CREDENTIAL_ENV_REFERENCE: &str = "$HALO_PI_CREDENTIAL";

/// Fixed first-party extension identity. The source digest is exposed for the
/// audit record, never as a renderer or evidence payload.
pub const HALO_PI_EXTENSION_ID: &str = "halo-workbench-permission-gate";
pub const HALO_PI_EXTENSION_VERSION: &str = "1.0.0";
pub const HALO_PI_EXTENSION_PERMISSIONS: &str =
    "Pi tool_call interception and RPC extension_ui_request only";
pub const HALO_PI_EXTENSION_SOURCE: &str =
    "Halo Studio source: src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts";
pub const HALO_PI_EXTENSION_DEPENDENCIES: &str =
    "host-provided @earendil-works/pi-coding-agent ExtensionAPI; no extension runtime imports";
pub const HALO_PI_EXTENSION_HOST_PERMISSIONS: &str =
    "observe tool_call name/id; request one confirmation; return block/allow; no filesystem, process, network, credential, or renderer access";
pub const HALO_PI_EXTENSION_UPDATE_OWNER: &str =
    "Halo Studio maintainers; update only with source/hash/license re-audit";
pub const HALO_PI_EXTENSION_LICENSE: &str =
    "Halo Studio repository license policy; host Pi package license must remain separately audited";

#[derive(Clone)]
pub struct PiRpcConfig {
    /// Explicit executable used by tests or a reviewed deployment.
    pub executable: Option<PathBuf>,
    /// Explicitly audited first-party extension path. When absent, the
    /// adapter writes the embedded extension to an owned temporary directory.
    pub extension_path: Option<PathBuf>,
    /// Non-sensitive provider selection passed through to Pi when configured
    /// by a later Halo configuration transaction. Credentials never belong in
    /// this structure.
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Halo's non-secret configuration authority. When present, the adapter
    /// must load it before a controlled RPC child is created.
    pub runtime_configuration: Option<Arc<dyn PiRuntimeConfigurationPort>>,
    /// OS-backed credential store. The adapter reads from it only at the
    /// controlled child creation boundary and never exposes the value.
    pub credential_store: Option<Arc<dyn PiCredentialStorePort>>,
    /// Pi-native capability/readiness source used to validate the selected
    /// provider, model, base URL and thinking level.
    pub provider_capabilities: Option<Arc<dyn PiProviderCapabilityPort>>,
    /// Test/deployment root for adapter-owned config/session directories. The
    /// directory itself is never exposed through the Workbench public seam.
    pub temporary_root: Option<PathBuf>,
    /// Stable adapter-owned root for standard session history. When omitted,
    /// the adapter derives an application-local data root; tests should set
    /// this or `temporary_root` to keep storage isolated.
    pub persistent_session_root: Option<PathBuf>,
    pub response_timeout: Duration,
    pub operation_timeout: Duration,
    pub abort_grace_period: Duration,
}

impl Default for PiRpcConfig {
    fn default() -> Self {
        Self {
            executable: None,
            extension_path: None,
            provider: None,
            model: None,
            runtime_configuration: None,
            credential_store: None,
            provider_capabilities: None,
            temporary_root: None,
            persistent_session_root: None,
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
            abort_grace_period: DEFAULT_ABORT_GRACE_PERIOD,
        }
    }
}

#[derive(Clone)]
pub struct PiRpcAdapter {
    events: broadcast::Sender<PiRpcEvent>,
    state: Arc<Mutex<AdapterState>>,
    lifecycle: Arc<Mutex<()>>,
    config: PiRpcConfig,
}

#[derive(Default)]
struct AdapterState {
    generation: Option<u64>,
    workspace: Option<PiRpcWorkspace>,
    executable: Option<PathBuf>,
    readiness_summary: Option<PiRpcAvailabilitySummary>,
    sessions: HashMap<String, Arc<PiSession>>,
}

#[derive(Clone)]
struct ResolvedRuntimeConfiguration {
    configuration: PiRuntimeConfiguration,
    capability: Option<PiProviderCapability>,
}

#[derive(Clone)]
struct ResolvedPiExecutable {
    path: PathBuf,
    summary: PiRpcAvailabilitySummary,
}

struct InstalledExtension {
    path: PathBuf,
    owned_dir: Option<PathBuf>,
}

impl Drop for InstalledExtension {
    fn drop(&mut self) {
        if let Some(directory) = self.owned_dir.take() {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

struct PiSession {
    generation: u64,
    task_id: Mutex<String>,
    session_id: Mutex<String>,
    is_readiness_probe: bool,
    adapter_state: Weak<Mutex<AdapterState>>,
    _config_dir: tempfile::TempDir,
    _extension: Option<InstalledExtension>,
    /// Standard session directories are intentionally persistent. Managed
    /// sessions use `--no-session` and therefore keep this field `None`.
    _session_dir: Option<PathBuf>,
    events: broadcast::Sender<PiRpcEvent>,
    stdin: Mutex<ChildStdin>,
    child: Mutex<ProcessTreeChild>,
    pending: Mutex<HashMap<String, oneshot::Sender<PortResult<Value>>>>,
    operations: Mutex<HashMap<String, PiOperationBinding>>,
    seen_extension_requests: Mutex<HashSet<String>>,
    tool_calls: Mutex<HashMap<String, CapturedToolCall>>,
    prompt_sent: AtomicBool,
    prompt_accepted: AtomicBool,
    running: AtomicBool,
    terminated: AtomicBool,
    terminal_disposition: AtomicU8,
    failure_reason: AtomicU8,
    settled_epoch: AtomicU64,
    settled: Notify,
    response_timeout: Duration,
    operation_timeout: Duration,
    abort_grace_period: Duration,
}

#[derive(Debug, Clone)]
struct PiOperationBinding {
    generation: u64,
    task_id: String,
    session_id: String,
    ui_request_id: String,
    redacted_tool_call_id: String,
}

#[derive(Debug, Deserialize)]
struct PermissionNotice {
    #[serde(rename = "toolCallId")]
    tool_call_id: String,
    #[serde(rename = "toolName")]
    tool_name: String,
}

/// Adapter-owned, redacted tool-call facts captured from the Pi RPC
/// `tool_execution_start` event and correlated to the first-party gate's
/// `extension_ui_request`. Raw arguments never leave this module.
#[derive(Debug, Clone)]
struct CapturedToolCall {
    tool_name: String,
    raw_tool_name: String,
    redacted_arguments: String,
    risk_level: PiRpcOperationRiskLevel,
}

impl Default for PiRpcAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PiRpcAdapter {
    pub const IDENTITY: &'static str = PI_RPC_ADAPTER_IDENTITY;

    pub fn new() -> Self {
        Self::with_config(PiRpcConfig::default())
    }

    pub fn with_config(config: PiRpcConfig) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            events,
            state: Arc::new(Mutex::new(AdapterState::default())),
            lifecycle: Arc::new(Mutex::new(())),
            config,
        }
    }

    pub fn extension_source() -> &'static str {
        HALO_PERMISSION_EXTENSION_SOURCE
    }

    pub fn extension_source_digest() -> String {
        stable_digest(HALO_PERMISSION_EXTENSION_SOURCE)
    }

    fn emit(&self, event: PiRpcEvent) {
        let _ = self.events.send(event);
    }

    async fn probe_executable_version(
        &self,
        executable: &Path,
    ) -> Result<std::process::Output, PiRpcFailureKind> {
        let config_dir = self.create_private_directory("version-probe")?;
        let output = configure_child_command(
            build_pi_command(executable, &["--version".to_string()]),
            executable,
            Some(config_dir.path()),
            None,
        )
        .output()
        .await;

        // Keep the probe's config lifetime independent from the returned
        // process output. This also makes failed probes fail closed if their
        // adapter-owned directory cannot be removed.
        config_dir.close().map_err(|_| PiRpcFailureKind::Internal)?;
        output.map_err(|_| PiRpcFailureKind::NotInstalled)
    }

    async fn probe_pi(&self) -> Result<ResolvedPiExecutable, PiRpcFailureKind> {
        let executable = self.resolve_executable().await?;
        let output = self.probe_executable_version(&executable).await?;
        if !output.status.success() {
            return Err(PiRpcFailureKind::UnsupportedVersion);
        }
        // Version probing is only executable/version readiness. It does not
        // read auth.json/models.json, invoke a provider, or prove RPC/model
        // readiness.
        let summary = availability_summary_from_version_output(&output)
            .ok_or(PiRpcFailureKind::UnsupportedVersion)?;
        Ok(ResolvedPiExecutable {
            path: executable,
            summary,
        })
    }

    async fn resolve_executable(&self) -> Result<PathBuf, PiRpcFailureKind> {
        if let Some(executable) = &self.config.executable {
            return Ok(executable.clone());
        }

        #[cfg(windows)]
        {
            let mut candidates = Vec::new();

            if let Ok(output) = Command::new("where.exe").arg("pi").output().await {
                if output.status.success() {
                    candidates.extend(parse_command_paths(&output.stdout));
                }
            }

            // npm commonly installs a PowerShell shim that is visible to
            // Get-Command even when where.exe returns exit code 1. The query
            // is fixed and returns only command paths, never environment data.
            if let Ok(output) = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "$ErrorActionPreference='SilentlyContinue'; (Get-Command pi -All | ForEach-Object { $_.Path })",
                ])
                .output()
                .await
            {
                candidates.extend(parse_command_paths(&output.stdout));
            }

            candidates.extend(
                ["pi.cmd", "pi.exe", "pi.ps1", "pi"]
                    .into_iter()
                    .map(PathBuf::from),
            );

            let mut fallback = None;
            for candidate in dedupe_paths(candidates) {
                if fallback.is_none() {
                    fallback = Some(candidate.clone());
                }
                if self
                    .probe_executable_version(&candidate)
                    .await
                    .is_ok_and(|output| {
                        output.status.success()
                            && availability_summary_from_version_output(&output).is_some()
                    })
                {
                    return Ok(candidate);
                }
            }
            return fallback.ok_or(PiRpcFailureKind::NotInstalled);
        }

        #[cfg(not(windows))]
        {
            Ok(PathBuf::from("pi"))
        }
    }

    fn install_first_party_extension(&self) -> Result<InstalledExtension, PiRpcFailureKind> {
        if let Some(path) = &self.config.extension_path {
            // Verify the reviewed source, then copy the exact bytes into an
            // adapter-owned directory. Pi never executes the caller-owned
            // path, so a replacement after validation cannot create a TOCTOU
            // execution path through the explicit extension option.
            let canonical_path =
                std::fs::canonicalize(path).map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
            let source = std::fs::read_to_string(canonical_path)
                .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
            if source != HALO_PERMISSION_EXTENSION_SOURCE {
                return Err(PiRpcFailureKind::CapabilityMismatch);
            }
        }

        self.install_embedded_extension()
    }

    fn install_embedded_extension(&self) -> Result<InstalledExtension, PiRpcFailureKind> {
        let extension_root = self.config.temporary_root.clone().unwrap_or_else(|| {
            std::env::temp_dir()
                .join("halo-studio")
                .join("pi-extensions")
        });
        let extension_dir =
            extension_root.join(format!("{HALO_PI_EXTENSION_ID}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&extension_dir).map_err(|_| PiRpcFailureKind::Internal)?;
        let extension_path = extension_dir.join(format!(
            "{HALO_PI_EXTENSION_ID}-{}.ts",
            stable_digest(HALO_PERMISSION_EXTENSION_SOURCE)
        ));
        if std::fs::write(&extension_path, HALO_PERMISSION_EXTENSION_SOURCE).is_err() {
            let _ = std::fs::remove_dir_all(&extension_dir);
            return Err(PiRpcFailureKind::Internal);
        }
        Ok(InstalledExtension {
            path: extension_path,
            owned_dir: Some(extension_dir),
        })
    }

    async fn spawn_session_process(
        &self,
        generation: u64,
        task_id: String,
        session_id: String,
        is_readiness_probe: bool,
        mode: PiRpcSessionMode,
        workspace: &PiRpcWorkspace,
        session_dir: Option<PathBuf>,
        executable: &Path,
        extension: Option<InstalledExtension>,
        runtime_configuration: Option<&PiRuntimeConfiguration>,
        runtime_capability: Option<&PiProviderCapability>,
        credential: Option<PiCredentialSecret>,
    ) -> Result<Arc<PiSession>, PiRpcFailureKind> {
        let config_dir = self.create_private_directory("config")?;
        if let Some(configuration) = runtime_configuration {
            write_pi_config_projection(config_dir.path(), configuration, runtime_capability)?;
        }
        if mode == PiRpcSessionMode::Standard && session_dir.is_none() {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
        let extension_path = extension.as_ref().map(|extension| extension.path.as_path());
        if is_readiness_probe
            && (extension_path.is_some()
                || runtime_configuration.is_some()
                || runtime_capability.is_some()
                || credential.is_some())
        {
            return Err(PiRpcFailureKind::Internal);
        }
        if !is_readiness_probe && extension_path.is_none() {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
        let credential_value = credential.map(PiCredentialSecret::into_string);
        let provider = (!is_readiness_probe).then_some(()).and_then(|_| {
            runtime_configuration
                .map(|configuration| configuration.provider_id.as_str())
                .or(self.config.provider.as_deref())
        });
        let model = (!is_readiness_probe).then_some(()).and_then(|_| {
            runtime_configuration
                .map(|configuration| configuration.model_id.as_str())
                .or(self.config.model.as_deref())
        });
        let thinking = (!is_readiness_probe).then_some(()).and_then(|_| {
            runtime_configuration.map(|configuration| configuration.thinking_level.as_str())
        });
        let mut command = configure_child_command(
            build_pi_command(
                executable,
                &pi_rpc_args(
                    extension_path,
                    mode,
                    session_dir.as_deref(),
                    provider,
                    model,
                    thinking,
                ),
            ),
            executable,
            Some(config_dir.path()),
            credential_value.as_deref(),
        );
        command
            .current_dir(&workspace.canonical_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Pi protocol output is stdout. Child diagnostics must not reach
            // a caller or evidence stream, so stderr is intentionally closed.
            .stderr(Stdio::null());
        let mut child = ProcessTreeChild::spawn(&mut command)
            .await
            .map_err(|_| PiRpcFailureKind::NotInstalled)?;
        let stdin = child.take_stdin().ok_or(PiRpcFailureKind::Transport)?;
        let stdout = child.take_stdout().ok_or(PiRpcFailureKind::Transport)?;
        let session = Arc::new(PiSession {
            generation,
            task_id: Mutex::new(task_id),
            session_id: Mutex::new(session_id),
            is_readiness_probe,
            adapter_state: Arc::downgrade(&self.state),
            _config_dir: config_dir,
            _extension: extension,
            _session_dir: session_dir,
            events: self.events.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            operations: Mutex::new(HashMap::new()),
            seen_extension_requests: Mutex::new(HashSet::new()),
            tool_calls: Mutex::new(HashMap::new()),
            prompt_sent: AtomicBool::new(false),
            prompt_accepted: AtomicBool::new(false),
            running: AtomicBool::new(false),
            terminated: AtomicBool::new(false),
            terminal_disposition: AtomicU8::new(SESSION_TERMINAL_OPEN),
            failure_reason: AtomicU8::new(NO_FAILURE_REASON),
            settled_epoch: AtomicU64::new(0),
            settled: Notify::new(),
            response_timeout: self.config.response_timeout,
            operation_timeout: self.config.operation_timeout,
            abort_grace_period: self.config.abort_grace_period,
        });
        tokio::spawn(read_pi_stdout(session.clone(), stdout));
        Ok(session)
    }

    fn create_private_directory(
        &self,
        prefix: &str,
    ) -> Result<tempfile::TempDir, PiRpcFailureKind> {
        let directory_prefix = format!("halo-pi-{prefix}-");
        let mut builder = tempfile::Builder::new();
        builder.prefix(&directory_prefix);
        match self.config.temporary_root.as_deref() {
            Some(root) => {
                std::fs::create_dir_all(root).map_err(|_| PiRpcFailureKind::Internal)?;
                builder
                    .tempdir_in(root)
                    .map_err(|_| PiRpcFailureKind::Internal)
            }
            None => builder.tempdir().map_err(|_| PiRpcFailureKind::Internal),
        }
    }

    fn standard_session_directory(
        &self,
        workspace: &PiRpcWorkspace,
        task_id: &str,
    ) -> Result<PathBuf, PiRpcFailureKind> {
        let root = self
            .config
            .persistent_session_root
            .clone()
            .or_else(|| {
                self.config
                    .temporary_root
                    .as_ref()
                    .map(|root| root.join("pi-sessions"))
            })
            .unwrap_or_else(default_persistent_session_root);
        let workspace_key = stable_digest(&format!(
            "{}:{}",
            workspace.workspace_id,
            workspace.canonical_root.to_string_lossy()
        ));
        let task_key = stable_digest(task_id);
        let workspace_root = root.join("workspaces").join(workspace_key);
        std::fs::create_dir_all(&workspace_root).map_err(|_| PiRpcFailureKind::Internal)?;
        let canonical_root =
            std::fs::canonicalize(&workspace_root).map_err(|_| PiRpcFailureKind::Internal)?;
        let session_dir = workspace_root.join(task_key);
        std::fs::create_dir_all(&session_dir).map_err(|_| PiRpcFailureKind::Internal)?;
        let canonical_session_dir =
            std::fs::canonicalize(&session_dir).map_err(|_| PiRpcFailureKind::Internal)?;
        if !canonical_session_dir.starts_with(&canonical_root) {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
        Ok(canonical_session_dir)
    }

    async fn resolve_runtime_configuration(
        &self,
        workspace: &PiRpcWorkspace,
    ) -> Result<Option<ResolvedRuntimeConfiguration>, PiRpcFailureKind> {
        let Some(configuration_port) = self.config.runtime_configuration.as_ref() else {
            if self.config.credential_store.is_some() || self.config.provider_capabilities.is_some()
            {
                return Err(PiRpcFailureKind::CapabilityMismatch);
            }
            return Ok(None);
        };

        // A project-local .pi tree is an uncontrolled Pi configuration and may
        // contain settings, auth state, packages, or extensions. P0 refuses to
        // run in that workspace rather than attempting to interpret or merge it.
        if workspace.canonical_root.join(".pi").exists() {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }

        let configuration = configuration_port
            .load_configuration()
            .await
            .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?
            .ok_or(PiRpcFailureKind::CapabilityMismatch)?;
        validate_runtime_configuration_shape(&configuration)
            .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;

        let capability = if let Some(capabilities) = self.config.provider_capabilities.as_ref() {
            let capability = capabilities
                .inspect(PiProviderCapabilityRequest {
                    provider_id: configuration.provider_id.clone(),
                    model_id: configuration.model_id.clone(),
                    base_url: configuration.base_url.clone(),
                })
                .await
                .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
            if capability.provider_id != configuration.provider_id
                || capability.model_id != configuration.model_id
                || capability.api.is_empty()
                || capability.api.len() > 128
                || capability
                    .api
                    .chars()
                    .any(|character| character.is_control() || character.is_whitespace())
                || (configuration.base_url.is_some() && !capability.accepts_base_url)
                || !capability
                    .supported_thinking_levels
                    .contains(&configuration.thinking_level)
            {
                return Err(PiRpcFailureKind::CapabilityMismatch);
            }
            Some(capability)
        } else {
            None
        };
        Ok(Some(ResolvedRuntimeConfiguration {
            configuration,
            capability,
        }))
    }

    async fn read_runtime_credential(
        &self,
        configuration: Option<&PiRuntimeConfiguration>,
    ) -> Result<Option<PiCredentialSecret>, PiRpcFailureKind> {
        let Some(configuration) = configuration else {
            return Ok(None);
        };
        let store = self
            .config
            .credential_store
            .as_ref()
            .ok_or(PiRpcFailureKind::Authentication)?;
        let secret = store
            .read(&configuration.provider_id, &configuration.credential_ref)
            .await
            .map_err(|_| PiRpcFailureKind::Authentication)?;
        let value = secret.into_string();
        if value.is_empty() {
            return Err(PiRpcFailureKind::Authentication);
        }
        Ok(Some(PiCredentialSecret::new(value)))
    }

    async fn validate_native_capability(
        &self,
        session: &Arc<PiSession>,
        configuration: &PiRuntimeConfiguration,
    ) -> Result<(), PiRpcFailureKind> {
        let models = session
            .request(
                "get_available_models",
                json!({ "type": "get_available_models" }),
            )
            .await
            .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
        let available = models
            .get("models")
            .and_then(Value::as_array)
            .ok_or(PiRpcFailureKind::CapabilityMismatch)?;
        let selected_model_is_available = available.iter().any(|model| {
            model.get("provider").and_then(Value::as_str) == Some(&configuration.provider_id)
                && model
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id == configuration.model_id)
        });
        if !selected_model_is_available {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }

        session
            .request(
                "set_model",
                json!({
                    "type": "set_model",
                    "provider": configuration.provider_id,
                    "modelId": configuration.model_id,
                }),
            )
            .await
            .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
        let levels = session
            .request(
                "get_available_thinking_levels",
                json!({ "type": "get_available_thinking_levels" }),
            )
            .await
            .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
        let levels = levels
            .get("levels")
            .and_then(Value::as_array)
            .ok_or(PiRpcFailureKind::CapabilityMismatch)?;
        if !levels
            .iter()
            .any(|level| level.as_str() == Some(configuration.thinking_level.as_str()))
        {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
        Ok(())
    }

    async fn handshake(&self, session: &Arc<PiSession>) -> Result<(), PiRpcFailureKind> {
        let state = session
            .request("get_state", json!({ "type": "get_state" }))
            .await
            .map_err(map_handshake_error)?;
        if validate_state_data(&state).is_err() {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
            return Err(PiRpcFailureKind::Protocol);
        }

        let entries = session
            .request("get_entries", json!({ "type": "get_entries" }))
            .await
            .map_err(map_handshake_error)?;
        let cursor = match validate_entries_data(&entries) {
            Ok(entries) => entries.leaf_id,
            Err(()) => {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return Err(PiRpcFailureKind::Protocol);
            }
        };
        if let Some(cursor) = cursor {
            let since = session
                .request(
                    "get_entries",
                    json!({ "type": "get_entries", "since": cursor }),
                )
                .await
                .map_err(map_handshake_error)?;
            if validate_incremental_entries_data(&since, &cursor).is_err() {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return Err(PiRpcFailureKind::Protocol);
            }
        }

        // An idle abort is side-effect free and proves the command path used
        // by StopSession without sending a prompt or triggering a model
        // request. Prompt/follow-up and event capabilities remain profile
        // declarations until the real-RPC acceptance owned by issue 14.
        session
            .request("abort", json!({ "type": "abort" }))
            .await
            .map_err(map_handshake_error)?;
        Ok(())
    }

    async fn start(&self, generation: u64, workspace: PiRpcWorkspace) -> PiRpcReply {
        let _lifecycle = self.lifecycle.lock().await;
        {
            let state = self.state.lock().await;
            if state.generation == Some(generation) && state.workspace.as_ref() == Some(&workspace)
            {
                return state
                    .readiness_summary
                    .clone()
                    .map(|summary| PiRpcReply::Ready { summary })
                    .unwrap_or(PiRpcReply::Unavailable {
                        reason: PiRpcFailureKind::CapabilityMismatch,
                    });
            }
            if state.generation.is_some() {
                return PiRpcReply::Unavailable {
                    reason: PiRpcFailureKind::Transport,
                };
            }
        }

        let resolved = match self.probe_pi().await {
            Ok(resolved) => resolved,
            Err(reason) => return PiRpcReply::Unavailable { reason },
        };
        let executable = resolved.path.clone();
        let readiness_probe = match self
            .spawn_session_process(
                generation,
                "__halo_workbench_readiness__".to_string(),
                "__halo_workbench_readiness__".to_string(),
                true,
                PiRpcSessionMode::Managed,
                &workspace,
                None,
                &executable,
                None,
                None,
                None,
                None,
            )
            .await
        {
            Ok(session) => session,
            Err(reason) => return PiRpcReply::Unavailable { reason },
        };
        if let Err(reason) = self.handshake(&readiness_probe).await {
            readiness_probe.terminate().await;
            return PiRpcReply::Unavailable { reason };
        }
        let readiness_summary = resolved.summary.with_readiness_handshake_verified();
        readiness_probe.terminate().await;

        let mut state = self.state.lock().await;
        state.generation = Some(generation);
        state.workspace = Some(workspace);
        state.executable = Some(executable);
        state.readiness_summary = Some(readiness_summary.clone());
        drop(state);

        self.emit(PiRpcEvent::Ready { generation });
        PiRpcReply::Ready {
            summary: readiness_summary,
        }
    }

    async fn create_session(
        &self,
        generation: u64,
        task_id: String,
        session_id: String,
        mode: PiRpcSessionMode,
    ) -> Result<(), PiRpcFailureKind> {
        let (workspace, executable) = {
            let state = self.state.lock().await;
            if state.generation != Some(generation) {
                return Err(PiRpcFailureKind::Transport);
            }
            if state.sessions.contains_key(&session_id) {
                return Err(PiRpcFailureKind::Internal);
            }
            (
                state.workspace.clone().ok_or(PiRpcFailureKind::Transport)?,
                state
                    .executable
                    .clone()
                    .ok_or(PiRpcFailureKind::Transport)?,
            )
        };

        let runtime_configuration = self.resolve_runtime_configuration(&workspace).await?;
        if self
            .config
            .provider
            .as_deref()
            .is_some_and(|value| !valid_cli_selection(value))
            || self
                .config
                .model
                .as_deref()
                .is_some_and(|value| !valid_cli_selection(value))
        {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
        let credential = self
            .read_runtime_credential(
                runtime_configuration
                    .as_ref()
                    .map(|resolved| &resolved.configuration),
            )
            .await?;
        let extension = self.install_first_party_extension()?;
        let standard_session_dir = (mode == PiRpcSessionMode::Standard)
            .then(|| self.standard_session_directory(&workspace, &task_id))
            .transpose()?;
        let cleanup_session_dir = standard_session_dir.clone();
        let session = match self
            .spawn_session_process(
                generation,
                task_id.clone(),
                session_id.clone(),
                false,
                mode,
                &workspace,
                standard_session_dir,
                &executable,
                Some(extension),
                runtime_configuration
                    .as_ref()
                    .map(|resolved| &resolved.configuration),
                runtime_configuration
                    .as_ref()
                    .and_then(|resolved| resolved.capability.as_ref()),
                credential,
            )
            .await
        {
            Ok(session) => session,
            Err(reason) => {
                if let Some(session_dir) = cleanup_session_dir.as_deref() {
                    remove_empty_standard_session_directory(session_dir);
                }
                return Err(reason);
            }
        };
        if let Err(reason) = self.handshake(&session).await {
            session.terminate().await;
            if let Some(session_dir) = cleanup_session_dir.as_deref() {
                remove_empty_standard_session_directory(session_dir);
            }
            return Err(reason);
        }
        if let Some(configuration) = runtime_configuration.as_ref() {
            if let Err(reason) = self
                .validate_native_capability(&session, &configuration.configuration)
                .await
            {
                session.terminate().await;
                if let Some(session_dir) = cleanup_session_dir.as_deref() {
                    remove_empty_standard_session_directory(session_dir);
                }
                return Err(reason);
            }
        };

        if session.terminated.load(Ordering::Acquire) || session.has_exited().await {
            let reason = session
                .recorded_failure_reason()
                .unwrap_or(PiRpcFailureKind::Transport);
            session.fail_closed(reason).await;
            if let Some(session_dir) = cleanup_session_dir.as_deref() {
                remove_empty_standard_session_directory(session_dir);
            }
            return Err(reason);
        }

        let mut state = self.state.lock().await;
        if state.generation != Some(generation)
            || session.terminated.load(Ordering::Acquire)
            || state.sessions.contains_key(&session_id)
        {
            drop(state);
            session.terminate().await;
            if let Some(session_dir) = cleanup_session_dir.as_deref() {
                remove_empty_standard_session_directory(session_dir);
            }
            return Err(PiRpcFailureKind::Transport);
        }
        state.sessions.insert(session_id.clone(), session);
        drop(state);
        self.emit(PiRpcEvent::SessionCreated {
            generation,
            session_id: session_id.clone(),
        });
        self.emit(PiRpcEvent::SessionIdle {
            generation,
            session_id,
        });
        Ok(())
    }

    async fn session(
        &self,
        generation: u64,
        task_id: &str,
        session_id: &str,
    ) -> PortResult<Arc<PiSession>> {
        let session = {
            let state = self.state.lock().await;
            if state.generation != Some(generation) {
                return Err(PortError::new(
                    PortErrorKind::NotAvailable,
                    "Pi RPC generation is no longer active",
                ));
            }
            state.sessions.get(session_id).cloned().ok_or_else(|| {
                PortError::new(PortErrorKind::NotFound, "Pi RPC session is not available")
            })?
        };
        if session.terminated.load(Ordering::Acquire) {
            return Err(PortError::new(
                PortErrorKind::NotAvailable,
                "Pi RPC session has failed closed",
            ));
        }
        if session.current_task_id().await != task_id {
            return Err(PortError::new(
                PortErrorKind::PermissionDenied,
                "Pi RPC session task scope did not match",
            ));
        }
        Ok(session)
    }

    async fn shutdown_sessions(&self, generation: u64) -> Result<(), PiRpcFailureKind> {
        let _lifecycle = self.lifecycle.lock().await;
        let sessions = {
            let mut state = self.state.lock().await;
            match state.generation {
                Some(active_generation) if active_generation != generation => {
                    return Err(PiRpcFailureKind::Transport);
                }
                None => return Ok(()),
                Some(_) => {}
            }
            state.generation = None;
            state.workspace = None;
            state.executable = None;
            state.readiness_summary = None;
            state
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };

        for session in sessions {
            if session.running.load(Ordering::Acquire) {
                let _ = session.abort_with_grace().await;
            }
            session.terminate().await;
        }
        Ok(())
    }
}

#[async_trait]
impl PiRpcPort for PiRpcAdapter {
    async fn execute(&self, command: PiRpcCommand) -> PortResult<PiRpcReply> {
        match command {
            PiRpcCommand::Probe { .. } => match self.probe_pi().await {
                Ok(resolved) => Ok(PiRpcReply::Available {
                    summary: resolved.summary,
                }),
                Err(reason) => Ok(PiRpcReply::Unavailable { reason }),
            },
            PiRpcCommand::Start {
                generation,
                workspace,
            } => Ok(self.start(generation, workspace).await),
            PiRpcCommand::CreateSession {
                generation,
                task_id,
                session_id,
                mode,
            } => match self
                .create_session(generation, task_id, session_id, mode)
                .await
            {
                Ok(()) => Ok(PiRpcReply::Accepted),
                Err(reason) => Ok(PiRpcReply::Unavailable { reason }),
            },
            PiRpcCommand::SendUserInput {
                generation,
                task_id,
                session_id,
                content,
            } => {
                let session = self.session(generation, &task_id, &session_id).await?;
                let command = next_input_command(&session.prompt_sent);
                session
                    .request(command, json!({ "type": command, "message": content }))
                    .await?;
                session.prompt_accepted.store(true, Ordering::Release);
                session.running.store(true, Ordering::Release);
                self.emit(PiRpcEvent::SessionRunning {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::FollowUp {
                generation,
                task_id,
                session_id,
                content,
            } => {
                let session = self.session(generation, &task_id, &session_id).await?;
                if !session.prompt_accepted.load(Ordering::Acquire) {
                    return Err(PortError::new(
                        PortErrorKind::InvalidRequest,
                        "Pi RPC follow-up requires an accepted prompt",
                    ));
                }
                session
                    .request(
                        "follow_up",
                        json!({ "type": "follow_up", "message": content }),
                    )
                    .await?;
                session.running.store(true, Ordering::Release);
                self.emit(PiRpcEvent::SessionRunning {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::StopSession {
                generation,
                task_id,
                session_id,
            }
            | PiRpcCommand::AbortSession {
                generation,
                task_id,
                session_id,
            } => {
                let session = self.session(generation, &task_id, &session_id).await?;
                let cancellation_mode = session.abort_with_grace().await?;
                session.terminate().await;
                self.state.lock().await.sessions.remove(&session_id);
                self.emit(PiRpcEvent::SessionStopped {
                    generation,
                    session_id,
                    cancellation_mode,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::EndSession {
                generation,
                task_id,
                session_id,
            } => {
                let session = self.session(generation, &task_id, &session_id).await?;
                if session.running.load(Ordering::Acquire) {
                    let _ = session.abort_with_grace().await;
                }
                session.terminate().await;
                self.state.lock().await.sessions.remove(&session_id);
                self.emit(PiRpcEvent::SessionEnded {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::ResolveOperation {
                generation,
                task_id,
                session_id,
                operation_id,
                decision,
            } => {
                let session = self.session(generation, &task_id, &session_id).await?;
                let confirmed = match decision {
                    PiRpcOperationDecision::AllowOnce => true,
                    PiRpcOperationDecision::Deny => false,
                };
                let binding = {
                    let mut operations = session.operations.lock().await;
                    operations.remove(&operation_id)
                };
                let binding = match binding {
                    Some(binding) => binding,
                    None => {
                        // A stale, cross-task, or duplicated UI decision must
                        // never be treated as a harmless lookup miss. The
                        // request id is the capability that authorizes one
                        // response, so an id mismatch closes this Pi session.
                        session.fail_closed(PiRpcFailureKind::Protocol).await;
                        return Err(PortError::new(
                            PortErrorKind::NotFound,
                            "Pi RPC permission operation is no longer pending",
                        ));
                    }
                };
                // The operation id is a single-use Halo capability. Its
                // binding contains the one redacted tool-call digest created
                // for this request; no caller-supplied raw toolCallId is
                // accepted or compared at this seam.
                if binding.generation != generation
                    || binding.task_id != task_id
                    || binding.session_id != session_id
                {
                    session.fail_closed(PiRpcFailureKind::Protocol).await;
                    return Err(PortError::new(
                        PortErrorKind::PermissionDenied,
                        "Pi RPC permission operation scope did not match",
                    ));
                }
                if session
                    .send_extension_ui_response(&binding.ui_request_id, confirmed)
                    .await
                    .is_err()
                {
                    session.fail_closed(PiRpcFailureKind::Transport).await;
                    return Err(PortError::new(
                        PortErrorKind::Backend,
                        "Pi RPC permission response could not be sent",
                    ));
                }
                self.emit(PiRpcEvent::OperationResolved {
                    generation,
                    session_id,
                    operation_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::Shutdown { generation } => match self.shutdown_sessions(generation).await
            {
                Ok(()) => Ok(PiRpcReply::Accepted),
                Err(reason) => Ok(PiRpcReply::Unavailable { reason }),
            },
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<PiRpcEvent> {
        self.events.subscribe()
    }
}

impl PiSession {
    fn claim_cancellation(&self) -> PortResult<()> {
        match self.terminal_disposition.compare_exchange(
            SESSION_TERMINAL_OPEN,
            SESSION_TERMINAL_CANCELLING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(SESSION_TERMINAL_FAILED) => Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC session has already failed",
            )),
            Err(_) => Err(PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC session cancellation is already in progress",
            )),
        }
    }

    fn recorded_failure_reason(&self) -> Option<PiRpcFailureKind> {
        match self.failure_reason.load(Ordering::Acquire) {
            1 => Some(PiRpcFailureKind::NotInstalled),
            2 => Some(PiRpcFailureKind::UnsupportedVersion),
            3 => Some(PiRpcFailureKind::CapabilityMismatch),
            4 => Some(PiRpcFailureKind::Authentication),
            5 => Some(PiRpcFailureKind::Transport),
            6 => Some(PiRpcFailureKind::Protocol),
            7 => Some(PiRpcFailureKind::Internal),
            _ => None,
        }
    }

    async fn current_task_id(&self) -> String {
        self.task_id.lock().await.clone()
    }

    async fn current_session_id(&self) -> String {
        self.session_id.lock().await.clone()
    }

    async fn capture_tool_call(
        &self,
        raw_tool_call_id: &str,
        raw_tool_name: &str,
        bounded_tool_name: &str,
        value: &Value,
    ) {
        // `args` is present on the Pi RPC `tool_execution_start` event, but a
        // missing/empty object must still be a correlated permission request,
        // so the gate always records a bounded, redacted summary.
        let args = value.get("args").cloned().unwrap_or_else(|| json!({}));
        let redacted_arguments = redact_tool_arguments(&args);
        let risk_level = classify_tool_risk(raw_tool_name, &args);
        let mut tool_calls = self.tool_calls.lock().await;
        tool_calls.insert(
            raw_tool_call_id.to_string(),
            CapturedToolCall {
                tool_name: bounded_tool_name.to_string(),
                raw_tool_name: raw_tool_name.to_string(),
                redacted_arguments,
                risk_level,
            },
        );
    }

    async fn forget_tool_call(&self, raw_tool_call_id: &str) {
        let mut tool_calls = self.tool_calls.lock().await;
        tool_calls.remove(raw_tool_call_id);
    }

    async fn request(self: &Arc<Self>, command: &str, payload: Value) -> PortResult<Value> {
        self.request_with_timeout(command, payload, self.response_timeout)
            .await
    }

    async fn request_with_timeout(
        self: &Arc<Self>,
        command: &str,
        payload: Value,
        response_timeout: Duration,
    ) -> PortResult<Value> {
        self.request_with_timeout_inner(command, payload, response_timeout, true)
            .await
    }

    async fn request_with_timeout_inner(
        self: &Arc<Self>,
        command: &str,
        payload: Value,
        response_timeout: Duration,
        fail_closed_on_transport_error: bool,
    ) -> PortResult<Value> {
        let mut payload = payload.as_object().cloned().ok_or_else(|| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC command must be an object",
            )
        })?;
        let request_id = Uuid::new_v4().to_string();
        payload.insert("id".to_string(), Value::String(request_id.clone()));
        let encoded = encode_jsonl(&Value::Object(payload)).map_err(|_| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC command could not be encoded",
            )
        })?;
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(request_id.clone(), sender);

        let write_result = async {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(&encoded).await?;
            stdin.flush().await
        }
        .await;
        if write_result.is_err() {
            self.pending.lock().await.remove(&request_id);
            if fail_closed_on_transport_error && !self.terminated.load(Ordering::Acquire) {
                self.fail_closed(PiRpcFailureKind::Transport).await;
            }
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC stdin is unavailable",
            ));
        }

        let response = match timeout(response_timeout, receiver).await {
            Ok(Ok(Ok(response))) => response,
            Ok(Ok(Err(error))) => return Err(error),
            Ok(Err(_)) => {
                return Err(PortError::new(
                    PortErrorKind::Backend,
                    "Pi RPC response stream closed",
                ));
            }
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                if fail_closed_on_transport_error && !self.terminated.load(Ordering::Acquire) {
                    self.fail_closed(PiRpcFailureKind::Transport).await;
                }
                return Err(PortError::new(
                    PortErrorKind::Timeout,
                    "Pi RPC response timed out",
                ));
            }
        };
        if response.get("command").and_then(Value::as_str) != Some(command) {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC response command did not match the request",
            ));
        }
        let Some(success) = response.get("success").and_then(Value::as_bool) else {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC response did not declare success",
            ));
        };
        if !success {
            return Err(PortError::new(
                PortErrorKind::Backend,
                "Pi RPC command was rejected",
            ));
        }
        Ok(response.get("data").cloned().unwrap_or(Value::Null))
    }

    async fn send_extension_ui_response(
        &self,
        request_id: &str,
        confirmed: bool,
    ) -> PortResult<()> {
        let value = json!({
            "type": "extension_ui_response",
            "id": request_id,
            "confirmed": confirmed,
        });
        let encoded = encode_jsonl(&value).map_err(|_| {
            PortError::new(
                PortErrorKind::InvalidRequest,
                "Pi RPC extension response could not be encoded",
            )
        })?;
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(&encoded)
            .await
            .map_err(|_| PortError::new(PortErrorKind::Backend, "Pi RPC stdin is unavailable"))?;
        stdin
            .flush()
            .await
            .map_err(|_| PortError::new(PortErrorKind::Backend, "Pi RPC stdin is unavailable"))
    }

    async fn abort_with_grace(self: &Arc<Self>) -> PortResult<PiRpcCancellationMode> {
        self.claim_cancellation()?;
        if !self.running.load(Ordering::Acquire) {
            return Ok(PiRpcCancellationMode::Native);
        }
        let observed_settlement = self.settled_epoch.load(Ordering::Acquire);
        let deadline = Instant::now() + self.abort_grace_period;
        let request_timeout = self.abort_grace_period.min(self.response_timeout);
        if self
            .request_with_timeout_inner("abort", json!({ "type": "abort" }), request_timeout, false)
            .await
            .is_err()
        {
            self.terminate().await;
            return Ok(PiRpcCancellationMode::Forced);
        }
        if !self.running.load(Ordering::Acquire) {
            return Ok(PiRpcCancellationMode::Native);
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        let settled = timeout(remaining, async {
            loop {
                let notified = self.settled.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if !self.running.load(Ordering::Acquire)
                    || self.settled_epoch.load(Ordering::Acquire) != observed_settlement
                {
                    break;
                }
                notified.await;
            }
        })
        .await
        .is_ok();
        if settled && !self.running.load(Ordering::Acquire) {
            return Ok(PiRpcCancellationMode::Native);
        }
        if self.running.load(Ordering::Acquire) {
            // Explicit abort is allowed to force-reclaim a stuck child after
            // the bounded grace period. This path never reports success as a
            // completed Pi run; it only closes the owned process.
            self.terminate().await;
        }
        Ok(PiRpcCancellationMode::Forced)
    }

    async fn handle_extension_ui_request(self: &Arc<Self>, value: &Value) {
        let Some(request_id) = value.get("id").and_then(Value::as_str) else {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        };
        if request_id.is_empty() || value.get("method").and_then(Value::as_str) != Some("confirm") {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        }
        let Some(message) = value.get("message").and_then(Value::as_str) else {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        };
        let notice = match serde_json::from_str::<PermissionNotice>(message) {
            Ok(notice) if !notice.tool_call_id.is_empty() && !notice.tool_name.is_empty() => notice,
            _ => {
                self.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            }
        };

        let captured = {
            let mut tool_calls = self.tool_calls.lock().await;
            let Some(captured) = tool_calls.remove(&notice.tool_call_id) else {
                drop(tool_calls);
                self.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            };
            captured
        };
        if captured.raw_tool_name != notice.tool_name {
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        }
        let summary = PiRpcOperationSummary {
            tool_name: captured.tool_name,
            arguments: captured.redacted_arguments,
            risk_level: captured.risk_level,
        };

        let task_id = self.current_task_id().await;
        let session_id = self.current_session_id().await;
        let operation_id = format!("pi-operation-{}", Uuid::new_v4());
        let binding = PiOperationBinding {
            generation: self.generation,
            task_id: task_id.clone(),
            session_id: session_id.clone(),
            ui_request_id: request_id.to_string(),
            redacted_tool_call_id: redact_tool_call_id(
                self.generation,
                &task_id,
                &session_id,
                &notice.tool_call_id,
            ),
        };
        let mut seen_requests = self.seen_extension_requests.lock().await;
        if !seen_requests.insert(request_id.to_string()) {
            drop(seen_requests);
            self.fail_closed(PiRpcFailureKind::Protocol).await;
            return;
        }
        drop(seen_requests);
        let mut operations = self.operations.lock().await;
        let redacted_tool_call_id = binding.redacted_tool_call_id.clone();
        operations.insert(operation_id.clone(), binding);
        drop(operations);

        let _ = self.events.send(PiRpcEvent::OperationRequested {
            generation: self.generation,
            session_id: session_id.clone(),
            operation_id: operation_id.clone(),
            kind: PiRpcOperationKind::Permission,
            summary,
            redacted_tool_call_id: Some(redacted_tool_call_id),
        });

        let timeout_duration = bounded_operation_timeout(value, self.operation_timeout);
        let session = Arc::clone(self);
        tokio::spawn(async move {
            sleep(timeout_duration).await;
            let binding = session.operations.lock().await.remove(&operation_id);
            let Some(binding) = binding else { return };
            if session
                .send_extension_ui_response(&binding.ui_request_id, false)
                .await
                .is_err()
            {
                session.fail_closed(PiRpcFailureKind::Transport).await;
                return;
            }
            let _ = session.events.send(PiRpcEvent::OperationResolved {
                generation: binding.generation,
                session_id: binding.session_id,
                operation_id,
            });
        });
    }

    async fn fail_protocol(self: &Arc<Self>, reason: PiRpcFailureKind) {
        let reason_code = match reason {
            PiRpcFailureKind::NotInstalled => 1,
            PiRpcFailureKind::UnsupportedVersion => 2,
            PiRpcFailureKind::CapabilityMismatch => 3,
            PiRpcFailureKind::Authentication => 4,
            PiRpcFailureKind::Transport => 5,
            PiRpcFailureKind::Protocol => 6,
            PiRpcFailureKind::Internal => 7,
        };
        if self
            .terminal_disposition
            .compare_exchange(
                SESSION_TERMINAL_OPEN,
                SESSION_TERMINAL_FAILED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return;
        }
        self.failure_reason.store(reason_code, Ordering::Release);
        self.running.store(false, Ordering::Release);
        self.settled.notify_waiters();
        let message = match reason {
            PiRpcFailureKind::Transport => "Pi RPC transport failure",
            _ => "Pi RPC protocol error",
        };
        self.fail_pending(message).await;
        self.operations.lock().await.clear();
        if self.is_readiness_probe {
            return;
        }
        let session_id = self.current_session_id().await;
        if let Some(adapter_state) = self.adapter_state.upgrade() {
            let mut state = adapter_state.lock().await;
            state
                .sessions
                .retain(|_, session| !Arc::ptr_eq(session, self));
        }
        let _ = self.events.send(PiRpcEvent::SessionFailed {
            generation: self.generation,
            session_id,
            reason,
        });
    }

    async fn fail_closed(self: &Arc<Self>, reason: PiRpcFailureKind) {
        self.fail_protocol(reason).await;
        self.terminate().await;
    }

    async fn fail_pending(&self, message: &str) {
        let pending = std::mem::take(&mut *self.pending.lock().await);
        for (_, sender) in pending {
            let _ = sender.send(Err(PortError::new(PortErrorKind::Backend, message)));
        }
    }

    async fn terminate(&self) {
        if self.terminated.swap(true, Ordering::AcqRel) {
            return;
        }
        self.running.store(false, Ordering::Release);
        self.settled.notify_waiters();
        self.fail_pending("Pi RPC session closed").await;
        self.operations.lock().await.clear();
        let mut stdin = self.stdin.lock().await;
        let _ = stdin.shutdown().await;
        drop(stdin);
        let mut child = self.child.lock().await;
        let _ = child.terminate(Duration::ZERO).await;
    }

    async fn has_exited(&self) -> bool {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => true,
            Ok(None) => false,
        }
    }
}

async fn read_pi_stdout(session: Arc<PiSession>, stdout: ChildStdout) {
    let mut reader = BufReader::new(stdout);
    let mut record = Vec::new();
    loop {
        record.clear();
        let read = match reader.read_until(b'\n', &mut record).await {
            Ok(read) => read,
            Err(_) => {
                session.fail_closed(PiRpcFailureKind::Transport).await;
                break;
            }
        };
        if read == 0 {
            if !session.terminated.load(Ordering::Acquire) {
                session.fail_closed(PiRpcFailureKind::Transport).await;
            }
            break;
        }
        if record.last() != Some(&b'\n') || record.len() == 1 {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
            break;
        }
        record.pop();
        match decode_jsonl_record(&record) {
            Ok(value) => handle_pi_message(&session, &value).await,
            Err(_) => {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                break;
            }
        }
    }
}

async fn handle_pi_message(session: &Arc<PiSession>, value: &Value) {
    match value.get("type").and_then(Value::as_str) {
        Some("response") => {
            let response_id = match value.get("id") {
                None => None,
                Some(Value::String(id)) if !id.is_empty() => Some(id.as_str()),
                Some(_) => {
                    session.fail_closed(PiRpcFailureKind::Protocol).await;
                    return;
                }
            };
            let sender = {
                let mut pending = session.pending.lock().await;
                match response_id {
                    Some(id) => pending.remove(id),
                    None if pending.len() == 1 => pending.drain().next().map(|(_, sender)| sender),
                    None => None,
                }
            };
            if let Some(sender) = sender {
                let _ = sender.send(Ok(value.clone()));
            } else {
                // An unknown id, an id-less response with multiple pending
                // requests, or an unsolicited response is a protocol error.
                session.fail_closed(PiRpcFailureKind::Protocol).await;
            }
        }
        Some("agent_start") => {
            session.running.store(true, Ordering::Release);
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::SessionRunning {
                generation: session.generation,
                session_id,
            });
        }
        Some("agent_end") => {
            // agent_end is deliberately not a settlement signal. Pi may still
            // retry, compact, or consume queued continuation messages.
        }
        Some("agent_settled") => {
            session.running.store(false, Ordering::Release);
            session.settled_epoch.fetch_add(1, Ordering::AcqRel);
            session.settled.notify_waiters();
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::AgentSettled {
                generation: session.generation,
                session_id,
            });
        }
        Some("message_update") => {
            if !valid_message_update(value) {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            }
            let text = extract_assistant_delta(value);
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::MessageUpdated {
                generation: session.generation,
                session_id,
                text,
            });
        }
        Some("tool_execution_start") => {
            emit_tool_event(session, value, ToolEventKind::Started).await;
        }
        Some("tool_execution_update") => {
            emit_tool_event(session, value, ToolEventKind::Updated).await;
        }
        Some("tool_execution_end") => {
            if value.get("isError").and_then(Value::as_bool).is_none() {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            }
            emit_tool_event(session, value, ToolEventKind::Ended).await;
        }
        Some("extension_ui_request") => session.handle_extension_ui_request(value).await,
        Some("extension_error") => {
            session.fail_closed(PiRpcFailureKind::Protocol).await;
        }
        _ => {
            // Pi may add events over time, but an unrecognized record cannot
            // be safely projected into Halo state. Keeping the process alive
            // would create a false-ready or false-settled seam.
            session.fail_closed(PiRpcFailureKind::Protocol).await;
        }
    }
}

enum ToolEventKind {
    Started,
    Updated,
    Ended,
}

async fn emit_tool_event(session: &Arc<PiSession>, value: &Value, kind: ToolEventKind) {
    let Some(tool_call_id) = value.get("toolCallId").and_then(Value::as_str) else {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    };
    if tool_call_id.is_empty() {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    }
    let Some(raw_tool_name) = value.get("toolName").and_then(Value::as_str) else {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    };
    let Some(tool_name) = bounded_protocol_label(raw_tool_name, MAX_TOOL_NAME_BYTES) else {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    };
    let session_id = session.current_session_id().await;
    let task_id = session.current_task_id().await;
    let redacted_tool_call_id =
        redact_tool_call_id(session.generation, &task_id, &session_id, tool_call_id);
    match kind {
        ToolEventKind::Started => {
            session
                .capture_tool_call(tool_call_id, raw_tool_name, &tool_name, value)
                .await;
        }
        ToolEventKind::Ended => {
            session.forget_tool_call(tool_call_id).await;
        }
        ToolEventKind::Updated => {}
    }
    let event = match kind {
        ToolEventKind::Started => PiRpcEvent::ToolExecutionStarted {
            generation: session.generation,
            session_id: session_id.clone(),
            redacted_tool_call_id,
            tool_name: tool_name.clone(),
        },
        ToolEventKind::Updated => PiRpcEvent::ToolExecutionUpdated {
            generation: session.generation,
            session_id: session_id.clone(),
            redacted_tool_call_id,
            tool_name: tool_name.clone(),
        },
        ToolEventKind::Ended => {
            let Some(is_error) = value.get("isError").and_then(Value::as_bool) else {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return;
            };
            PiRpcEvent::ToolExecutionEnded {
                generation: session.generation,
                session_id,
                redacted_tool_call_id,
                tool_name,
                is_error,
            }
        }
    };
    let _ = session.events.send(event);
}

fn valid_message_update(value: &Value) -> bool {
    value.get("message").is_some_and(Value::is_object)
        && value
            .get("assistantMessageEvent")
            .and_then(Value::as_object)
            .and_then(|event| event.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|event_type| SUPPORTED_ASSISTANT_MESSAGE_EVENT_TYPES.contains(&event_type))
}

/// Projects only the assistant text delta from a Pi message event. Thinking,
/// tool-call arguments, partial message objects and the original event are
/// deliberately discarded at this boundary.
fn extract_assistant_delta(value: &Value) -> String {
    let event = value
        .get("assistantMessageEvent")
        .and_then(Value::as_object);
    let event_type = event
        .and_then(|event| event.get("type"))
        .and_then(Value::as_str);
    if !matches!(event_type, Some("text_delta" | "text_start" | "text_end")) {
        return String::new();
    }
    event
        .and_then(|event| event.get("delta").or_else(|| event.get("content")))
        .and_then(Value::as_str)
        .map(redact_assistant_text)
        .unwrap_or_default()
}

fn bounded_protocol_label(value: &str, max_bytes: usize) -> Option<String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(truncate_utf8(&redact_assistant_text(value), max_bytes))
}

/// Redacts high-confidence credential forms before assistant text enters the
/// Halo event stream. This is intentionally conservative: ordinary prose is
/// preserved, while common bearer/API-key prefixes and key/value secrets are
/// replaced without retaining the original token.
fn redact_assistant_text(value: &str) -> String {
    let mut redacted = value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect::<String>();
    for header in ["authorization", "cookie"] {
        redacted = redact_header_values(&redacted, header);
    }
    for prefix in ["sk-", "sk_", "ghp_", "github_pat_", "xoxb-", "AIza"] {
        redacted = redact_prefixed_token(&redacted, prefix);
    }
    redacted = redact_literal_value(&redacted, "bearer ");
    for name in [
        "api-key",
        "api_key",
        "secret",
        "token",
        "password",
        "sessionid",
        "entryid",
        "toolcallid",
        "session_id",
        "entry_id",
        "tool_call_id",
    ] {
        redacted = redact_named_values(&redacted, name);
    }
    truncate_utf8(&redacted, MAX_ASSISTANT_DELTA_BYTES)
}

fn redact_header_values(value: &str, header: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let Some(start) = find_named_marker(&redacted, header, cursor) else {
            break;
        };
        let mut delimiter = start + header.len();
        if redacted[delimiter..].starts_with('"') || redacted[delimiter..].starts_with('\'') {
            delimiter += 1;
        }
        delimiter = skip_horizontal_whitespace(&redacted, delimiter);
        if !redacted[delimiter..].starts_with(':') && !redacted[delimiter..].starts_with('=') {
            cursor = delimiter;
            continue;
        }
        let value_start = skip_horizontal_whitespace(&redacted, delimiter + 1);
        let value_end = header_value_end(&redacted, value_start);
        if value_start == value_end {
            cursor = value_start;
            continue;
        }
        redacted.replace_range(value_start..value_end, "[redacted]");
        cursor = value_start + "[redacted]".len();
    }
    redacted
}

fn redact_named_values(value: &str, name: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let Some(start) = find_named_marker(&redacted, name, cursor) else {
            break;
        };
        let mut delimiter = start + name.len();
        if redacted[delimiter..].starts_with('"') || redacted[delimiter..].starts_with('\'') {
            delimiter += 1;
        }
        delimiter = skip_horizontal_whitespace(&redacted, delimiter);
        if !redacted[delimiter..].starts_with(':') && !redacted[delimiter..].starts_with('=') {
            cursor = delimiter;
            continue;
        }
        let mut value_start = skip_horizontal_whitespace(&redacted, delimiter + 1);
        let quote = redacted[value_start..]
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\'' | '`'));
        if let Some(quote) = quote {
            value_start += quote.len_utf8();
            let value_end = quoted_value_end(&redacted, value_start, quote);
            if value_start != value_end {
                redacted.replace_range(value_start..value_end, "[redacted]");
                cursor = value_start + "[redacted]".len();
                continue;
            }
        } else {
            let value_end = token_value_end(&redacted, value_start);
            if value_start != value_end {
                redacted.replace_range(value_start..value_end, "[redacted]");
                cursor = value_start + "[redacted]".len();
                continue;
            }
        }
        cursor = value_start;
    }
    redacted
}

fn redact_literal_value(value: &str, marker: &str) -> String {
    let mut redacted = value.to_string();
    let mut cursor = 0;
    while cursor < redacted.len() {
        let lower = redacted[cursor..].to_ascii_lowercase();
        let Some(relative) = lower.find(marker) else {
            break;
        };
        let value_start = cursor + relative + marker.len();
        let value_end = token_value_end(&redacted, value_start);
        if value_start == value_end {
            cursor = value_start;
            continue;
        }
        redacted.replace_range(value_start..value_end, "[redacted]");
        cursor = value_start + "[redacted]".len();
    }
    redacted
}

fn find_named_marker(value: &str, name: &str, mut cursor: usize) -> Option<usize> {
    while cursor < value.len() {
        let lower = value[cursor..].to_ascii_lowercase();
        let relative = lower.find(name)?;
        let start = cursor + relative;
        let end = start + name.len();
        if identifier_boundary(value, start, end) {
            return Some(start);
        }
        cursor = end;
    }
    None
}

fn identifier_boundary(value: &str, start: usize, end: usize) -> bool {
    let before = value[..start].chars().next_back();
    let after = value[end..].chars().next();
    !before.is_some_and(is_identifier_character) && !after.is_some_and(is_identifier_character)
}

fn is_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn skip_horizontal_whitespace(value: &str, mut cursor: usize) -> usize {
    while let Some(character) = value[cursor..].chars().next() {
        if !matches!(character, ' ' | '\t') {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

fn header_value_end(value: &str, value_start: usize) -> usize {
    for (offset, character) in value[value_start..].char_indices() {
        let cursor = value_start + offset;
        if matches!(character, '\n' | '\r') {
            return cursor;
        }
        if cursor > value_start
            && value[..cursor]
                .chars()
                .next_back()
                .is_some_and(|previous| matches!(previous, ' ' | '\t'))
            && is_inline_sensitive_key(value, cursor)
        {
            let mut boundary = cursor;
            while boundary > value_start
                && value[..boundary]
                    .chars()
                    .next_back()
                    .is_some_and(|previous| matches!(previous, ' ' | '\t'))
            {
                boundary -= value[..boundary]
                    .chars()
                    .next_back()
                    .expect("boundary has a preceding character")
                    .len_utf8();
            }
            return boundary;
        }
    }
    value.len()
}

fn is_inline_sensitive_key(value: &str, cursor: usize) -> bool {
    [
        "authorization",
        "cookie",
        "api-key",
        "api_key",
        "secret",
        "token",
        "password",
        "sessionid",
        "entryid",
        "toolcallid",
        "session_id",
        "entry_id",
        "tool_call_id",
    ]
    .into_iter()
    .any(|name| {
        find_named_marker(value, name, cursor) == Some(cursor)
            && named_marker_has_value_delimiter(value, cursor, name)
    })
}

fn named_marker_has_value_delimiter(value: &str, start: usize, name: &str) -> bool {
    let mut cursor = start + name.len();
    if value[cursor..].starts_with('"') || value[cursor..].starts_with('\'') {
        cursor += 1;
    }
    cursor = skip_horizontal_whitespace(value, cursor);
    value[cursor..].starts_with(':') || value[cursor..].starts_with('=')
}

fn quoted_value_end(value: &str, value_start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (offset, character) in value[value_start..].char_indices() {
        if character == quote && !escaped {
            return value_start + offset;
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    value.len()
}

fn token_value_end(value: &str, value_start: usize) -> usize {
    value[value_start..]
        .char_indices()
        .find(|(_, character)| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '`' | ',' | ';' | '}' | ']')
        })
        .map(|(offset, _)| value_start + offset)
        .unwrap_or(value.len())
}

fn redact_prefixed_token(value: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative) = value[cursor..].find(prefix) {
        let start = cursor + relative;
        result.push_str(&value[cursor..start]);
        let end = value[start..]
            .char_indices()
            .find(|(_, character)| {
                character.is_whitespace() || matches!(character, '"' | '\'' | '`' | ',' | ';')
            })
            .map(|(offset, _)| start + offset)
            .unwrap_or(value.len());
        result.push_str("[redacted]");
        cursor = end;
        if cursor >= value.len() {
            break;
        }
    }
    result.push_str(&value[cursor..]);
    result
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn stable_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

fn default_persistent_session_root() -> PathBuf {
    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("Halo Studio")
            .join("pi-sessions");
    }

    #[cfg(not(windows))]
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home)
            .join("halo-studio")
            .join("pi-sessions");
    }

    std::env::temp_dir().join("halo-studio").join("pi-sessions")
}

/// Failed standard-session startup must not leave an empty task directory, but
/// a directory containing prior Pi history is persistent by contract. Only
/// remove directories that are provably empty and remain below the adapter's
/// task-scoped path; never recursively delete a session root.
fn remove_empty_standard_session_directory(path: &Path) {
    let is_empty = std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    if !is_empty || std::fs::remove_dir(path).is_err() {
        return;
    }

    let Some(workspace_root) = path.parent() else {
        return;
    };
    let workspace_is_empty = std::fs::read_dir(workspace_root)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false);
    if workspace_is_empty {
        let _ = std::fs::remove_dir(workspace_root);
    }
}

fn next_input_command(prompt_sent: &AtomicBool) -> &'static str {
    if prompt_sent.swap(true, Ordering::AcqRel) {
        "follow_up"
    } else {
        "prompt"
    }
}

fn valid_cli_selection(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value.len() <= 256
        && value
            .chars()
            .all(|character| !character.is_control() && character != '\\')
}

fn bounded_operation_timeout(value: &Value, configured: Duration) -> Duration {
    let requested = value
        .get("timeout")
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(configured);
    requested.min(configured)
}

fn redact_tool_call_id(generation: u64, task_id: &str, session_id: &str, value: &str) -> String {
    format!(
        "tool-{}",
        stable_digest(&format!("{generation}:{task_id}:{session_id}:{value}"))
    )
}

/// Browser / Computer Use tool families that can produce worktree-external
/// side effects. P0 still gates every tool; the risk level only makes those
/// decisions more prominent in Halo UI.
const BROWSER_COMPUTER_TOOL_NAME_MARKERS: &[&str] = &[
    "browser",
    "computer",
    "computer_use",
    "computer-use",
    "playwright",
    "browser_action",
    "webdriver",
];

/// Argument keys that indicate an external side effect when observed on a
/// browser / Computer Use tool call.
const EXTERNAL_SIDE_EFFECT_ARG_MARKERS: &[&str] = &[
    "write",
    "commit",
    "submit",
    "upload",
    "download",
    "clipboard",
    "paste",
    "process",
    "exec",
    "command",
    "shell",
    "system",
    "run",
];

fn classify_tool_risk(tool_name: &str, args: &Value) -> PiRpcOperationRiskLevel {
    let normalized = tool_name.to_ascii_lowercase().replace('_', "-");
    let is_browser_or_computer = BROWSER_COMPUTER_TOOL_NAME_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker));
    if !is_browser_or_computer {
        return PiRpcOperationRiskLevel::Standard;
    }
    if args_contain_external_side_effect(args) {
        PiRpcOperationRiskLevel::HighRisk
    } else {
        PiRpcOperationRiskLevel::Standard
    }
}

fn args_contain_external_side_effect(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let lower = key.to_ascii_lowercase();
            EXTERNAL_SIDE_EFFECT_ARG_MARKERS
                .iter()
                .any(|marker| lower.contains(marker))
                || args_contain_external_side_effect(value)
        }),
        Value::Array(items) => items.iter().any(args_contain_external_side_effect),
        _ => false,
    }
}

/// Redacts raw tool arguments into a bounded, renderer-safe summary. Raw
/// parameters, credentials, paths, and Pi identifiers never survive this
/// function.
fn redact_tool_arguments(value: &Value) -> String {
    let redacted = redact_tool_value(value);
    truncate_utf8(
        &serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".to_string()),
        MAX_TOOL_ARGUMENTS_BYTES,
    )
}

fn redact_tool_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in map {
                if is_sensitive_tool_key(key) {
                    redacted.insert(key.clone(), json!("[redacted]"));
                } else {
                    redacted.insert(key.clone(), redact_tool_value(value));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_tool_value).collect()),
        Value::String(value) => {
            if looks_like_sensitive_tool_string(value) {
                json!("[redacted]")
            } else {
                Value::String(redact_assistant_text(value))
            }
        }
        other => other.clone(),
    }
}

fn is_sensitive_tool_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "api-key",
        "credential",
        "sessionid",
        "entryid",
        "toolcallid",
        "answer",
    ]
    .into_iter()
    .any(|marker| lower.contains(marker))
}

fn looks_like_sensitive_tool_string(value: &str) -> bool {
    let trimmed = value.trim();
    let looks_like_path = trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || (trimmed.len() >= 3
            && trimmed.as_bytes()[1] == b':'
            && matches!(trimmed.as_bytes()[0], b'a'..=b'z' | b'A'..=b'Z'))
        || trimmed.starts_with("~/");
    let looks_like_url = trimmed.contains("://");
    let looks_like_secret = [
        "bearer ",
        "sk-",
        "sk_",
        "ghp_",
        "github_pat_",
        "xoxb-",
        "AIza",
    ]
    .into_iter()
    .any(|prefix| trimmed.to_ascii_lowercase().starts_with(prefix));
    looks_like_path || looks_like_url || looks_like_secret
}

fn validate_state_data(value: &Value) -> Result<(), ()> {
    let object = value.as_object().ok_or(())?;
    let is_streaming = object
        .get("isStreaming")
        .and_then(Value::as_bool)
        .ok_or(())?;
    let is_compacting = object
        .get("isCompacting")
        .and_then(Value::as_bool)
        .ok_or(())?;
    if is_streaming || is_compacting {
        return Err(());
    }
    Ok(())
}

#[derive(Debug)]
struct EntriesData {
    ids: HashSet<String>,
    leaf_id: Option<String>,
}

fn parse_entries_data(value: &Value) -> Result<EntriesData, ()> {
    let object = value.as_object().ok_or(())?;
    let entries = object.get("entries").and_then(Value::as_array).ok_or(())?;
    let mut ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or(())?;
        if !ids.insert(id.to_string()) {
            return Err(());
        }
    }
    let leaf_id = object.get("leafId").ok_or(())?;
    if !leaf_id.is_null() && leaf_id.as_str().is_none_or(str::is_empty) {
        return Err(());
    }
    Ok(EntriesData {
        ids,
        leaf_id: leaf_id.as_str().map(str::to_string),
    })
}

fn validate_entries_data(value: &Value) -> Result<EntriesData, ()> {
    let entries = parse_entries_data(value)?;
    match (&entries.leaf_id, entries.ids.is_empty()) {
        (None, true) => Ok(entries),
        (Some(leaf_id), false) if entries.ids.contains(leaf_id) => Ok(entries),
        _ => Err(()),
    }
}

fn validate_incremental_entries_data(value: &Value, cursor: &str) -> Result<(), ()> {
    let entries = parse_entries_data(value)?;
    if entries.ids.contains(cursor) {
        return Err(());
    }
    match (&entries.leaf_id, entries.ids.is_empty()) {
        (Some(leaf_id), false) if entries.ids.contains(leaf_id) => Ok(()),
        (Some(leaf_id), true) if leaf_id == cursor => Ok(()),
        _ => Err(()),
    }
}

fn pi_rpc_args(
    extension_path: Option<&Path>,
    mode: PiRpcSessionMode,
    session_dir: Option<&Path>,
    provider: Option<&str>,
    model: Option<&str>,
    thinking: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["--mode".to_string(), "rpc".to_string()];
    match mode {
        PiRpcSessionMode::Managed => args.push("--no-session".to_string()),
        PiRpcSessionMode::Standard => {
            let Some(session_dir) = session_dir else {
                return Vec::new();
            };
            args.extend([
                "--session-dir".to_string(),
                session_dir.to_string_lossy().into_owned(),
            ]);
        }
    }
    args.extend(["--no-approve".to_string(), "--no-extensions".to_string()]);
    if let Some(extension_path) = extension_path {
        args.extend([
            "--extension".to_string(),
            extension_path.to_string_lossy().into_owned(),
        ]);
    }
    if let Some(provider) = provider {
        args.extend(["--provider".to_string(), provider.to_string()]);
    }
    if let Some(model) = model {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    if let Some(thinking) = thinking {
        if let Some(model_index) = args.iter().position(|argument| argument == "--model") {
            if let Some(model_value) = args.get_mut(model_index + 1) {
                model_value.push(':');
                model_value.push_str(thinking);
            }
        }
    }
    args
}

/// Projects Halo's validated, non-secret configuration into the small set of
/// Pi RPC startup arguments that P0 permits. The base URL is intentionally
/// excluded from argv; callers project it into the adapter-owned Pi config
/// directory instead. Credentials are never represented in this vector.
pub fn pi_rpc_arguments(
    configuration: &bitfun_runtime_ports::PiRuntimeConfiguration,
    mode: PiRpcSessionMode,
    extension_path: &str,
    session_dir: Option<&str>,
) -> Vec<String> {
    if validate_runtime_configuration_shape(configuration).is_err()
        || !valid_cli_selection(&configuration.provider_id)
        || !valid_cli_selection(&configuration.model_id)
    {
        return Vec::new();
    }
    pi_rpc_args(
        Some(Path::new(extension_path)),
        mode,
        session_dir.map(Path::new),
        Some(&configuration.provider_id),
        Some(&configuration.model_id),
        Some(configuration.thinking_level.as_str()),
    )
}

pub fn pi_models_json_projection(
    configuration: &PiRuntimeConfiguration,
    capability: Option<&PiProviderCapability>,
) -> Result<Value, PiRpcFailureKind> {
    validate_runtime_configuration_shape(configuration)
        .map_err(|_| PiRpcFailureKind::CapabilityMismatch)?;
    if let Some(capability) = capability {
        if capability.provider_id != configuration.provider_id
            || capability.model_id != configuration.model_id
            || capability.api.is_empty()
            || capability.api.len() > 128
            || capability
                .api
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
    }

    let mut provider = serde_json::Map::new();
    provider.insert(
        "apiKey".to_string(),
        Value::String(HALO_PI_CREDENTIAL_ENV_REFERENCE.to_string()),
    );
    provider.insert("authHeader".to_string(), Value::Bool(true));
    if let Some(base_url) = configuration.base_url.as_deref() {
        provider.insert("baseUrl".to_string(), Value::String(base_url.to_string()));
    }
    if let Some(capability) = capability {
        let reasoning = capability
            .supported_thinking_levels
            .iter()
            .any(|level| *level != bitfun_runtime_ports::PiThinkingLevel::Off);
        provider.insert("api".to_string(), Value::String(capability.api.clone()));
        provider.insert(
            "models".to_string(),
            json!([{
                "id": configuration.model_id,
                // Only facts owned by Halo's selected configuration and the
                // audited capability port belong in this projection. Pi may
                // discover richer metadata; inventing context, token limits,
                // modalities, or pricing here would create false provenance.
                "reasoning": reasoning
            }]),
        );
    }

    let mut providers = serde_json::Map::new();
    providers.insert(configuration.provider_id.clone(), Value::Object(provider));
    Ok(Value::Object(serde_json::Map::from_iter([(
        "providers".to_string(),
        Value::Object(providers),
    )])))
}

fn write_pi_config_projection(
    config_dir: &Path,
    configuration: &PiRuntimeConfiguration,
    capability: Option<&PiProviderCapability>,
) -> Result<(), PiRpcFailureKind> {
    let projection = pi_models_json_projection(configuration, capability)?;
    let encoded = serde_json::to_vec(&projection).map_err(|_| PiRpcFailureKind::Internal)?;
    std::fs::write(config_dir.join("models.json"), encoded).map_err(|_| PiRpcFailureKind::Internal)
}

fn map_handshake_error(error: PortError) -> PiRpcFailureKind {
    if matches!(
        error.message.as_str(),
        "Pi RPC transport failure"
            | "Pi RPC stdin is unavailable"
            | "Pi RPC response stream closed"
    ) {
        return PiRpcFailureKind::Transport;
    }
    match error.kind {
        PortErrorKind::Timeout
        | PortErrorKind::NotAvailable
        | PortErrorKind::NotFound
        | PortErrorKind::Cancelled
        | PortErrorKind::SessionInUse
        | PortErrorKind::CleanupRequired => PiRpcFailureKind::Transport,
        PortErrorKind::InvalidRequest | PortErrorKind::PermissionDenied => {
            PiRpcFailureKind::Protocol
        }
        PortErrorKind::Backend => PiRpcFailureKind::Protocol,
    }
}

fn parse_command_paths(bytes: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn availability_summary_from_version_output(
    output: &std::process::Output,
) -> Option<PiRpcAvailabilitySummary> {
    [output.stdout.as_slice(), output.stderr.as_slice()]
        .into_iter()
        .find_map(|bytes| {
            let value = String::from_utf8(bytes.to_vec()).ok()?;
            let value = value.trim();
            SUPPORTED_PI_RPC_PROFILES
                .iter()
                .find_map(|(literal, version)| {
                    (*literal == value).then(|| {
                        PiRpcAvailabilitySummary::new(
                            *version,
                            PiRpcVersionEvidenceSource::LocalVersionProbe,
                        )
                    })
                })
        })
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if !result.iter().any(|existing| existing == &path) {
            result.push(path);
        }
    }
    result
}

fn configure_child_command(
    mut command: Command,
    executable: &Path,
    config_dir: Option<&Path>,
    credential: Option<&str>,
) -> Command {
    command.env_clear();
    for name in SAFE_CHILD_ENVIRONMENT {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if let Some(config_dir) = config_dir {
        command.env("PI_CODING_AGENT_DIR", config_dir);
    }
    if let Some(credential) = credential {
        command.env(HALO_PI_CREDENTIAL_ENV, credential);
    }

    // The fixture is a controlled test executable, not a production Pi
    // process. Passing its synthetic mode explicitly keeps integration tests
    // deterministic without inheriting arbitrary host environment values.
    if executable
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("pi_rpc_fixture"))
    {
        if let Some(mode) = std::env::var_os("HALO_PI_RPC_FIXTURE_MODE") {
            command.env("HALO_PI_RPC_FIXTURE_MODE", mode);
        }
    }

    command
}

fn build_pi_command(executable: &Path, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        match executable
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("ps1") => {
                let mut command = Command::new("powershell.exe");
                command.args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ]);
                command.arg(executable).args(args);
                return command;
            }
            Some("cmd") | Some("bat") => {
                let mut command = Command::new("cmd.exe");
                command.arg("/D").arg("/S").arg("/C");
                let mut command_line = quote_windows_arg(&executable.to_string_lossy());
                for argument in args {
                    command_line.push(' ');
                    command_line.push_str(&quote_windows_arg(argument));
                }
                command.arg(command_line);
                return command;
            }
            _ => {}
        }
    }

    let mut command = Command::new(executable);
    command.args(args);
    command
}

#[cfg(windows)]
fn quote_windows_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}
