//! Narrow ports consumed by the Halo Workbench Runtime owner.
//!
//! Pi transport details and remote identifiers stay behind [`PiRpcPort`].
//! Commands and events use Halo-local identifiers plus explicitly redacted
//! tool-call identifiers for permission correlation.

use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::PortResult;

/// The only production managed-execution identity in the Halo P0 product.
pub const PI_RPC_ADAPTER_IDENTITY: &str = "pi-rpc-p0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchWorkspaceFactsRequest {
    pub workspace_id: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchWorkspaceFacts {
    pub workspace_id: String,
    pub canonical_root: PathBuf,
    pub trusted: bool,
    pub git_repository: bool,
}

#[async_trait]
pub trait WorkbenchWorkspaceFactsPort: Send + Sync {
    async fn inspect(
        &self,
        request: WorkbenchWorkspaceFactsRequest,
    ) -> PortResult<WorkbenchWorkspaceFacts>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PiProviderReadiness {
    pub available: bool,
}

#[async_trait]
pub trait PiProviderReadinessPort: Send + Sync {
    async fn check(&self) -> PortResult<PiProviderReadiness>;
}

/// The thinking levels that Halo permits in persisted runtime configuration.
///
/// Pi supports additional experimental levels, but P0 deliberately keeps the
/// product boundary to the stable, bounded set below. The adapter may still
/// learn about richer Pi capabilities behind [`PiProviderCapabilityPort`]
/// without allowing those values into Halo configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
}

impl PiThinkingLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Startup flags that may be persisted by Halo. Session isolation is selected
/// by the runtime session mode and is intentionally not user-configurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiStartupOptions {
    pub no_extensions: bool,
    pub no_approve: bool,
}

impl Default for PiStartupOptions {
    fn default() -> Self {
        Self {
            no_extensions: true,
            no_approve: true,
        }
    }
}

/// Halo's complete non-secret Pi configuration authority.
///
/// The custom Debug implementation is intentional: this value can cross
/// internal async seams, but neither the credential reference nor the full
/// base URL should appear in logs or error formatting by accident.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PiRuntimeConfiguration {
    pub provider_id: String,
    pub base_url: Option<String>,
    pub model_id: String,
    pub thinking_level: PiThinkingLevel,
    pub startup_options: PiStartupOptions,
    pub credential_ref: String,
}

/// Renderer-safe projection of the Halo Pi configuration authority.
///
/// The full base URL is never part of this DTO. `base_url_hint` is only the
/// constant `"<configured>"`; even an origin can reveal a private host, port,
/// or tenant identifier. The credential reference is opaque and cannot be
/// used to retrieve a secret by the renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiRuntimeConfigurationView {
    pub provider_id: String,
    pub model_id: String,
    pub thinking_level: PiThinkingLevel,
    pub startup_options: PiStartupOptions,
    pub credential_ref: String,
    pub base_url_hint: Option<String>,
}

impl fmt::Debug for PiRuntimeConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PiRuntimeConfiguration")
            .field("provider_id", &self.provider_id)
            .field("base_url", &"<redacted>")
            .field("model_id", &self.model_id)
            .field("thinking_level", &self.thinking_level)
            .field("startup_options", &self.startup_options)
            .field("credential_ref", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PiCredentialSecret(String);

impl PiCredentialSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Consumes the wrapper at the final child-process boundary.
    pub fn into_string(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

impl fmt::Debug for PiCredentialSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted credential>")
    }
}

