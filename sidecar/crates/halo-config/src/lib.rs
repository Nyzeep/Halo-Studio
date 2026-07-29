//! 受管启动配置、凭据边界（失败关闭）、配置事务。契约见 docs/module-contracts.md 第 3 节。
//!
//! 凭据红线：[`Secret`] 无 `Display`、`Debug` 恒为 `Secret(***)`、不实现 serde；
//! 本 crate 不依赖 serde，序列化路径在类型与依赖两个层面被截断。

mod credential;
mod launch;
mod secret;
mod transaction;

pub use credential::{CredentialError, CredentialStore, WindowsCredentialStore};
pub use launch::{
    build_child_env, credential_env_var_for, validate_launch_config, AgentKind, ConfigError,
    LaunchConfig, ThinkingLevel, ENV_WHITELIST, PI_CREDENTIAL_ENV_VAR,
};
pub use secret::Secret;
pub use transaction::{rollback, ConfigTransaction, TxError, TxReceipt};
