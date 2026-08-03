use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bitfun_runtime_ports::{
    PiCredentialSecret, PiCredentialStorePort, PiProviderCapability, PiProviderCapabilityPort,
    PiProviderCapabilityRequest, PiProviderReadiness, PiProviderReadinessPort,
    PiRuntimeConfiguration, PiRuntimeConfigurationManagementPort, PiRuntimeConfigurationPort,
    PiRuntimeConfigurationView, PiStartupOptions, PortError, PortErrorKind, PortResult,
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

const CREDENTIAL_REF_PREFIX: &str = "halo-pi-credential-v1-";

#[async_trait]
pub trait PiRuntimeConfigurationRepository: Send + Sync {
    async fn load(&self) -> PortResult<Option<PiRuntimeConfiguration>>;
    async fn save(&self, configuration: &PiRuntimeConfiguration) -> PortResult<()>;
    async fn delete(&self) -> PortResult<()>;
    async fn load_rollback(&self) -> PortResult<Option<PiRuntimeConfiguration>>;
    async fn save_rollback(&self, configuration: Option<&PiRuntimeConfiguration>)
        -> PortResult<()>;
}

#[derive(Default)]
pub struct MemoryPiRuntimeConfigurationRepository {
    value: Mutex<Option<PiRuntimeConfiguration>>,
    rollback: Mutex<Option<PiRuntimeConfiguration>>,
    fail_reads: Mutex<bool>,
    fail_writes: Mutex<bool>,
}

impl MemoryPiRuntimeConfigurationRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_read_failure(&self, failed: bool) {
        if let Ok(mut value) = self.fail_reads.lock() {
            *value = failed;
        }
    }

    pub fn set_write_failure(&self, failed: bool) {
        if let Ok(mut value) = self.fail_writes.lock() {
            *value = failed;
        }
    }
}

#[async_trait]
impl PiRuntimeConfigurationRepository for MemoryPiRuntimeConfigurationRepository {
    async fn load(&self) -> PortResult<Option<PiRuntimeConfiguration>> {
        if *self
            .fail_reads
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))?
        {
            return Err(backend_error("configuration repository is unavailable"));
        }
        Ok(self
            .value
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))?
            .clone())
    }

    async fn save(&self, configuration: &PiRuntimeConfiguration) -> PortResult<()> {
        if *self
            .fail_writes
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))?
        {
            return Err(backend_error("configuration repository is unavailable"));
        }
        *self
            .value
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))? =
            Some(configuration.clone());
        Ok(())
    }

    async fn delete(&self) -> PortResult<()> {
        if *self
            .fail_writes
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))?
        {
            return Err(backend_error("configuration repository is unavailable"));
        }
        *self
            .value
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))? = None;
        Ok(())
    }

    async fn load_rollback(&self) -> PortResult<Option<PiRuntimeConfiguration>> {
        Ok(self
            .rollback
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))?
            .clone())
    }

    async fn save_rollback(
        &self,
        configuration: Option<&PiRuntimeConfiguration>,
    ) -> PortResult<()> {
        *self
            .rollback
            .lock()
            .map_err(|_| backend_error("configuration repository lock failed"))? =
            configuration.cloned();
        Ok(())
    }
}

pub struct JsonFilePiRuntimeConfigurationRepository {
    path: PathBuf,
}

impl JsonFilePiRuntimeConfigurationRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn rollback_path(&self) -> PathBuf {
        self.path.with_extension("rollback.json")
    }
}