impl Drop for PiCredentialSecret {
    fn drop(&mut self) {
        // Best-effort scrubbing for the in-process copy. The operating system
        // and child process own their separate copies after process creation.
        unsafe {
            self.0.as_bytes_mut().fill(0);
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PiProviderCapabilityRequest {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: Option<String>,
}

impl fmt::Debug for PiProviderCapabilityRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PiProviderCapabilityRequest")
            .field("provider_id", &self.provider_id)
            .field("model_id", &self.model_id)
            .field("base_url", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiProviderCapability {
    pub provider_id: String,
    pub model_id: String,
    /// The audited Pi API implementation used for a non-native provider
    /// projection. This stays capability metadata and is never persisted in
    /// Halo configuration.
    pub api: String,
    pub accepts_base_url: bool,
    pub supported_thinking_levels: Vec<PiThinkingLevel>,
}

#[async_trait]
pub trait PiProviderCapabilityPort: Send + Sync {
    async fn inspect(
        &self,
        request: PiProviderCapabilityRequest,
    ) -> PortResult<PiProviderCapability>;
}

#[async_trait]
pub trait PiRuntimeConfigurationPort: Send + Sync {
    async fn load_configuration(&self) -> PortResult<Option<PiRuntimeConfiguration>>;
}

#[async_trait]
pub trait PiRuntimeConfigurationManagementPort:
    PiRuntimeConfigurationPort + PiProviderReadinessPort
{
    async fn create_configuration(&self, configuration: PiRuntimeConfiguration) -> PortResult<()>;
    async fn update_configuration(&self, configuration: PiRuntimeConfiguration) -> PortResult<()>;
    async fn delete_configuration(&self) -> PortResult<()>;
    async fn rollback_configuration(&self) -> PortResult<()>;
    async fn public_configuration(&self) -> PortResult<Option<PiRuntimeConfigurationView>>;
}

#[async_trait]
pub trait PiCredentialStorePort: Send + Sync {
    async fn write(&self, provider_id: &str, secret: PiCredentialSecret) -> PortResult<String>;

    async fn read(&self, provider_id: &str, credential_ref: &str)
        -> PortResult<PiCredentialSecret>;

    async fn delete(&self, provider_id: &str, credential_ref: &str) -> PortResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiRpcWorkspace {
    pub workspace_id: String,
    pub canonical_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiRpcSessionMode {
    Standard,
    Managed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiRpcOperationKind {
    Permission,
}

#[derive(Clone, PartialEq, Eq)]
pub enum PiRpcOperationDecision {
    AllowOnce,
    Deny,
}

impl fmt::Debug for PiRpcOperationDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllowOnce => formatter.write_str("AllowOnce"),
            Self::Deny => formatter.write_str("Deny"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiRpcFailureKind {
    NotInstalled,
    UnsupportedVersion,
    CapabilityMismatch,
    Authentication,
    Transport,
    Protocol,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiRpcVersion {
    #[serde(rename = "0.81.1")]
    V0_81_1,
    #[serde(rename = "0.83.0")]
    V0_83_0,
}

impl PiRpcVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V0_81_1 => "0.81.1",
            Self::V0_83_0 => "0.83.0",
        }
    }

    pub const fn compatibility_profile(self) -> PiRpcCompatibilityProfile {
        match self {
            Self::V0_81_1 => PiRpcCompatibilityProfile::PiRpc0811P0,
            Self::V0_83_0 => PiRpcCompatibilityProfile::PiRpc0830P0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiRpcCompatibilityProfile {
    #[serde(rename = "pi-rpc-0.81.1-p0")]
    PiRpc0811P0,
    #[serde(rename = "pi-rpc-0.83.0-p0")]
    PiRpc0830P0,
}

impl PiRpcCompatibilityProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PiRpc0811P0 => "pi-rpc-0.81.1-p0",
            Self::PiRpc0830P0 => "pi-rpc-0.83.0-p0",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiRpcVersionEvidenceSource {
    LocalVersionProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiRpcCapability {
    #[serde(rename = "prompt")]
    Prompt,
    #[serde(rename = "follow_up")]
    FollowUp,
    #[serde(rename = "abort")]
    Abort,
    #[serde(rename = "get_state")]
    GetState,
    #[serde(rename = "get_entries")]
    GetEntries,
    #[serde(rename = "get_entries.entries")]
    GetEntriesEntries,
    #[serde(rename = "get_entries.leaf_id")]
    GetEntriesLeafId,
    #[serde(rename = "get_entries.since")]
    GetEntriesSince,
    #[serde(rename = "message_update")]
    MessageUpdate,
    #[serde(rename = "tool_execution_start")]
    ToolExecutionStart,
    #[serde(rename = "tool_execution_update")]
    ToolExecutionUpdate,
    #[serde(rename = "tool_execution_end")]
    ToolExecutionEnd,
    #[serde(rename = "agent_settled")]
    AgentSettled,
    #[serde(rename = "extension_ui_request")]
    ExtensionUiRequest,
    #[serde(rename = "extension_ui_response")]
    ExtensionUiResponse,
}

impl PiRpcCapability {
    pub const fn required_p0() -> &'static [Self] {
        &[
            Self::Prompt,
            Self::FollowUp,
            Self::Abort,
            Self::GetState,
            Self::GetEntries,
            Self::GetEntriesEntries,
            Self::GetEntriesLeafId,
            Self::GetEntriesSince,
            Self::MessageUpdate,
            Self::ToolExecutionStart,
            Self::ToolExecutionUpdate,
            Self::ToolExecutionEnd,
            Self::AgentSettled,
            Self::ExtensionUiRequest,
            Self::ExtensionUiResponse,
        ]
    }

    /// Capabilities that the controlled readiness handshake can verify
    /// without sending a prompt, follow-up, or model request.
    pub const fn verified_by_readiness_handshake() -> &'static [Self] {
        &[
            Self::Abort,
            Self::GetState,
            Self::GetEntries,
            Self::GetEntriesEntries,
            Self::GetEntriesLeafId,
            Self::GetEntriesSince,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiRpcVersionSummary {
    pub version: PiRpcVersion,
    pub profile: PiRpcCompatibilityProfile,
    pub evidence_source: PiRpcVersionEvidenceSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiRpcCapabilitySummary {
    pub required: Vec<PiRpcCapability>,
    pub verified: Vec<PiRpcCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiRpcAvailabilitySummary {
    pub version: PiRpcVersionSummary,
    pub capabilities: PiRpcCapabilitySummary,
}

impl PiRpcAvailabilitySummary {
    pub fn new(version: PiRpcVersion, evidence_source: PiRpcVersionEvidenceSource) -> Self {
        Self {
            version: PiRpcVersionSummary {
                version,
                profile: version.compatibility_profile(),
                evidence_source,
            },
            capabilities: PiRpcCapabilitySummary {
                required: PiRpcCapability::required_p0().to_vec(),
                verified: Vec::new(),
            },
        }
    }

    pub fn with_readiness_handshake_verified(mut self) -> Self {
        self.capabilities.verified = PiRpcCapability::verified_by_readiness_handshake().to_vec();
        self
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum PiRpcCommand {
    Probe {
        generation: u64,
        workspace: PiRpcWorkspace,
    },
    Start {
        generation: u64,
        workspace: PiRpcWorkspace,
    },
    CreateSession {
        generation: u64,
        task_id: String,
        session_id: String,
        mode: PiRpcSessionMode,
    },
    SendUserInput {
        generation: u64,
        session_id: String,
        content: String,
    },
    StopSession {
        generation: u64,
        session_id: String,
    },
    EndSession {
        generation: u64,
        session_id: String,
    },
    ResolveOperation {
        generation: u64,
        task_id: String,
        session_id: String,
        operation_id: String,
        decision: PiRpcOperationDecision,
    },
    Shutdown {
        generation: u64,
    },
}

impl fmt::Debug for PiRpcCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Probe {
                generation,
                workspace,
            } => formatter
                .debug_struct("Probe")
                .field("generation", generation)
                .field("workspace", workspace)
                .finish(),
            Self::Start {
                generation,
                workspace,
            } => formatter
                .debug_struct("Start")
                .field("generation", generation)
                .field("workspace", workspace)
                .finish(),
            Self::CreateSession {
                generation,
                task_id,
                session_id,
                mode,
            } => formatter
                .debug_struct("CreateSession")
                .field("generation", generation)
                .field("task_id", task_id)
                .field("session_id", session_id)
                .field("mode", mode)
                .finish(),
            Self::SendUserInput {
                generation,
                session_id,
                ..
            } => formatter
                .debug_struct("SendUserInput")
                .field("generation", generation)
                .field("session_id", session_id)
                .field("content", &"<redacted>")
                .finish(),
            Self::StopSession {
                generation,
                session_id,
            } => formatter
                .debug_struct("StopSession")
                .field("generation", generation)
                .field("session_id", session_id)
                .finish(),
            Self::EndSession {
                generation,
                session_id,
            } => formatter
                .debug_struct("EndSession")
                .field("generation", generation)
                .field("session_id", session_id)
                .finish(),
            Self::ResolveOperation {
                generation,
                task_id,
                session_id,
                operation_id,
                decision,
            } => formatter
                .debug_struct("ResolveOperation")
                .field("generation", generation)
                .field("task_id", task_id)
                .field("session_id", session_id)
                .field("operation_id", operation_id)
                .field("decision", decision)
                .finish(),
            Self::Shutdown { generation } => formatter
                .debug_struct("Shutdown")
                .field("generation", generation)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PiRpcReply {
    Available { summary: PiRpcAvailabilitySummary },
    Ready { summary: PiRpcAvailabilitySummary },
    Accepted,
    Unavailable { reason: PiRpcFailureKind },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PiRpcEvent {
    Ready {
        generation: u64,
    },
    Failed {
        generation: u64,
        reason: PiRpcFailureKind,
    },
    SessionCreated {
        generation: u64,
        session_id: String,
    },
    SessionRunning {
        generation: u64,
        session_id: String,
    },
    SessionIdle {
        generation: u64,
        session_id: String,
    },
    SessionStopped {
        generation: u64,
        session_id: String,
    },
    SessionEnded {
        generation: u64,
        session_id: String,
    },
    SessionFailed {
        generation: u64,
        session_id: String,
        reason: PiRpcFailureKind,
    },
    OperationRequested {
        generation: u64,
        session_id: String,
        operation_id: String,
        kind: PiRpcOperationKind,
        redacted_tool_call_id: Option<String>,
    },
    OperationResolved {
        generation: u64,
        session_id: String,
        operation_id: String,
    },
    MessageUpdated {
        generation: u64,
        session_id: String,
    },
    ToolExecutionStarted {
        generation: u64,
        session_id: String,
        redacted_tool_call_id: String,
        tool_name: String,
    },
    ToolExecutionUpdated {
        generation: u64,
        session_id: String,
        redacted_tool_call_id: String,
        tool_name: String,
    },
    ToolExecutionEnded {
        generation: u64,
        session_id: String,
        redacted_tool_call_id: String,
        tool_name: String,
        is_error: bool,
    },
    AgentSettled {
        generation: u64,
        session_id: String,
    },
}

impl PiRpcEvent {
    pub const fn generation(&self) -> u64 {
        match self {
            Self::Ready { generation }
            | Self::Failed { generation, .. }
            | Self::SessionCreated { generation, .. }
            | Self::SessionRunning { generation, .. }
            | Self::SessionIdle { generation, .. }
            | Self::SessionStopped { generation, .. }
            | Self::SessionEnded { generation, .. }
            | Self::SessionFailed { generation, .. }
            | Self::OperationRequested { generation, .. }
            | Self::OperationResolved { generation, .. }
            | Self::MessageUpdated { generation, .. }
            | Self::ToolExecutionStarted { generation, .. }
            | Self::ToolExecutionUpdated { generation, .. }
            | Self::ToolExecutionEnded { generation, .. }
            | Self::AgentSettled { generation, .. } => *generation,
        }
    }
}

#[async_trait]
pub trait PiRpcPort: Send + Sync {
    async fn execute(&self, command: PiRpcCommand) -> PortResult<PiRpcReply>;

    fn subscribe(&self) -> broadcast::Receiver<PiRpcEvent>;
}
