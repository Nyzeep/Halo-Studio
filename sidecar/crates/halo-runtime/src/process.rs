//! 子进程监督抽象：单元测试用测试替身注入，生产路径包装 std::process::Child。

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::RuntimeError;

const PROBE_ENV_WHITELIST: &[&str] = &[
    "SYSTEMROOT",
    "WINDIR",
    "PATH",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "COMSPEC",
    "PATHEXT",
    "SystemDrive",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
];

pub(crate) trait ChildProcess: Send {
    fn has_exited(&mut self) -> bool;
    fn kill(&mut self);
}

pub(crate) struct RealChild(Child);

impl RealChild {
    pub(crate) fn new(child: Child) -> Self {
        Self(child)
    }
}

impl ChildProcess for RealChild {
    fn has_exited(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(Some(_)))
    }

    fn kill(&mut self) {
        let _ = self.0.kill();
        // kill 后必须 wait，避免 Windows 上残留僵尸句柄
        let _ = self.0.wait();
    }
}

/// 在 grace 时限内轮询等待子进程退出；true = 已自行退出。
pub(crate) fn wait_exit(child: &mut dyn ChildProcess, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        if child.has_exited() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// 执行 `<exe> --version` 并从首行解析 semver。
pub(crate) fn probe_version(exe: &str, app_name: &str) -> Result<String, RuntimeError> {
    let host: HashMap<String, String> = std::env::vars().collect();
    let mut command = Command::new(exe);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_clear()
        .envs(probe_env(&host));
    let output = command
        .output()
        .map_err(|_| RuntimeError::Probe(format!("无法执行 {app_name} 版本探测命令")))?;
    if !output.status.success() {
        return Err(RuntimeError::Probe(format!(
            "{app_name} 版本探测命令未成功完成"
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().unwrap_or("");
    parse_semver_token(first)
        .ok_or_else(|| RuntimeError::Probe(format!("{app_name} 版本输出格式不受支持")))
}

/// 在一行文本中寻找第一个 semver 形式的 token（允许 v 前缀与 -pre/+build 后缀）。
fn probe_env(host: &HashMap<String, String>) -> HashMap<String, String> {
    let mut env = HashMap::new();
    for canonical_name in PROBE_ENV_WHITELIST {
        if let Some(value) = host.get(*canonical_name) {
            env.insert((*canonical_name).to_string(), value.clone());
            continue;
        }
        if let Some((_, value)) = host
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(canonical_name))
            .min_by(|left, right| left.0.cmp(right.0))
        {
            env.insert((*canonical_name).to_string(), value.clone());
        }
    }
    env
}

pub(crate) fn parse_semver_token(line: &str) -> Option<String> {
    for raw in line.split_whitespace() {
        let token = raw.strip_prefix('v').unwrap_or(raw);
        let split_at = token.find(['-', '+']);
        let (core, suffix) = match split_at {
            Some(index) => (&token[..index], Some(&token[index..])),
            None => (token, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|part| {
                    !part.is_empty()
                        && part.chars().all(|character| character.is_ascii_digit())
                        && part.parse::<u64>().is_ok()
                })
        {
            let core = parts.join(".");
            return Some(match suffix {
                Some(value) if value.starts_with('-') => format!("{core}-pre-release"),
                Some(_) => format!("{core}+build"),
                None => core,
            });
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod testchild {
    use super::ChildProcess;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// 测试替身：由测试脚本控制退出/记录强杀。
    #[derive(Clone)]
    pub(crate) struct TestChild {
        pub(crate) exited: Arc<AtomicBool>,
        pub(crate) killed: Arc<AtomicBool>,
    }

    impl TestChild {
        pub(crate) fn new() -> Self {
            Self {
                exited: Arc::new(AtomicBool::new(false)),
                killed: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl ChildProcess for TestChild {
        fn has_exited(&mut self) -> bool {
            self.exited.load(Ordering::SeqCst)
        }

        fn kill(&mut self) {
            self.killed.store(true, Ordering::SeqCst);
            self.exited.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_semver_token_variants() {
        assert_eq!(parse_semver_token("pi 1.4.0"), Some("1.4.0".to_string()));
        assert_eq!(parse_semver_token("1.4.0"), Some("1.4.0".to_string()));
        assert_eq!(parse_semver_token("v2.0.1"), Some("2.0.1".to_string()));
        assert_eq!(
            parse_semver_token("pi version 1.4.0-beta.1 (windows)"),
            Some("1.4.0-pre-release".to_string())
        );
        assert_eq!(parse_semver_token("no version here"), None);
        assert_eq!(parse_semver_token("1.4"), None);
        assert_eq!(parse_semver_token(""), None);
    }

    #[test]
    fn probe_environment_keeps_only_the_platform_whitelist() {
        let mut host = HashMap::new();
        host.insert("Path".to_string(), "C:\\Windows".to_string());
        host.insert("OPENAI_API_KEY".to_string(), "probe-secret".to_string());
        host.insert(
            "OPENCODE_SERVER_PASSWORD".to_string(),
            "server-secret".to_string(),
        );

        let env = probe_env(&host);
        assert_eq!(env.get("PATH").map(String::as_str), Some("C:\\Windows"));
        assert!(!env.contains_key("OPENAI_API_KEY"));
        assert!(!env.contains_key("OPENCODE_SERVER_PASSWORD"));
    }

    #[test]
    fn version_parser_does_not_preserve_untrusted_suffix_text() {
        let parsed = parse_semver_token("opencode 1.18.5-secret-marker");
        assert_eq!(parsed.as_deref(), Some("1.18.5-pre-release"));
        assert!(!parsed.unwrap_or_default().contains("secret-marker"));
    }
}
