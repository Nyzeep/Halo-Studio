//! 子进程监督抽象：单元测试用测试替身注入，生产路径包装 std::process::Child。

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::RuntimeError;

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
    let output = Command::new(exe)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| RuntimeError::Probe(format!("无法执行 {app_name} 版本探测命令：{e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().unwrap_or("");
    parse_semver_token(first).ok_or_else(|| {
        RuntimeError::Probe(format!("{app_name} 版本输出首行未包含 semver：{first}"))
    })
}

/// 在一行文本中寻找第一个 semver 形式的 token（允许 v 前缀与 -pre/+build 后缀）。
pub(crate) fn parse_semver_token(line: &str) -> Option<String> {
    for raw in line.split_whitespace() {
        let tok = raw.trim_start_matches('v');
        let core = tok
            .splitn(2, |c| c == '-' || c == '+')
            .next()
            .unwrap_or(tok);
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(tok.to_string());
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
            Some("1.4.0-beta.1".to_string())
        );
        assert_eq!(parse_semver_token("no version here"), None);
        assert_eq!(parse_semver_token("1.4"), None);
        assert_eq!(parse_semver_token(""), None);
    }
}
