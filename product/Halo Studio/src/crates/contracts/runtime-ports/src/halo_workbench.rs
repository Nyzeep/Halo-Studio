//! Narrow ports consumed by the Halo Workbench Runtime owner.
//!
//! Pi transport details and remote identifiers stay behind [`PiRpcPort`].
//! Commands and events use Halo-local identifiers plus explicitly redacted
//! tool-call identifiers for permission correlation.

use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiRpcReply {
    Available,
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
