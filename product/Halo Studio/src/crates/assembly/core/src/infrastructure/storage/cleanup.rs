//! Compatibility wrapper for generic storage cleanup.

use crate::infrastructure::PathManager;
use crate::util::errors::*;
use std::path::PathBuf;

pub use bitfun_services_core::storage_cleanup::{CleanupCategory, CleanupPolicy, CleanupResult};

pub struct CleanupService {
    inner: bitfun_services_core::storage_cleanup::CleanupService,
}

impl CleanupService {
    pub fn new(path_manager: PathManager, policy: CleanupPolicy) -> Self {
        Self::new_with_logs_dir(path_manager.clone(), policy, path_manager.logs_dir())
    }

    pub fn new_with_logs_dir(
        path_manager: PathManager,
        policy: CleanupPolicy,
        logs_dir: PathBuf,
    ) -> Self {
        let roots = bitfun_services_core::storage_cleanup::CleanupRoots {
            temp_dir: path_manager.temp_dir(),
            logs_dir,
            cache_dir: path_manager.cache_root(),
        };
        Self {
            inner: bitfun_services_core::storage_cleanup::CleanupService::new(roots, policy),
        }
    }

    pub async fn cleanup_all(&self) -> BitFunResult<CleanupResult> {
        self.inner.cleanup_all().await.map_err(BitFunError::service)
    }
}
