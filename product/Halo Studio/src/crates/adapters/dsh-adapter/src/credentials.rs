//! Credential handling for the DSH adapter (ADR-0078, ADR-0008/0025).
//!
//! DSH carries only a CredentialRef — the name of an environment variable —
//! never a literal key (research section 4.1: "Configuration carries only this
//! name — a literal key is not a configuration value"). The credential value
//! exists only in memory and in the controlled child's environment at spawn
//! time:
//!
//! - values are injected into the child environment and nowhere else;
//! - `DSH_HOME` points at a Halo-managed directory so profiles, sessions and
//!   credentials state stay isolated from the developer's own `~/.dsh`;
//! - `.env` is never an injection channel: the adapter neither reads nor
//!   writes any `.env` file, and upstream forbids bootstrap names (`DSH_*` /
//!   `XDG_*`) in `.env` anyway (research section 4.1, `loadLayeredEnv`);
//! - the child environment is a full replacement (upstream clients own the
//!   credential policy the same way), so nothing leaks through inheritance.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use halo_runtime_ports::{PortError, PortErrorKind, PortResult};

use crate::DshFailureKind;

/// The environment variable upstream resolves the DeepSeek API key from
/// (`apiKeyEnv` default; research section 4.1).
pub const DSH_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";

/// The child environment key pointing at the Halo-managed DSH home.
pub const DSH_HOME_ENV: &str = "DSH_HOME";

/// Prefix for adapter-owned temporary DSH home directories.
pub(crate) const DSH_MANAGED_HOME_PREFIX: &str = "halo-dsh-home-";

/// Maximum length of a credential environment-variable name.
const MAX_CREDENTIAL_REF_BYTES: usize = 128;

/// A validated CredentialRef: the *name* of the environment variable that
/// carries the credential value. Bootstrap names (`DSH_*` / `XDG_*`) decide
/// how the process boots and are rejected as credential targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DshCredentialRef(String);

impl DshCredentialRef {
    pub fn new(name: impl Into<String>) -> Result<Self, DshFailureKind> {
        let name = name.into();
        let valid = !name.is_empty()
            && name.len() <= MAX_CREDENTIAL_REF_BYTES
            && name
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
            && !name.to_ascii_uppercase().starts_with("DSH_")
            && !name.to_ascii_uppercase().starts_with("XDG_");
        if valid {
            Ok(Self(name))
        } else {
            Err(DshFailureKind::Protocol)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// OS-backed or memory-backed source of credential values, read only at the
/// controlled child creation boundary. Values never cross the
/// `ManagedExecutorPort` seam and are never cached by the adapter.
#[async_trait]
pub trait DshCredentialStore: Send + Sync {
    async fn resolve(&self, credential_ref: &DshCredentialRef) -> PortResult<String>;
}

/// In-memory credential store for tests and explicit review deployments.
#[derive(Default)]
pub struct MemoryDshCredentialStore {
    entries: Mutex<HashMap<String, String>>,
}

impl MemoryDshCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, name: &str, value: &str) {
        self.entries
            .lock()
            .expect("credential store lock is available")
            .insert(name.to_string(), value.to_string());
    }
}

#[async_trait]
impl DshCredentialStore for MemoryDshCredentialStore {
    async fn resolve(&self, credential_ref: &DshCredentialRef) -> PortResult<String> {
        self.entries
            .lock()
            .expect("credential store lock is available")
            .get(credential_ref.as_str())
            .cloned()
            .ok_or_else(|| {
                PortError::new(
                    PortErrorKind::NotFound,
                    "credential reference is not present in the store",
                )
            })
    }
}

/// The parent environment keys the controlled DSH child may inherit. DSH runs
/// on Node, so process-locating and system keys are required; everything else
/// (shell history, package tokens, cloud configs) stays out of the child.
const SAFE_CHILD_ENVIRONMENT: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SystemRoot",
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "HOME",
    "APPDATA",
    "LOCALAPPDATA",
    "ALLUSERSPROFILE",
    "PROGRAMDATA",
    "COMSPEC",
];

/// Builds the controlled child's environment.
///
/// The allowlisted parent keys are inherited (with any `DSH_*` / `XDG_*`
/// bootstrap names stripped), `DSH_HOME` is forced to the Halo-managed
/// directory, and credential values are injected under their CredentialRef
/// names. No `.env` file is read or written on this path, and credential
/// values never reach argv, config files, or the port seam.
pub fn build_child_environment(
    dsh_home: &Path,
    credentials: &[(DshCredentialRef, String)],
) -> HashMap<String, PathBuf> {
    let mut environment: HashMap<String, PathBuf> = HashMap::new();
    // The allowlist contains no `DSH_*` / `XDG_*` bootstrap names, so nothing
    // under those prefixes is ever inherited (research section 4.1): the
    // child boots exactly from the Halo-managed home injected below.
    for key in SAFE_CHILD_ENVIRONMENT {
        if key.eq_ignore_ascii_case(DSH_HOME_ENV) {
            continue;
        }
        if let Some(value) = std::env::var_os(key) {
            environment.insert((*key).to_string(), PathBuf::from(value));
        }
    }
    environment.insert(DSH_HOME_ENV.to_string(), dsh_home.to_path_buf());
    for (reference, value) in credentials {
        environment.insert(reference.0.clone(), PathBuf::from(value));
    }
    environment
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        build_child_environment, DshCredentialRef, DSH_API_KEY_ENV, DSH_HOME_ENV,
        DSH_MANAGED_HOME_PREFIX,
    };
    use crate::DshFailureKind;

    #[test]
    fn credential_ref_validates_env_names() {
        assert!(DshCredentialRef::new(DSH_API_KEY_ENV).is_ok());
        assert!(DshCredentialRef::new("_PRIVATE_KEY_ENV").is_ok());
        assert_eq!(
            DshCredentialRef::new("DSH_HOME"),
            Err(DshFailureKind::Protocol)
        );
        assert_eq!(
            DshCredentialRef::new("XDG_CONFIG_HOME"),
            Err(DshFailureKind::Protocol)
        );
        assert_eq!(DshCredentialRef::new(""), Err(DshFailureKind::Protocol));
        assert_eq!(
            DshCredentialRef::new("1BAD_NAME"),
            Err(DshFailureKind::Protocol)
        );
        assert_eq!(
            DshCredentialRef::new("BAD NAME"),
            Err(DshFailureKind::Protocol)
        );
    }

    #[test]
    fn child_environment_forces_managed_home_and_injects_credentials() {
        let reference = DshCredentialRef::new(DSH_API_KEY_ENV).expect("valid reference");
        let environment =
            build_child_environment(Path::new("/managed/halo-dsh"), &[(reference, "value".into())]);
        assert_eq!(
            environment.get(DSH_HOME_ENV),
            Some(&Path::new("/managed/halo-dsh").to_path_buf())
        );
        assert_eq!(
            environment.get(DSH_API_KEY_ENV),
            Some(&PathBuf::from("value"))
        );
        assert!(environment.contains_key("PATH") || environment.contains_key("Path"));
    }

    #[test]
    fn managed_home_prefix_is_stable() {
        assert_eq!(DSH_MANAGED_HOME_PREFIX, "halo-dsh-home-");
    }
}
