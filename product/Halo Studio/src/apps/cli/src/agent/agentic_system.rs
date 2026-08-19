use anyhow::{Context, Result};

use halo_core::product_assembly::DeliveryProfile;
use halo_core::product_runtime::CoreRuntimeServicesProvider;
use halo_core::runtime_ownership::CoreRuntimeOwnership;
use std::sync::Arc;

pub(crate) use halo_core::agentic::system::AgenticSystem;

pub(crate) fn select_agentic_system_profile(profile: DeliveryProfile) -> Result<()> {
    halo_core::agentic::system::select_agentic_system_profile(profile)
        .context("Failed to select agentic system delivery profile")
}

pub(crate) async fn init_agentic_system(
    profile: DeliveryProfile,
    runtime_ownership: Arc<CoreRuntimeOwnership>,
) -> Result<AgenticSystem> {
    let system =
        halo_core::agentic::system::init_agentic_system_for_profile_with_runtime_ownership(
            profile,
            runtime_ownership,
        )
        .await
        .context("Failed to initialize agentic system")?;
    system
        .coordinator
        .set_terminal_port(CoreRuntimeServicesProvider::terminal_port());
    system
        .coordinator
        .set_remote_exec_port(CoreRuntimeServicesProvider::remote_exec_port());
    Ok(system)
}
