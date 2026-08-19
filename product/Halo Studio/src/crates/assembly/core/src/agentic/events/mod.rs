//! Core-compatible event layer facade.
//!
//! Provider-neutral queue and routing owners live in `halo-agent-runtime`.

pub mod queue {
    pub use halo_agent_runtime::event_queue::*;
}

pub mod router {
    pub use halo_agent_runtime::event_router::*;
}

pub mod types;

pub use queue::*;
pub use router::*;
pub use types::*;
