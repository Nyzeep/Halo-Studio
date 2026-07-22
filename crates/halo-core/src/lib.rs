pub mod completion;
pub mod event_bus;
pub mod runtime;
pub mod scheduler;

pub use event_bus::{EventBus, EventBusError};
pub use halo_protocol::{
    AgentCapability, AgentProfile, AgentProvider, CompletionCandidate, RunSnapshot, RunState,
    RuntimeCommand, RuntimeEvent, SlashCommand, WorkflowKind,
};
