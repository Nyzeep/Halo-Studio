//! Portable owner for the Halo Workbench Runtime public seam.
//!
//! The owner exposes Halo-local state and intent types. Pi RPC protocol and
//! process details remain behind the Pi RPC port. Implementation lives in
//! private sibling modules behind the identical public Interface:
//! `vocabulary` (public types), `state` (state machine + adapter bindings),
//! `dispatch` (intent handling) and `redaction` (sanitization/validation).

mod dispatch;
mod redaction;
mod state;
mod vocabulary;

pub use state::HaloWorkbenchRuntime;
pub use vocabulary::*;
