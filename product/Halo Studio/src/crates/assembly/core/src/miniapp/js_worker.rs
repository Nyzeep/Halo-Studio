//! Compatibility facade for MiniApp JS worker processes.

pub use halo_services_integrations::miniapp::worker::{
    JsWorker, MiniAppWorkerEvent, MiniAppWorkerEventFuture, MiniAppWorkerEventSink,
    SharedMiniAppWorkerEventSink,
};
