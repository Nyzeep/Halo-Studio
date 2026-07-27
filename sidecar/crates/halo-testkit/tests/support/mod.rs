//! 测试辅助：子进程守护（用例失败时也不泄漏假进程）。
#![allow(dead_code)]

use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

pub struct KillOnDrop(pub Child);

impl KillOnDrop {
    pub fn new(child: Child) -> Self {
        Self(child)
    }

    /// 进程仍在运行返回 true。
    pub fn try_running(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }

    /// 在超时内轮询等待退出；超时未退出返回 None。
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.0.try_wait() {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    pub fn kill_now(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
