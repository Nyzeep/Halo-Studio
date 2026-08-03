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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bitfun_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiProviderCapability, PiProviderCapabilityPort,
    PiProviderCapabilityRequest, PiRpcCommand, PiRpcEvent, PiRpcFailureKind,
    PiRpcOperationDecision, PiRpcOperationKind, PiRpcPort, PiRpcReply, PiRpcSessionMode,
    PiRpcWorkspace, PiRuntimeConfiguration, PiRuntimeConfigurationPort, PortError, PortErrorKind,
    PortResult, PI_RPC_ADAPTER_IDENTITY,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{broadcast, oneshot, Mutex, Notify};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use crate::framing::{decode_jsonl_record, encode_jsonl};

const EVENT_CAPACITY: usize = 128;
const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_ABORT_GRACE_PERIOD: Duration = Duration::from_secs(3);
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
    extension_path: Option<PathBuf>,
    owned_extension_dir: Option<PathBuf>,
    runtime_configuration: Option<PiRuntimeConfiguration>,
    runtime_capability: Option<PiProviderCapability>,
    prepared_session: Option<Arc<PiSession>>,
    readiness_failed: bool,
    sessions: HashMap<String, Arc<PiSession>>,
}

#[derive(Clone)]
struct ResolvedRuntimeConfiguration {
    configuration: PiRuntimeConfiguration,
    capability: Option<PiProviderCapability>,
}

