use crate::secret::Secret;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// 凭据存取抽象。操作系统存储不可用时一切操作必须失败关闭，
/// 绝不回退到明文文件或内存缓存。
pub trait CredentialStore: Send + Sync {
    fn set(&self, ref_name: &str, secret: &Secret) -> Result<(), CredentialError>;
    /// 引用不存在时返回 `CredentialError::NotFound`。
    fn get(&self, ref_name: &str) -> Result<Secret, CredentialError>;
    fn exists(&self, ref_name: &str) -> Result<bool, CredentialError>;
    /// OS 存储是否可达；`false` 时 set/get/exists 一律失败关闭。
    fn available(&self) -> bool;
}

/// 凭据操作错误。message 永不携带凭据明文。
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("操作系统凭据存储不可用，操作已失败关闭")]
    StoreUnavailable,
    #[error("凭据引用不存在")]
    NotFound,
    #[error("凭据存储后端错误：{0}")]
    Backend(String),
}

/// Windows 凭据管理器条目的固定 service 名。
const SERVICE: &str = "HaloStudio";

/// 可用性探测使用的无敏感值 account 前缀。
const AVAILABILITY_PROBE_REF_PREFIX: &str = "halo-store-availability-probe";
const AVAILABILITY_PROBE_VALUE: &str = "halo-store-availability-probe";
static AVAILABILITY_PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);
static AVAILABILITY_PROBE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Windows 凭据管理器实现（keyring windows-native）。
/// service 固定 `HaloStudio`，account = 凭据引用名。
pub struct WindowsCredentialStore {
    available: OnceLock<bool>,
}

impl WindowsCredentialStore {
    pub fn new() -> Self {
        WindowsCredentialStore {
            available: OnceLock::new(),
        }
    }

    fn entry(ref_name: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(SERVICE, ref_name).map_err(map_backend_error)
    }

    fn ensure_available(&self) -> Result<(), CredentialError> {
        if self.available() {
            Ok(())
        } else {
            Err(CredentialError::StoreUnavailable)
        }
    }

    fn probe_available() -> bool {
        let _probe_guard = AVAILABILITY_PROBE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let probe_number = AVAILABILITY_PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let ref_name = format!(
            "{}-{}-{probe_number}",
            AVAILABILITY_PROBE_REF_PREFIX,
            std::process::id()
        );
        let entry = match Self::entry(&ref_name) {
            Ok(entry) => entry,
            Err(_) => return false,
        };
        if entry.set_password(AVAILABILITY_PROBE_VALUE).is_err() {
            return false;
        }
        entry.delete_credential().is_ok()
    }
}

impl Default for WindowsCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for WindowsCredentialStore {
    fn set(&self, ref_name: &str, secret: &Secret) -> Result<(), CredentialError> {
        self.ensure_available()?;
        Self::entry(ref_name)?
            .set_password(secret.expose())
            .map_err(map_backend_error)
    }

    fn get(&self, ref_name: &str) -> Result<Secret, CredentialError> {
        self.ensure_available()?;
        Self::entry(ref_name)?
            .get_password()
            .map(Secret::new)
            .map_err(map_backend_error)
    }

    fn exists(&self, ref_name: &str) -> Result<bool, CredentialError> {
        self.ensure_available()?;
        match Self::entry(ref_name)?.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(err) => Err(map_backend_error(err)),
        }
    }

    fn available(&self) -> bool {
        // 只读 NoEntry 不能证明当前会话有写权限；写入再删除进程专属的无敏感值探测项。
        // 任一步失败都按不可用处理，避免后续 set/get 与可用性结论相矛盾。
        *self.available.get_or_init(Self::probe_available)
    }
}

