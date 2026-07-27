//! 受管运行时：进程监督、Pi RPC 适配器、OpenCode 回环服务适配器、取消/停止语义。
//! 适配器线协议以 docs/module-contracts.md 第 5 节为权威；本 crate 不依赖其他 halo crate，
//! 线程 + crossbeam-channel，无 async。

mod opencode;
mod pi;
mod process;

pub use opencode::{OpenCodeHandle, OpenCodeRuntime, OPENCODE_LOCKED_VERSION};
pub use pi::{PiHandle, PiRuntime};

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde_json::Value;

/// 受管应用独立健康状态；绝不合并为“全局在线”。
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeState {
    NotProbed,
    Probing,
    Starting,
    Ready,
    Failed { reason: String, recovery_hint: String },
    Stopping,
    Stopped,
}

/// runtime 自有轨迹条目；由 halo-sidecar 映射为契约 TraceItem。
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTraceItem {
    pub kind: String,
    pub text: String,
    pub detail: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    State(RuntimeState),
    Trace(RuntimeTraceItem),
    ActionRequest {
        request_id: String,
        kind: String,
        prompt: String,
    },
    Verification {
        status: String,
        detail: String,
    },
    TaskDone {
        outcome: String,
        summary: String,
    },
}

/// 启动命令；env 已由 halo-config 按白名单构好，可能含注入凭据，故 Debug 不输出任何值。
pub struct LaunchCmd {
    pub exe: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: String,
}

impl fmt::Debug for LaunchCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchCmd")
            .field("exe", &self.exe)
            .field("args", &self.args)
            .field("env", &format_args!("<{} 个变量，值已隐藏>", self.env.len()))
            .field("cwd", &self.cwd)
            .finish()
    }
}

/// runtime 自有任务输入；只携带用户显式提供的内容。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RunTaskSpec {
    pub instructions: String,
    pub files: Vec<String>,
    pub base_diff: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    Graceful,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub ready: Duration,
    pub cancel_grace: Duration,
    pub shutdown_grace: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            ready: Duration::from_secs(10),
            cancel_grace: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(5),
        }
    }
}

/// 错误 message 一律中文、不携带凭据（token、端口等敏感连接信息也不得进入 message）。
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("启动受管应用失败：{0}")]
    Spawn(String),
    #[error("版本探测失败：{0}")]
    Probe(String),
    #[error("运行时未就绪：{0}")]
    NotReady(String),
    #[error("当前运行时状态不允许该操作")]
    InvalidState,
    #[error("与受管应用通信失败：{0}")]
    Io(String),
    #[error("运行时版本不匹配（RUNTIME_VERSION_MISMATCH）：{0}")]
    VersionMismatch(String),
    #[error("受管应用拒绝了本次认证，请重新启动运行时以生成新的认证信息")]
    Unauthorized,
}

/// Mutex 中毒时继续使用内部值：本 crate 的共享状态均为简单标量/映射，恢复使用不破坏不变量，
/// 从而避免在非测试代码里出现 unwrap。
pub(crate) fn lock<'a, T>(m: &'a Mutex<T>) -> MutexGuard<'a, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 把 Pi 通知 / OpenCode 事件对象（TraceItem 同构：kind/text/detail）规范化为 RuntimeEvent。
/// action_request 与 verification 提升为专用变体，其余 kind 保持为 Trace。
pub(crate) fn map_trace_event(params: &Value) -> RuntimeEvent {
    let kind = params
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("agent_note")
        .to_string();
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let detail = params
        .get("detail")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    match kind.as_str() {
        "action_request" => RuntimeEvent::ActionRequest {
            request_id: detail
                .get("request_id")
                .or_else(|| params.get("request_id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind: detail
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("permission")
                .to_string(),
            prompt: match detail.get("prompt").and_then(Value::as_str) {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => text,
            },
        },
        "verification" => RuntimeEvent::Verification {
            status: detail
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("not_run")
                .to_string(),
            detail: match detail.get("detail").and_then(Value::as_str) {
                Some(d) if !d.is_empty() => d.to_string(),
                _ => text,
            },
        },
        _ => RuntimeEvent::Trace(RuntimeTraceItem { kind, text, detail }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn launch_cmd_debug_hides_env_values() {
        let mut env = HashMap::new();
        env.insert("HALO_OC_TOKEN".to_string(), "super-secret-value".to_string());
        let cmd = LaunchCmd {
            exe: "pi.exe".into(),
            args: vec!["--rpc".into()],
            env,
            cwd: "D:\\repo".into(),
        };
        let dbg = format!("{cmd:?}");
        assert!(!dbg.contains("super-secret-value"));
        assert!(dbg.contains("值已隐藏"));
    }

    #[test]
    fn timeouts_default_matches_contract() {
        let t = Timeouts::default();
        assert_eq!(t.ready, Duration::from_secs(10));
        assert_eq!(t.cancel_grace, Duration::from_secs(10));
        assert_eq!(t.shutdown_grace, Duration::from_secs(5));
    }

    #[test]
    fn map_trace_event_action_request_and_verification() {
        let ev = map_trace_event(&json!({
            "kind": "action_request",
            "text": "需要权限",
            "detail": {"request_id": "ar-1", "kind": "permission", "prompt": "允许写入 src/a.rs 吗？"}
        }));
        assert_eq!(
            ev,
            RuntimeEvent::ActionRequest {
                request_id: "ar-1".into(),
                kind: "permission".into(),
                prompt: "允许写入 src/a.rs 吗？".into()
            }
        );

        let ev = map_trace_event(&json!({
            "kind": "verification",
            "detail": {"status": "passed", "detail": "cargo test 全部通过"}
        }));
        assert_eq!(
            ev,
            RuntimeEvent::Verification {
                status: "passed".into(),
                detail: "cargo test 全部通过".into()
            }
        );
    }

    #[test]
    fn map_trace_event_plain_kinds_stay_trace() {
        let ev = map_trace_event(&json!({"kind": "phase", "text": "planning", "detail": {}}));
        match ev {
            RuntimeEvent::Trace(item) => {
                assert_eq!(item.kind, "phase");
                assert_eq!(item.text, "planning");
            }
            other => panic!("应为 Trace，实际 {other:?}"),
        }
    }
}