impl Drop for AdapterState {
    fn drop(&mut self) {
        if let Some(directory) = self.owned_extension_dir.take() {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

struct InstalledExtension {
    path: PathBuf,
    owned_dir: Option<PathBuf>,
}

impl InstalledExtension {
    fn cleanup(mut self) -> Result<(), PiRpcFailureKind> {
        let Some(directory) = self.owned_dir.as_ref() else {
            return Ok(());
        };

        match std::fs::remove_dir_all(directory) {
            Ok(()) => {
                self.owned_dir = None;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.owned_dir = None;
                Ok(())
            }
            Err(_) => Err(PiRpcFailureKind::Internal),
        }
    }
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
    is_prepared: AtomicBool,
    adapter_state: Weak<Mutex<AdapterState>>,
    _config_dir: tempfile::TempDir,
    _session_dir: Option<tempfile::TempDir>,
    events: broadcast::Sender<PiRpcEvent>,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<String, oneshot::Sender<PortResult<Value>>>>,
    operations: Mutex<HashMap<String, PiOperationBinding>>,
    seen_extension_requests: Mutex<HashSet<String>>,
    prompt_sent: AtomicBool,
    running: AtomicBool,
    terminated: AtomicBool,
    failure_reported: AtomicBool,
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

    async fn probe_pi(&self) -> Result<PathBuf, PiRpcFailureKind> {
        let executable = self.resolve_executable().await?;
        let output = self.probe_executable_version(&executable).await?;
        if !output.status.success() {
            return Err(PiRpcFailureKind::UnsupportedVersion);
        }
        // Version probing is only executable readiness. It does not read
        // auth.json/models.json, invoke a provider, or prove model readiness.
        if output.stdout.is_empty() && output.stderr.is_empty() {
            return Err(PiRpcFailureKind::Protocol);
        }
        Ok(executable)
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
                    .is_ok_and(|output| output.status.success())
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
        prepared: bool,
        mode: PiRpcSessionMode,
        workspace: &PiRpcWorkspace,
        executable: &Path,
        extension_path: &Path,
        runtime_configuration: Option<&PiRuntimeConfiguration>,
        runtime_capability: Option<&PiProviderCapability>,
        credential: Option<PiCredentialSecret>,
    ) -> Result<Arc<PiSession>, PiRpcFailureKind> {
        let config_dir = self.create_private_directory("config")?;
        if let Some(configuration) = runtime_configuration {
            write_pi_config_projection(config_dir.path(), configuration, runtime_capability)?;
        }
        let session_dir = match mode {
            PiRpcSessionMode::Standard => Some(self.create_private_directory("session")?),
            PiRpcSessionMode::Managed => None,
        };
        if mode == PiRpcSessionMode::Standard && session_dir.is_none() {
            return Err(PiRpcFailureKind::CapabilityMismatch);
        }
        let credential_value = credential.map(PiCredentialSecret::into_string);
        let mut child = configure_child_command(
            build_pi_command(
                executable,
                &pi_rpc_args(
                    extension_path,
                    mode,
                    session_dir.as_ref().map(|directory| directory.path()),
                    runtime_configuration
                        .map(|configuration| configuration.provider_id.as_str())
                        .or(self.config.provider.as_deref()),
                    runtime_configuration
                        .map(|configuration| configuration.model_id.as_str())
                        .or(self.config.model.as_deref()),
                    runtime_configuration
                        .map(|configuration| configuration.thinking_level.as_str()),
                ),
            ),
            executable,
            Some(config_dir.path()),
            credential_value.as_deref(),
        )
        .current_dir(&workspace.canonical_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Pi protocol output is stdout. Child diagnostics must not reach
        // a caller or evidence stream, so stderr is intentionally closed.
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| PiRpcFailureKind::NotInstalled)?;
        let stdin = child.stdin.take().ok_or(PiRpcFailureKind::Transport)?;
        let stdout = child.stdout.take().ok_or(PiRpcFailureKind::Transport)?;
        let session = Arc::new(PiSession {
            generation,
            task_id: Mutex::new(task_id),
            session_id: Mutex::new(session_id),
            is_prepared: AtomicBool::new(prepared),
            adapter_state: Arc::downgrade(&self.state),
            _config_dir: config_dir,
            _session_dir: session_dir,
            events: self.events.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            operations: Mutex::new(HashMap::new()),
            seen_extension_requests: Mutex::new(HashSet::new()),
            prompt_sent: AtomicBool::new(false),
            running: AtomicBool::new(false),
            terminated: AtomicBool::new(false),
            failure_reported: AtomicBool::new(false),
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
            Ok(cursor) => cursor,
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
            if validate_entries_data(&since).is_err() {
                session.fail_closed(PiRpcFailureKind::Protocol).await;
                return Err(PiRpcFailureKind::Protocol);
            }
        }
        Ok(())
    }

    async fn start(&self, generation: u64, workspace: PiRpcWorkspace) -> PiRpcReply {
        let _lifecycle = self.lifecycle.lock().await;
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
            return PiRpcReply::Unavailable {
                reason: PiRpcFailureKind::CapabilityMismatch,
            };
        }
        {
            let state = self.state.lock().await;
            if state.generation == Some(generation)
                && state.workspace.as_ref() == Some(&workspace)
                && !state.readiness_failed
            {
                return PiRpcReply::Accepted;
            }
            if state.generation == Some(generation)
                && state.workspace.as_ref() == Some(&workspace)
                && state.readiness_failed
            {
                return PiRpcReply::Unavailable {
                    reason: PiRpcFailureKind::Transport,
                };
            }
            if state.generation.is_some() {
                return PiRpcReply::Unavailable {
                    reason: PiRpcFailureKind::Transport,
                };
            }
        }

        let runtime_configuration = match self.resolve_runtime_configuration(&workspace).await {
            Ok(configuration) => configuration,
            Err(reason) => return PiRpcReply::Unavailable { reason },
        };

        let mut extension = match self.install_first_party_extension() {
            Ok(extension) => extension,
            Err(reason) => return PiRpcReply::Unavailable { reason },
        };
        let extension_path = extension.path.clone();
        let executable = match self.probe_pi().await {
            Ok(path) => path,
            Err(reason) => {
                let reason = extension.cleanup().err().unwrap_or(reason);
                return PiRpcReply::Unavailable { reason };
            }
        };
        let credential = match self
            .read_runtime_credential(
                runtime_configuration
                    .as_ref()
                    .map(|resolved| &resolved.configuration),
            )
            .await
        {
            Ok(credential) => credential,
            Err(reason) => {
                let reason = extension.cleanup().err().unwrap_or(reason);
                return PiRpcReply::Unavailable { reason };
            }
        };

        let prepared_session = match self
            .spawn_session_process(
                generation,
                "__halo_workbench_prepared__".to_string(),
                "__halo_workbench_prepared__".to_string(),
                true,
                PiRpcSessionMode::Managed,
                &workspace,
                &executable,
                &extension_path,
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
                let reason = extension.cleanup().err().unwrap_or(reason);
                return PiRpcReply::Unavailable { reason };
            }
        };
        if let Err(reason) = self.handshake(&prepared_session).await {
            prepared_session.terminate().await;
            let reason = extension.cleanup().err().unwrap_or(reason);
            return PiRpcReply::Unavailable { reason };
        }
        if let Some(configuration) = runtime_configuration.as_ref() {
            if let Err(reason) = self
                .validate_native_capability(&prepared_session, &configuration.configuration)
                .await
            {
                prepared_session.terminate().await;
                let reason = extension.cleanup().err().unwrap_or(reason);
                return PiRpcReply::Unavailable { reason };
            }
        }

        let mut state = self.state.lock().await;
        let extension_path = std::mem::take(&mut extension.path);
        let owned_extension_dir = extension.owned_dir.take();
        state.generation = Some(generation);
        state.workspace = Some(workspace);
        state.executable = Some(executable);
        state.extension_path = Some(extension_path);
        state.owned_extension_dir = owned_extension_dir;
        state.runtime_configuration = runtime_configuration
            .as_ref()
            .map(|resolved| resolved.configuration.clone());
        state.runtime_capability = runtime_configuration
            .as_ref()
            .and_then(|resolved| resolved.capability.clone());
        state.prepared_session = Some(prepared_session);
        state.readiness_failed = false;
        drop(state);

        self.emit(PiRpcEvent::Ready { generation });
        PiRpcReply::Accepted
    }

    async fn create_session(
        &self,
        generation: u64,
        task_id: String,
        session_id: String,
        mode: PiRpcSessionMode,
    ) -> Result<(), PiRpcFailureKind> {
        let (
            workspace,
            executable,
            extension_path,
            runtime_configuration,
            runtime_capability,
            prepared_session,
        ) = {
            let mut state = self.state.lock().await;
            if state.generation != Some(generation) || state.readiness_failed {
                return Err(PiRpcFailureKind::Transport);
            }
            if state.sessions.contains_key(&session_id) {
                return Err(PiRpcFailureKind::Internal);
            }
            let prepared_session = if mode == PiRpcSessionMode::Managed {
                state.prepared_session.take()
            } else {
                None
            };
            (
                state.workspace.clone().ok_or(PiRpcFailureKind::Transport)?,
                state
                    .executable
                    .clone()
                    .ok_or(PiRpcFailureKind::Transport)?,
                state
                    .extension_path
                    .clone()
                    .ok_or(PiRpcFailureKind::CapabilityMismatch)?,
                state.runtime_configuration.clone(),
                state.runtime_capability.clone(),
                prepared_session,
            )
        };

        let session = match prepared_session {
            Some(session) if !session.terminated.load(Ordering::Acquire) => session,
            None => {
                let credential = self
                    .read_runtime_credential(runtime_configuration.as_ref())
                    .await?;
                let session = self
                    .spawn_session_process(
                        generation,
                        task_id.clone(),
                        session_id.clone(),
                        false,
                        mode,
                        &workspace,
                        &executable,
                        &extension_path,
                        runtime_configuration.as_ref(),
                        runtime_capability.as_ref(),
                        credential,
                    )
                    .await?;
                if let Err(reason) = self.handshake(&session).await {
                    session.terminate().await;
                    return Err(reason);
                }
                session
            }
            Some(session) => {
                session.terminate().await;
                let credential = self
                    .read_runtime_credential(runtime_configuration.as_ref())
                    .await?;
                self.spawn_session_process(
                    generation,
                    task_id.clone(),
                    session_id.clone(),
                    false,
                    mode,
                    &workspace,
                    &executable,
                    &extension_path,
                    runtime_configuration.as_ref(),
                    runtime_capability.as_ref(),
                    credential,
                )
                .await?
            }
        };
        session.set_scope(task_id, session_id.clone()).await;

        let mut state = self.state.lock().await;
        if state.generation != Some(generation) {
            drop(state);
            session.terminate().await;
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

    async fn session(&self, generation: u64, session_id: &str) -> PortResult<Arc<PiSession>> {
        let state = self.state.lock().await;
        if state.generation != Some(generation) {
            return Err(PortError::new(
                PortErrorKind::NotAvailable,
                "Pi RPC generation is no longer active",
            ));
        }
        let session = state.sessions.get(session_id).cloned().ok_or_else(|| {
            PortError::new(PortErrorKind::NotFound, "Pi RPC session is not available")
        })?;
        if session.terminated.load(Ordering::Acquire) {
            return Err(PortError::new(
                PortErrorKind::NotAvailable,
                "Pi RPC session has failed closed",
            ));
        }
        Ok(session)
    }

    async fn shutdown_sessions(&self, generation: u64) -> Result<(), PiRpcFailureKind> {
        let _lifecycle = self.lifecycle.lock().await;
        let (sessions, owned_extension_dir) = {
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
            state.extension_path = None;
            state.runtime_configuration = None;
            state.readiness_failed = false;
            let owned_extension_dir = state.owned_extension_dir.clone();
            let mut sessions = state
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>();
            if let Some(session) = state.prepared_session.take() {
                sessions.push(session);
            }
            (sessions, owned_extension_dir)
        };

        for session in sessions {
            if session.running.load(Ordering::Acquire) {
                let _ = session.abort_with_grace().await;
            }
            session.terminate().await;
        }

        if let Some(directory) = owned_extension_dir {
            match std::fs::remove_dir_all(directory) {
                Ok(()) => {
                    self.state.lock().await.owned_extension_dir = None;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(PiRpcFailureKind::Internal),
            }
        }
        Ok(())
    }
}

#[async_trait]
impl PiRpcPort for PiRpcAdapter {
    async fn execute(&self, command: PiRpcCommand) -> PortResult<PiRpcReply> {
        match command {
            PiRpcCommand::Probe { .. } => match self.probe_pi().await {
                Ok(_) => Ok(PiRpcReply::Available),
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
                session_id,
                content,
            } => {
                let session = self.session(generation, &session_id).await?;
                let command = next_input_command(&session.prompt_sent);
                session
                    .request(command, json!({ "type": command, "message": content }))
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
                session_id,
            } => {
                let session = self.session(generation, &session_id).await?;
                session.abort_with_grace().await?;
                self.emit(PiRpcEvent::SessionStopped {
                    generation,
                    session_id,
                });
                Ok(PiRpcReply::Accepted)
            }
            PiRpcCommand::EndSession {
                generation,
                session_id,
            } => {
                let session = self.session(generation, &session_id).await?;
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
                let session = self.session(generation, &session_id).await?;
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
    async fn current_task_id(&self) -> String {
        self.task_id.lock().await.clone()
    }

    async fn current_session_id(&self) -> String {
        self.session_id.lock().await.clone()
    }

    async fn set_scope(&self, task_id: String, session_id: String) {
        self.is_prepared.store(false, Ordering::Release);
        *self.task_id.lock().await = task_id;
        *self.session_id.lock().await = session_id;
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
            if !self.terminated.load(Ordering::Acquire) {
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
                if !self.terminated.load(Ordering::Acquire) {
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

    async fn abort_with_grace(self: &Arc<Self>) -> PortResult<()> {
        if !self.running.load(Ordering::Acquire) {
            return Ok(());
        }
        let observed_settlement = self.settled_epoch.load(Ordering::Acquire);
        let deadline = Instant::now() + self.abort_grace_period;
        let request_timeout = self.abort_grace_period.min(self.response_timeout);
        if let Err(error) = self
            .request_with_timeout("abort", json!({ "type": "abort" }), request_timeout)
            .await
        {
            if !self.terminated.load(Ordering::Acquire) {
                self.fail_closed(PiRpcFailureKind::Transport).await;
            }
            return Err(error);
        }
        if !self.running.load(Ordering::Acquire) {
            return Ok(());
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
        if !settled && self.running.load(Ordering::Acquire) {
            // Explicit abort is allowed to force-reclaim a stuck child after
            // the bounded grace period. This path never reports success as a
            // completed Pi run; it only closes the owned process.
            self.terminate().await;
        }
        Ok(())
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
        if self.failure_reported.swap(true, Ordering::AcqRel) {
            return;
        }
        self.running.store(false, Ordering::Release);
        self.settled.notify_waiters();
        let message = match reason {
            PiRpcFailureKind::Transport => "Pi RPC transport failure",
            _ => "Pi RPC protocol error",
        };
        self.fail_pending(message).await;
        self.operations.lock().await.clear();
        let session_id = self.current_session_id().await;
        let is_prepared = self.is_prepared.load(Ordering::Acquire);
        if let Some(adapter_state) = self.adapter_state.upgrade() {
            let mut state = adapter_state.lock().await;
            if is_prepared {
                if state
                    .prepared_session
                    .as_ref()
                    .is_some_and(|prepared| Arc::ptr_eq(prepared, self))
                {
                    state.prepared_session = None;
                }
                // A failed readiness process must not leave Start idempotently
                // reporting a healthy Pi generation. The runtime receives the
                // redacted Failed event below and can fence the generation.
                state.readiness_failed = true;
            } else {
                state
                    .sessions
                    .retain(|_, session| !Arc::ptr_eq(session, self));
            }
        }
        if is_prepared {
            let _ = self.events.send(PiRpcEvent::Failed {
                generation: self.generation,
                reason,
            });
        } else {
            let _ = self.events.send(PiRpcEvent::SessionFailed {
                generation: self.generation,
                session_id,
                reason,
            });
        }
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
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
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
            let session_id = session.current_session_id().await;
            let _ = session.events.send(PiRpcEvent::MessageUpdated {
                generation: session.generation,
                session_id,
            });
        }
        Some("tool_execution_start") => {
            emit_tool_event(session, value, ToolEventKind::Started).await;
        }
        Some("tool_execution_update") => {
            emit_tool_event(session, value, ToolEventKind::Updated).await;
        }
        Some("tool_execution_end") => {
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
    let Some(tool_name) = value.get("toolName").and_then(Value::as_str) else {
        session.fail_closed(PiRpcFailureKind::Protocol).await;
        return;
    };
    let session_id = session.current_session_id().await;
    let task_id = session.current_task_id().await;
    let redacted_tool_call_id =
        redact_tool_call_id(session.generation, &task_id, &session_id, tool_call_id);
    let event = match kind {
        ToolEventKind::Started => PiRpcEvent::ToolExecutionStarted {
            generation: session.generation,
            session_id: session_id.clone(),
            redacted_tool_call_id,
            tool_name: tool_name.to_string(),
        },
        ToolEventKind::Updated => PiRpcEvent::ToolExecutionUpdated {
            generation: session.generation,
            session_id: session_id.clone(),
            redacted_tool_call_id,
            tool_name: tool_name.to_string(),
        },
        ToolEventKind::Ended => PiRpcEvent::ToolExecutionEnded {
            generation: session.generation,
            session_id,
            redacted_tool_call_id,
            tool_name: tool_name.to_string(),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        },
    };
    let _ = session.events.send(event);
}

fn stable_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
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

fn validate_entries_data(value: &Value) -> Result<Option<String>, ()> {
    let object = value.as_object().ok_or(())?;
    let entries = object.get("entries").and_then(Value::as_array).ok_or(())?;
    for entry in entries {
        if entry
            .get("id")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(());
        }
    }
    let leaf_id = object.get("leafId").ok_or(())?;
    if !leaf_id.is_null() && leaf_id.as_str().is_none_or(str::is_empty) {
        return Err(());
    }
    Ok(leaf_id.as_str().map(str::to_string))
}

fn pi_rpc_args(
    extension_path: &Path,
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
    args.extend([
        "--no-approve".to_string(),
        "--no-extensions".to_string(),
        "--extension".to_string(),
        extension_path.to_string_lossy().into_owned(),
    ]);
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
        Path::new(extension_path),
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
