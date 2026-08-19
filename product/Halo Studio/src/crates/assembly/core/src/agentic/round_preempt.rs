//! Compatibility re-exports for round-boundary injection state.

pub use halo_agent_runtime::scheduler::{
    DialogRoundInjectionInterrupt, NoopDialogRoundInjectionSource, SessionRoundInjectionBuffer,
};
pub use halo_runtime_ports::{
    DialogRoundInjectionSource, RoundInjection, RoundInjectionKind, RoundInjectionTarget,
};
