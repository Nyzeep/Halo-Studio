//! Compatibility facade for runtime-owned session facts.

pub use halo_agent_runtime::session::{
    sanitize_persisted_session_state, CompressionState, PersistedSessionStateFile, Session,
    SessionConfig, SessionContinuationPolicy, SessionKind, SessionModelBindingPolicy,
    SessionSummary,
};