#[async_trait]
impl PiRuntimeConfigurationRepository for JsonFilePiRuntimeConfigurationRepository {
    async fn load(&self) -> PortResult<Option<PiRuntimeConfiguration>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(backend_error("configuration repository could not be read")),
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| invalid_error("stored Pi configuration is invalid"))
    }

    async fn save(&self, configuration: &PiRuntimeConfiguration) -> PortResult<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| backend_error("configuration path has no parent"))?;
        fs::create_dir_all(parent)
            .map_err(|_| backend_error("configuration repository could not be prepared"))?;
        let bytes = serde_json::to_vec_pretty(configuration)
            .map_err(|_| backend_error("configuration could not be encoded"))?;
        let temporary = parent.join(format!(".halo-pi-config-{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, bytes)
            .map_err(|_| backend_error("configuration repository could not be written"))?;
        replace_configuration_file(&temporary, &self.path)
    }

    async fn delete(&self) -> PortResult<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(backend_error(
                "configuration repository could not be deleted",
            )),
        }
    }

    async fn load_rollback(&self) -> PortResult<Option<PiRuntimeConfiguration>> {
        let path = self.rollback_path();
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| invalid_error("stored Pi rollback configuration is invalid")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(backend_error(
                "configuration rollback repository could not be read",
            )),
        }
    }

    async fn save_rollback(
        &self,
        configuration: Option<&PiRuntimeConfiguration>,
    ) -> PortResult<()> {
        let path = self.rollback_path();
        if configuration.is_none() {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(backend_error(
                    "configuration rollback repository could not be deleted",
                )),
            }
        } else {
            let configuration = configuration.expect("rollback configuration exists");
            let parent = path
                .parent()
                .ok_or_else(|| backend_error("configuration rollback path has no parent"))?;
            fs::create_dir_all(parent).map_err(|_| {
                backend_error("configuration rollback repository could not be prepared")
            })?;
            let bytes = serde_json::to_vec_pretty(configuration)
                .map_err(|_| backend_error("rollback configuration could not be encoded"))?;
            let temporary = parent.join(format!(".halo-pi-rollback-{}.tmp", Uuid::new_v4()));
            fs::write(&temporary, bytes).map_err(|_| {
                backend_error("configuration rollback repository could not be written")
            })?;
            replace_configuration_file(&temporary, &path)
        }
    }
}

#[cfg(not(windows))]
fn replace_configuration_file(temporary: &Path, destination: &Path) -> PortResult<()> {
    fs::rename(temporary, destination)
        .map_err(|_| backend_error("configuration repository could not be committed"))
}

