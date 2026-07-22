pub mod completion;
pub mod runtime;
pub mod scheduler;

pub use halo_protocol::{
    AgentCapability, AgentProfile, AgentProvider, CompletionCandidate, RuntimeEvent, SlashCommand,
    WorkflowKind,
};