/// keyring 错误映射。`BadEncoding` 变体内嵌原始密钥字节、`Ambiguous`
/// 内嵌凭据元数据，二者一律替换为固定文案，绝不进入 message。
fn map_backend_error(err: keyring::Error) -> CredentialError {
    match err {
        keyring::Error::NoEntry => CredentialError::NotFound,
        keyring::Error::BadEncoding(_) => {
            CredentialError::Backend("存储中的凭据数据无法按 UTF-8 解码".to_string())
        }
        keyring::Error::Ambiguous(_) => {
            CredentialError::Backend("凭据引用在存储中存在多个匹配条目".to_string())
        }
        other => CredentialError::Backend(format!("凭据后端错误：{other}")),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 内存测试替身；`available = false` 模式模拟 OS 存储不可用。
    pub(crate) struct FakeCredentialStore {
        available: bool,
        entries: Mutex<HashMap<String, String>>,
    }

    impl FakeCredentialStore {
        pub(crate) fn new() -> Self {
            FakeCredentialStore {
                available: true,
                entries: Mutex::new(HashMap::new()),
            }
        }

        pub(crate) fn unavailable() -> Self {
            FakeCredentialStore {
                available: false,
                entries: Mutex::new(HashMap::new()),
            }
        }
    }

    impl CredentialStore for FakeCredentialStore {
        fn set(&self, ref_name: &str, secret: &Secret) -> Result<(), CredentialError> {
            if !self.available {
                return Err(CredentialError::StoreUnavailable);
            }
            self.entries
                .lock()
                .unwrap()
                .insert(ref_name.to_string(), secret.expose().to_string());
            Ok(())
        }

        fn get(&self, ref_name: &str) -> Result<Secret, CredentialError> {
            if !self.available {
                return Err(CredentialError::StoreUnavailable);
            }
            self.entries
                .lock()
                .unwrap()
                .get(ref_name)
                .map(|v| Secret::new(v.clone()))
                .ok_or(CredentialError::NotFound)
        }

        fn exists(&self, ref_name: &str) -> Result<bool, CredentialError> {
            if !self.available {
                return Err(CredentialError::StoreUnavailable);
            }
            Ok(self.entries.lock().unwrap().contains_key(ref_name))
        }

        fn available(&self) -> bool {
            self.available
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::FakeCredentialStore;
    use super::*;

    const PLAINTEXT: &str = "sk-plain-credential-value-42";

    #[test]
    fn set_get_exists_roundtrip() {
        let store = FakeCredentialStore::new();
        assert!(store.available());
        assert!(!store.exists("halo/pi/openai").unwrap());

        store
            .set("halo/pi/openai", &Secret::new(PLAINTEXT))
            .unwrap();
        assert!(store.exists("halo/pi/openai").unwrap());
        assert_eq!(store.get("halo/pi/openai").unwrap().expose(), PLAINTEXT);
    }

    #[test]
    fn get_missing_ref_returns_not_found() {
        let store = FakeCredentialStore::new();
        let err = store.get("halo/missing").unwrap_err();
        assert!(matches!(err, CredentialError::NotFound));
    }

    #[test]
    fn unavailable_store_fails_closed_for_all_operations() {
        let store = FakeCredentialStore::unavailable();
        assert!(!store.available());
        assert!(matches!(
            store.set("r", &Secret::new(PLAINTEXT)).unwrap_err(),
            CredentialError::StoreUnavailable
        ));
        assert!(matches!(
            store.get("r").unwrap_err(),
            CredentialError::StoreUnavailable
        ));
        assert!(matches!(
            store.exists("r").unwrap_err(),
            CredentialError::StoreUnavailable
        ));
    }

    #[test]
    fn error_messages_never_contain_plaintext() {
        let unavailable = FakeCredentialStore::unavailable();
        let err = unavailable.set("r", &Secret::new(PLAINTEXT)).unwrap_err();
        for rendered in [format!("{err}"), format!("{err:?}")] {
            assert!(!rendered.contains(PLAINTEXT));
        }

        let backend = CredentialError::Backend("平台调用失败".to_string());
        for rendered in [format!("{backend}"), format!("{backend:?}")] {
            assert!(!rendered.contains(PLAINTEXT));
        }
    }
}