#[cfg(windows)]
fn replace_configuration_file(temporary: &Path, destination: &Path) -> PortResult<()> {
    let backup = destination.with_extension("bak");
    let had_existing = match fs::rename(destination, &backup) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => {
            let _ = fs::remove_file(temporary);
            return Err(backend_error(
                "configuration repository could not be committed",
            ));
        }
    };
    if let Err(_) = fs::rename(temporary, destination) {
        if had_existing {
            let _ = fs::rename(&backup, destination);
        }
        let _ = fs::remove_file(temporary);
        return Err(backend_error(
            "configuration repository could not be committed",
        ));
    }
    if had_existing {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub struct PiRuntimeConfigurationService {
    repository: Arc<dyn PiRuntimeConfigurationRepository>,
    capabilities: Option<Arc<dyn PiProviderCapabilityPort>>,
    credential_store: Option<Arc<dyn PiCredentialStorePort>>,
    current: AsyncMutex<Option<PiRuntimeConfiguration>>,
    previous: AsyncMutex<Option<PiRuntimeConfiguration>>,
}

impl PiRuntimeConfigurationService {
    pub fn new(
        repository: Arc<dyn PiRuntimeConfigurationRepository>,
        capabilities: Arc<dyn PiProviderCapabilityPort>,
    ) -> Self {
        Self {
            repository,
            capabilities: Some(capabilities),
            credential_store: None,
            current: AsyncMutex::new(None),
            previous: AsyncMutex::new(None),
        }
    }

    /// Creates a configuration authority whose final provider/model check is
    /// deferred to the Pi-native readiness handshake performed by the adapter.
    pub fn new_without_capabilities(repository: Arc<dyn PiRuntimeConfigurationRepository>) -> Self {
        Self {
            repository,
            capabilities: None,
            credential_store: None,
            current: AsyncMutex::new(None),
            previous: AsyncMutex::new(None),
        }
    }

    /// Attaches the provider-bound credential store at the composition seam.
    /// Readiness checks only whether the opaque reference resolves; the secret
    /// is borrowed and immediately dropped without entering the configuration
    /// projection or any error value.
    pub fn with_credential_store(
        mut self,
        credential_store: Arc<dyn PiCredentialStorePort>,
    ) -> Self {
        self.credential_store = Some(credential_store);
        self
    }

    pub async fn create(&self, configuration: PiRuntimeConfiguration) -> PortResult<()> {
        if self.repository.load().await?.is_some() {
            return Err(invalid_error("Pi configuration already exists"));
        }
        self.validate(&configuration).await?;
        self.repository.save(&configuration).await?;
        self.repository.save_rollback(None).await?;
        *self.current.lock().await = Some(configuration);
        *self.previous.lock().await = None;
        Ok(())
    }

    pub async fn update(&self, configuration: PiRuntimeConfiguration) -> PortResult<()> {
        let previous = self.repository.load().await?.ok_or_else(|| {
            PortError::new(PortErrorKind::NotFound, "Pi configuration is missing")
        })?;
        self.validate(&configuration).await?;
        self.repository.save(&configuration).await?;
        if let Err(error) = self.repository.save_rollback(Some(&previous)).await {
            let _ = self.repository.save(&previous).await;
            return Err(error);
        }
        *self.previous.lock().await = Some(previous);
        *self.current.lock().await = Some(configuration);
        Ok(())
    }

    pub async fn delete(&self) -> PortResult<()> {
        let previous = self.repository.load().await?;
        self.repository.save_rollback(previous.as_ref()).await?;
        self.repository.delete().await?;
        *self.previous.lock().await = previous;
        *self.current.lock().await = None;
        Ok(())
    }

    pub async fn rollback(&self) -> PortResult<()> {
        let rollback = match self.previous.lock().await.clone() {
            Some(rollback) => rollback,
            None => self.repository.load_rollback().await?.ok_or_else(|| {
                PortError::new(
                    PortErrorKind::NotFound,
                    "Pi configuration rollback is unavailable",
                )
            })?,
        };
        let current = self.repository.load().await?;
        self.repository.save(&rollback).await?;
        if let Err(error) = self.repository.save_rollback(current.as_ref()).await {
            if let Some(current) = current.as_ref() {
                let _ = self.repository.save(current).await;
            } else {
                let _ = self.repository.delete().await;
            }
            return Err(error);
        }
        *self.previous.lock().await = current;
        *self.current.lock().await = Some(rollback);
        Ok(())
    }

    pub async fn current(&self) -> PortResult<Option<PiRuntimeConfiguration>> {
        let value = self.repository.load().await?;
        *self.current.lock().await = value.clone();
        Ok(value)
    }

    pub async fn public_view(&self) -> PortResult<Option<PiRuntimeConfigurationView>> {
        Ok(self.current().await?.map(public_view))
    }

    async fn readiness(&self) -> PortResult<PiProviderReadiness> {
        let configuration = self.current().await?.ok_or_else(|| {
            PortError::new(PortErrorKind::NotFound, "Pi configuration is missing")
        })?;
        self.validate(&configuration).await?;
        if let Some(credential_store) = self.credential_store.as_ref() {
            credential_store
                .read(&configuration.provider_id, &configuration.credential_ref)
                .await?;
        }
        Ok(PiProviderReadiness { available: true })
    }

    async fn validate(&self, configuration: &PiRuntimeConfiguration) -> PortResult<()> {
        validate_runtime_configuration_shape(configuration)?;
        if let Some(capabilities) = self.capabilities.as_ref() {
            let capability = capabilities
                .inspect(PiProviderCapabilityRequest {
                    provider_id: configuration.provider_id.clone(),
                    model_id: configuration.model_id.clone(),
                    base_url: configuration.base_url.clone(),
                })
                .await?;
            if capability.provider_id != configuration.provider_id
                || capability.model_id != configuration.model_id
                || !valid_pi_api(&capability.api)
                || (configuration.base_url.is_some() && !capability.accepts_base_url)
                || !capability
                    .supported_thinking_levels
                    .contains(&configuration.thinking_level)
            {
                return Err(invalid_error("Pi provider/model capability does not match"));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl PiRuntimeConfigurationPort for PiRuntimeConfigurationService {
    async fn load_configuration(&self) -> PortResult<Option<PiRuntimeConfiguration>> {
        self.current().await
    }
}

#[async_trait]
impl PiProviderReadinessPort for PiRuntimeConfigurationService {
    async fn check(&self) -> PortResult<PiProviderReadiness> {
        self.readiness().await
    }
}

#[async_trait]
impl PiRuntimeConfigurationManagementPort for PiRuntimeConfigurationService {
    async fn create_configuration(&self, configuration: PiRuntimeConfiguration) -> PortResult<()> {
        self.create(configuration).await
    }

    async fn update_configuration(&self, configuration: PiRuntimeConfiguration) -> PortResult<()> {
        self.update(configuration).await
    }

    async fn delete_configuration(&self) -> PortResult<()> {
        self.delete().await
    }

    async fn rollback_configuration(&self) -> PortResult<()> {
        self.rollback().await
    }

    async fn public_configuration(&self) -> PortResult<Option<PiRuntimeConfigurationView>> {
        self.public_view().await
    }
}

pub struct StaticPiProviderCapabilities {
    values: HashMap<(String, String), PiProviderCapability>,
}

impl StaticPiProviderCapabilities {
    pub fn new(values: Vec<PiProviderCapability>) -> Self {
        Self {
            values: values
                .into_iter()
                .map(|value| ((value.provider_id.clone(), value.model_id.clone()), value))
                .collect(),
        }
    }
}

#[async_trait]
impl PiProviderCapabilityPort for StaticPiProviderCapabilities {
    async fn inspect(
        &self,
        request: PiProviderCapabilityRequest,
    ) -> PortResult<PiProviderCapability> {
        let capability = self
            .values
            .get(&(request.provider_id, request.model_id))
            .cloned()
            .ok_or_else(|| invalid_error("Pi provider/model is unavailable"))?;
        if request.base_url.is_some() && !capability.accepts_base_url {
            return Err(invalid_error("Pi provider does not accept a base URL"));
        }
        Ok(capability)
    }
}

struct MemoryCredentialRecord {
    provider_id: String,
    secret: String,
}

#[derive(Default)]
pub struct MemoryPiCredentialStore {
    values: Mutex<HashMap<String, MemoryCredentialRecord>>,
    fail_reads: Mutex<bool>,
    fail_writes: Mutex<bool>,
    fail_deletes: Mutex<bool>,
}

impl MemoryPiCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_read_failure(&self, failed: bool) {
        if let Ok(mut value) = self.fail_reads.lock() {
            *value = failed;
        }
    }

    pub fn set_write_failure(&self, failed: bool) {
        if let Ok(mut value) = self.fail_writes.lock() {
            *value = failed;
        }
    }

    pub fn set_delete_failure(&self, failed: bool) {
        if let Ok(mut value) = self.fail_deletes.lock() {
            *value = failed;
        }
    }
}

#[async_trait]
impl PiCredentialStorePort for MemoryPiCredentialStore {
    async fn write(&self, provider_id: &str, secret: PiCredentialSecret) -> PortResult<String> {
        if *self
            .fail_writes
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?
        {
            return Err(backend_error("credential store is unavailable"));
        }
        let secret = secret.into_string();
        if secret.is_empty() {
            return Err(invalid_error("Pi credential must not be empty"));
        }
        let reference = format!("{CREDENTIAL_REF_PREFIX}{}", Uuid::new_v4());
        self.values
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?
            .insert(
                reference.clone(),
                MemoryCredentialRecord {
                    provider_id: provider_id.to_string(),
                    secret,
                },
            );
        Ok(reference)
    }

    async fn read(
        &self,
        provider_id: &str,
        credential_ref: &str,
    ) -> PortResult<PiCredentialSecret> {
        if *self
            .fail_reads
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?
        {
            return Err(backend_error("credential store is unavailable"));
        }
        let values = self
            .values
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?;
        let record = values.get(credential_ref).ok_or_else(|| {
            PortError::new(
                PortErrorKind::NotFound,
                "Pi credential reference is missing",
            )
        })?;
        if record.provider_id != provider_id {
            return Err(PortError::new(
                PortErrorKind::PermissionDenied,
                "Pi credential provider does not match configuration",
            ));
        }
        Ok(PiCredentialSecret::new(record.secret.clone()))
    }

    async fn delete(&self, provider_id: &str, credential_ref: &str) -> PortResult<()> {
        if *self
            .fail_deletes
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?
        {
            return Err(backend_error("credential store is unavailable"));
        }
        let owner = self
            .values
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?
            .get(credential_ref)
            .map(|record| record.provider_id.clone());
        if owner.is_some_and(|owner| owner != provider_id) {
            return Err(PortError::new(
                PortErrorKind::PermissionDenied,
                "Pi credential provider does not match configuration",
            ));
        }
        self.values
            .lock()
            .map_err(|_| backend_error("credential store lock failed"))?
            .remove(credential_ref);
        Ok(())
    }
}

pub fn validate_runtime_configuration_shape(
    configuration: &PiRuntimeConfiguration,
) -> PortResult<()> {
    if !valid_selection(&configuration.provider_id)
        || !valid_selection(&configuration.model_id)
        || !valid_credential_ref(&configuration.credential_ref)
    {
        return Err(invalid_error(
            "Pi configuration contains an invalid selection",
        ));
    }
    if configuration
        .base_url
        .as_deref()
        .is_some_and(|value| !valid_base_url(value))
    {
        return Err(invalid_error(
            "Pi base URL is invalid or contains credentials",
        ));
    }
    if configuration.startup_options != PiStartupOptions::default() {
        return Err(invalid_error(
            "Pi startup options are not in the P0 allowlist",
        ));
    }
    Ok(())
}

fn valid_selection(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value
            .chars()
            .all(|character| !character.is_control() && character != '\\')
}

fn valid_pi_api(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| !character.is_control() && !character.is_whitespace())
}

fn valid_credential_ref(value: &str) -> bool {
    value.starts_with(CREDENTIAL_REF_PREFIX)
        && value.len() <= 128
        && value[CREDENTIAL_REF_PREFIX.len()..]
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn valid_base_url(value: &str) -> bool {
    if value.len() > 2048
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
        || value.contains(['?', '#', '@'])
    {
        return false;
    }
    let lowercase = value.to_ascii_lowercase();
    if lowercase.contains("%40")
        || lowercase.contains("%3a")
        || lowercase.contains("%2f")
        || lowercase.contains("%5c")
    {
        return false;
    }
    let Some((scheme, authority)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") || authority.is_empty() {
        return false;
    }
    let host_and_port = authority.split('/').next().unwrap_or_default();
    if host_and_port.is_empty() || host_and_port.ends_with(':') {
        return false;
    }
    if let Some(rest) = host_and_port.strip_prefix('[') {
        let Some((host, port)) = rest.split_once(']') else {
            return false;
        };
        if host.is_empty() || host.contains(['[', ']']) {
            return false;
        }
        return match port.strip_prefix(':') {
            None => true,
            Some(port) => port.parse::<u16>().is_ok_and(|port| port != 0),
        };
    }
    let mut parts = host_and_port.split(':');
    let host = parts.next().unwrap_or_default();
    let port = parts.next();
    if host.is_empty() || host.contains(['[', ']']) || parts.next().is_some() {
        return false;
    }
    match port {
        None => true,
        Some(port) => port.parse::<u16>().is_ok_and(|port| port != 0),
    }
}

fn public_view(configuration: PiRuntimeConfiguration) -> PiRuntimeConfigurationView {
    PiRuntimeConfigurationView {
        provider_id: configuration.provider_id,
        model_id: configuration.model_id,
        thinking_level: configuration.thinking_level,
        startup_options: configuration.startup_options,
        credential_ref: configuration.credential_ref,
        base_url_hint: configuration.base_url.as_deref().map(base_url_hint),
    }
}

fn base_url_hint(_value: &str) -> String {
    // Even an origin can reveal an internal host, private IP, port, or tenant
    // identifier. The Renderer only needs to know that a base URL was set.
    "<configured>".to_string()
}

fn invalid_error(message: &'static str) -> PortError {
    PortError::new(PortErrorKind::InvalidRequest, message)
}

fn backend_error(message: &'static str) -> PortError {
    PortError::new(PortErrorKind::Backend, message)
}
