//! Persistence layer
//!
//! Responsible for persistent storage and loading of data

pub mod manager;
pub mod session_branch;

pub use halo_runtime_ports::SessionTurnLoadTiming;
pub use halo_services_core::session::{
    SessionBranchRequest, SessionBranchResult, SessionMetadataPage,
};
pub use manager::{MaterializedSessionReferenceTranscript, PersistenceManager};
