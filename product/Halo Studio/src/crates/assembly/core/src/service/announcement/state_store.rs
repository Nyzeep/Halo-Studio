use super::types::AnnouncementState;
use crate::infrastructure::app_paths::PathManager;
use crate::util::errors::{HaloError, HaloResult};
use std::sync::Arc;

pub struct AnnouncementStateStore {
    inner: halo_services_integrations::announcement::AnnouncementStateStore,
}

impl AnnouncementStateStore {
    pub fn new(path_manager: &Arc<PathManager>) -> Self {
        Self {
            inner: halo_services_integrations::announcement::AnnouncementStateStore::new(
                path_manager.user_config_dir(),
            ),
        }
    }

    /// Load state from disk.  Returns a default state if the file does not exist.
    pub async fn load(&self) -> HaloResult<AnnouncementState> {
        self.inner.load().await.map_err(map_state_store_error)
    }

    /// Persist state to disk.
    pub async fn save(&self, state: &AnnouncementState) -> HaloResult<()> {
        self.inner.save(state).await.map_err(map_state_store_error)
    }
}

fn map_state_store_error(
    err: halo_services_integrations::announcement::AnnouncementStateStoreError,
) -> HaloError {
    match err {
        halo_services_integrations::announcement::AnnouncementStateStoreError::Io(err) => {
            HaloError::Io(err)
        }
        halo_services_integrations::announcement::AnnouncementStateStoreError::Serialization(
            err,
        ) => HaloError::Serialization(err),
    }
}
