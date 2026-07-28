//! OpenCode 1.x 受管回环服务适配器。
//!
//! 仅接受经过验证的稳定 1.x 兼容性档案：服务绑定回环地址，每次启动生成
//! 新的 Basic 认证密码，并以 OpenCode Server 的 `/global/health` 完成真实就绪检查。
//! 端口、认证密码和 Authorization 值只保留在私有句柄中，不进入 Debug、错误或事件。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use rand::RngCore;
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::process::{probe_version, ChildProcess, RealChild};
use crate::{
    lock, LaunchCmd, RunTaskSpec, RuntimeError, RuntimeEvent, RuntimeState, RuntimeTraceItem,
    StopOutcome, Timeouts,
};

/// 已验证的 OpenCode Server 兼容性档案标识。新主版本须建立新档案后才能启动。
pub const OPENCODE_COMPATIBILITY_PROFILE: &str = "opencode-server-1.x";
/// 当前档案支持的最低稳定版 OpenCode。
pub const OPENCODE_MIN_SUPPORTED_VERSION: &str = "1.18.5";

const OPENCODE_DEFAULT_USERNAME: &str = "opencode";
const MAX_TRACE_TEXT: usize = 240;
const OPENCODE_ISOLATED_STATE_DIRS: [(&str, &str); 4] = [
    ("XDG_CONFIG_HOME", "config"),
    ("XDG_DATA_HOME", "data"),
    ("XDG_CACHE_HOME", "cache"),
    ("XDG_STATE_HOME", "state"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StableVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl StableVersion {
    fn parse(version: &str) -> Option<Self> {
        let mut pieces = version.split('.');
        let major = parse_component(pieces.next()?)?;
        let minor = parse_component(pieces.next()?)?;
        let patch = parse_component(pieces.next()?)?;
        if pieces.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

fn parse_component(component: &str) -> Option<u64> {
    if component.is_empty()
        || !component.chars().all(|c| c.is_ascii_digit())
        || (component.len() > 1 && component.starts_with('0'))
    {
        return None;
    }
    component.parse().ok()
}

enum ReadyLine {
    Confirmed,
    AddressMismatch,
    StreamClosed,
}

fn listening_url(line: &str) -> Option<&str> {
    let marker = "opencode server listening on ";
    let lower = line.to_ascii_lowercase();
    let offset = lower.find(marker)?;
    line[offset + marker.len()..]
        .split_whitespace()
        .next()
        .map(|url| url.trim_end_matches('/'))
}

fn watch_ready_line(stdout: std::process::ChildStdout, port: u16) -> Receiver<ReadyLine> {
    let (tx, rx) = bounded(1);
    let expected = format!("http://127.0.0.1:{port}");
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut confirmed = false;
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let Some(url) = listening_url(&line) else {
                continue;
            };
            if confirmed {
                continue;
            }
            if url == expected {
                // 保持消费 stdout，避免服务后续诊断输出堵塞或因管道关闭退出。
                let _ = tx.send(ReadyLine::Confirmed);
                confirmed = true;
                continue;
            }
            let _ = tx.send(ReadyLine::AddressMismatch);
            return;
        }
        if !confirmed {
            let _ = tx.send(ReadyLine::StreamClosed);
        }
    });
    rx
}

fn report_start_failure(tx: &Sender<RuntimeEvent>, reason: &str, recovery_hint: &str) {
    let _ = tx.send(RuntimeEvent::State(RuntimeState::Failed {
        reason: reason.to_string(),
        recovery_hint: recovery_hint.to_string(),
    }));
}

pub struct OpenCodeRuntime;

impl OpenCodeRuntime {
    /// 探测 `<exe> --version`，返回首行中的 semver 文本。
    pub fn probe(exe: &str) -> Result<String, RuntimeError> {
        probe_version(exe, "OpenCode")
    }

    /// 兼容档案仅接受稳定的 `>= 1.18.5, < 2.0.0` OpenCode Server。
    /// 预发布、带前缀、畸形版本和未知主版本均失败关闭。
    pub fn is_compatible_version(version: &str) -> bool {
        let Some(version) = StableVersion::parse(version) else {
            return false;
        };
        version.major == 1 && (version.minor > 18 || (version.minor == 18 && version.patch >= 5))
    }

    /// 启动真实 OpenCode Server。认证只存在于该次进程及私有回环句柄中。
    pub fn start(
        cmd: LaunchCmd,
        tx: Sender<RuntimeEvent>,
        opts: Timeouts,
    ) -> Result<OpenCodeHandle, RuntimeError> {
        let port = match pick_free_port() {
            Ok(port) => port,
            Err(error) => {
                report_start_failure(
                    &tx,
                    "无法申请 OpenCode 回环服务端口",
                    "请检查本机网络策略后重试；不会回退到非回环服务",
                );
                return Err(error);
            }
        };
        let password = random_password();
        let LaunchCmd { exe, mut env, cwd } = cmd;
        let workspace_directory = cwd.clone();
        let runtime_dir = match isolated_opencode_state(&mut env) {
            Ok(runtime_dir) => runtime_dir,
            Err(error) => {
                report_start_failure(
                    &tx,
                    "无法创建 OpenCode 隔离运行时目录",
                    "请确认系统临时目录可用后重新启动；不会读取或复用全局 OpenCode 配置",
                );
                return Err(error);
            }
        };

        let mut command = Command::new(&exe);
        command
            .arg("serve")
            .arg("--hostname")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string());
        // 子进程环境 = halo-config 构造的白名单环境 + 本次 OpenCode Server 认证。
        command.env_clear();
        command.envs(&env);
        command.env("OPENCODE_SERVER_PASSWORD", &password);
        if !cwd.is_empty() {
            command.current_dir(&cwd);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                report_start_failure(
                    &tx,
                    "无法启动 OpenCode 进程",
                    "请确认 OpenCode 可执行文件有效且未被安全策略阻止后重试",
                );
                return Err(RuntimeError::Spawn("无法启动 OpenCode 进程".to_string()));
            }
        };

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                report_start_failure(
                    &tx,
                    "无法确认 OpenCode 回环服务已监听",
                    "请重新启动 OpenCode；若问题持续，请检查可执行文件是否完整",
                );
                return Err(RuntimeError::Spawn(
                    "无法读取 OpenCode 就绪输出".to_string(),
                ));
            }
        };
        let ready_line = watch_ready_line(stdout, port);

        let handle = connect_after_ready_line(
            port,
            password,
            Some(Box::new(RealChild::new(child))),
            tx,
            opts,
            Some(ready_line),
            Some(runtime_dir),
            workspace_directory,
        )?;
        handle.monitor_child_exit();
        Ok(handle)
    }
}

/// 绑定 127.0.0.1:0 取得空闲回环端口后立即释放。
fn pick_free_port() -> Result<u16, RuntimeError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| RuntimeError::Spawn("无法申请 OpenCode 回环服务端口".to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|_| RuntimeError::Spawn("无法读取 OpenCode 回环服务端口".to_string()))?
        .port();
    drop(listener);
    Ok(port)
}

/// 32 字节随机密码编码为 64 个十六进制字符；每次启动均重新生成。
fn random_password() -> String {
    use std::fmt::Write as _;

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut password = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(password, "{byte:02x}");
    }
    password
}

/// 让每个受管 OpenCode 进程使用一次性目录，而不是用户的全局 OpenCode 状态。
/// `USERPROFILE` 仍可保留给 Windows 子进程基础设施，但 OpenCode 的配置、数据、缓存
/// 和状态根都由本次运行显式覆盖；目录由私有句柄持有并在退出后清理。
fn isolated_opencode_state(env: &mut HashMap<String, String>) -> Result<TempDir, RuntimeError> {
    let runtime_dir = tempfile::Builder::new()
        .prefix("halo-opencode-")
        .tempdir()
        .map_err(|_| RuntimeError::Spawn("无法创建 OpenCode 隔离运行时目录".to_string()))?;

    for (variable, directory_name) in OPENCODE_ISOLATED_STATE_DIRS {
        let path = runtime_dir.path().join(directory_name);
        std::fs::create_dir_all(&path)
            .map_err(|_| RuntimeError::Spawn("无法创建 OpenCode 隔离运行时目录".to_string()))?;
        env.insert(variable.to_string(), path.to_string_lossy().into_owned());
    }

    Ok(runtime_dir)
}

fn basic_authorization(password: &str) -> String {
    let credentials = format!("{OPENCODE_DEFAULT_USERNAME}:{password}");
    format!("Basic {}", STANDARD.encode(credentials))
}

/// 单轮真实会话的完成闸门。新建 session 可能先广播初始状态，而首轮也可能在
/// `prompt_async` 返回前完成；因此不能把观测到 busy 当作完成的前置条件。
#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionRoundPhase {
    Inactive,
    PromptSubmitting,
    AwaitingCompletion,
    CheckingCompletion,
    Completed,
}

#[derive(Clone, Copy)]
struct SessionRoundGate {
    phase: SessionRoundPhase,
    event_stream_ended: bool,
}

impl Default for SessionRoundGate {
    fn default() -> Self {
        Self {
            phase: SessionRoundPhase::Inactive,
            event_stream_ended: false,
        }
    }
}

struct OcShared {
    state: Mutex<RuntimeState>,
    tx: Sender<RuntimeEvent>,
    agent: ureq::Agent,
    // 连接细节及密码只存在私有字段；任何可观察文本均不得包含它们。
    port: u16,
    password: String,
    // 受信任工作区只用于实例路由，绝不进入错误、事件或 Debug 输出。
    workspace_directory: String,
    // OpenCode session id 是远程实现细节，只在本次运行的私有句柄中保存。
    session_id: Mutex<Option<String>>,
    round_gate: Mutex<SessionRoundGate>,
    child: Mutex<Option<Box<dyn ChildProcess>>>,
    runtime_dir: Mutex<Option<TempDir>>,
}

impl OcShared {
    fn set_state(&self, state: RuntimeState) {
        *lock(&self.state) = state.clone();
        let _ = self.tx.send(RuntimeEvent::State(state));
    }

    fn set_failed(&self, reason: &str, recovery_hint: &str) {
        self.set_state(RuntimeState::Failed {
            reason: reason.to_string(),
            recovery_hint: recovery_hint.to_string(),
        });
    }

    fn begin_initial_round_submission(&self) -> bool {
        let mut gate = lock(&self.round_gate);
        if gate.phase != SessionRoundPhase::Inactive {
            return false;
        }
        *gate = SessionRoundGate {
            phase: SessionRoundPhase::PromptSubmitting,
            event_stream_ended: false,
        };
        true
    }

    fn reset_initial_round(&self) {
        *lock(&self.round_gate) = SessionRoundGate::default();
    }

    fn mark_initial_prompt_accepted(&self) -> bool {
        let mut gate = lock(&self.round_gate);
        if gate.phase != SessionRoundPhase::PromptSubmitting {
            return false;
        }
        gate.phase = SessionRoundPhase::AwaitingCompletion;
        true
    }

    fn round_is_completed(&self) -> bool {
        lock(&self.round_gate).phase == SessionRoundPhase::Completed
    }

    fn event_stream_ended(&self) -> bool {
        lock(&self.round_gate).event_stream_ended
    }

    fn should_trace_busy_round(&self) -> bool {
        matches!(
            lock(&self.round_gate).phase,
            SessionRoundPhase::AwaitingCompletion | SessionRoundPhase::CheckingCompletion
        )
    }

    /// 提示被接受后，任何 idle 都只触发一次完成检查。提交中的初始 idle 会被忽略，
    /// 因为后续状态快照和完成消息才是可证明的轮次边界。
    fn note_idle_event(&self) -> bool {
        lock(&self.round_gate).phase == SessionRoundPhase::AwaitingCompletion
    }

    fn claim_completion_check(&self) -> bool {
        let mut gate = lock(&self.round_gate);
        if gate.phase != SessionRoundPhase::AwaitingCompletion {
            return false;
        }
        gate.phase = SessionRoundPhase::CheckingCompletion;
        true
    }

    fn finish_completion_check(&self, completed: bool) -> bool {
        let mut gate = lock(&self.round_gate);
        if gate.phase != SessionRoundPhase::CheckingCompletion {
            return false;
        }
        gate.phase = if completed {
            SessionRoundPhase::Completed
        } else {
            SessionRoundPhase::AwaitingCompletion
        };
        completed
    }

    /// 在提示被接受前断开的 SSE 不能抢先把任务判为失败：确认路径仍可从状态和
    /// 完成消息恢复极速轮次。已接受后的断流则交给任务事件循环失败关闭。
    fn note_event_stream_failure(&self) -> bool {
        let mut gate = lock(&self.round_gate);
        gate.event_stream_ended = true;
        matches!(gate.phase, SessionRoundPhase::AwaitingCompletion)
    }

    /// 停止流程会主动关闭 SSE；此时不能让迟到的读错误覆盖 Stopped。
    fn set_session_failed_if_active(&self) {
        let failure = RuntimeState::Failed {
            reason: "OpenCode 受管会话事件流失败".to_string(),
            recovery_hint: "请检查 OpenCode 的本地日志并重新启动运行时后重试".to_string(),
        };
        let changed = {
            let mut state = lock(&self.state);
            if matches!(*state, RuntimeState::Starting | RuntimeState::Ready) {
                *state = failure.clone();
                true
            } else {
                false
            }
        };
        if changed {
            let _ = self.tx.send(RuntimeEvent::State(failure));
        }
    }

    fn kill_child(&self) {
        if let Some(mut child) = lock(&self.child).take() {
            child.kill();
        }
    }

    fn cleanup_runtime_dir(&self) {
        drop(lock(&self.runtime_dir).take());
    }

    /// 统一回环请求入口。错误信息不包含 URL、端口或 Authorization 值。
    fn request(&self, method: &str, path: &str, timeout: Duration) -> Result<Value, RuntimeError> {
        self.request_json(method, path, None, timeout)
    }

    /// 统一 JSON 请求入口。调用方只能取得受控的 JSON 值，连接细节始终留在此处。
    fn request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, RuntimeError> {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let request = match method {
            "GET" => self.agent.get(&url),
            "POST" => self.agent.post(&url),
            _ => return Err(RuntimeError::Io("不支持的 OpenCode 请求方法".to_string())),
        }
        .set("Authorization", &basic_authorization(&self.password))
        .timeout(timeout);

        let response = match (method, body) {
            ("POST", Some(body)) => request.send_json(body),
            ("GET", None) | ("POST", None) => request.call(),
            _ => return Err(RuntimeError::Io("不支持的 OpenCode 请求方法".to_string())),
        };
        match response {
            Ok(response) => Ok(response.into_json::<Value>().unwrap_or(Value::Null)),
            Err(ureq::Error::Status(401, _)) => Err(RuntimeError::Unauthorized),
            Err(ureq::Error::Status(code, _)) => {
                Err(RuntimeError::Io(format!("OpenCode 返回了 HTTP {code}")))
            }
            Err(ureq::Error::Transport(_)) => Err(RuntimeError::Io(
                "与 OpenCode 的本地服务连接失败".to_string(),
            )),
        }
    }

    /// OpenCode 的实例端点必须显式指定已信任工作区，避免依赖 server 的 cwd 默认值。
    fn instance_path(&self, path: &str) -> String {
        if self.workspace_directory.is_empty() {
            return path.to_string();
        }
        let separator = if path.contains('?') { '&' } else { '?' };
        format!(
            "{path}{separator}directory={}",
            percent_encode(&self.workspace_directory)
        )
    }

    /// 建立真实 SSE 事件流。只在私有线程内消费，原始帧不会转发给 IPC。
    fn open_event_stream(&self) -> Result<Box<dyn Read + Send + Sync>, RuntimeError> {
        let path = self.instance_path("/event");
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let response = self
            .agent
            .get(&url)
            .set("Authorization", &basic_authorization(&self.password))
            .call();
        match response {
            Ok(response) => Ok(response.into_reader()),
            Err(ureq::Error::Status(401, _)) => Err(RuntimeError::Unauthorized),
            Err(ureq::Error::Status(code, _)) => Err(RuntimeError::CapabilityUnavailable(format!(
                "OpenCode 真实事件能力不可用（HTTP {code}）"
            ))),
            Err(ureq::Error::Transport(_)) => {
                Err(RuntimeError::Io("无法连接 OpenCode 真实事件流".to_string()))
            }
        }
    }
}

fn percent_encode(value: &str) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// 健康检查完成后只接受可安全呈现的纯数字版本，避免把外部响应文本带入错误或事件。
fn safe_version_for_reason(version: &str) -> Option<&str> {
    if version.len() <= 32 && version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Some(version)
    } else {
        None
    }
}

/// 就绪流程（生产与测试共用）：真实 `/global/health` + Basic auth + 兼容性档案。
fn connect(
    port: u16,
    password: String,
    child: Option<Box<dyn ChildProcess>>,
    tx: Sender<RuntimeEvent>,
    opts: Timeouts,
) -> Result<OpenCodeHandle, RuntimeError> {
    connect_after_ready_line(port, password, child, tx, opts, None, None, String::new())
}

fn connect_after_ready_line(
    port: u16,
    password: String,
    child: Option<Box<dyn ChildProcess>>,
    tx: Sender<RuntimeEvent>,
    opts: Timeouts,
    ready_line: Option<Receiver<ReadyLine>>,
    runtime_dir: Option<TempDir>,
    workspace_directory: String,
) -> Result<OpenCodeHandle, RuntimeError> {
    let shared = Arc::new(OcShared {
        state: Mutex::new(RuntimeState::Starting),
        tx,
        agent: ureq::agent(),
        port,
        password,
        workspace_directory,
        session_id: Mutex::new(None),
        round_gate: Mutex::new(SessionRoundGate::default()),
        child: Mutex::new(child),
        runtime_dir: Mutex::new(runtime_dir),
    });
    let _ = shared.tx.send(RuntimeEvent::State(RuntimeState::Starting));

    let deadline = Instant::now() + opts.ready;
    if let Some(ready_line) = ready_line {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let ready = ready_line.recv_timeout(remaining);
        match ready {
            Ok(ReadyLine::Confirmed) => {}
            Ok(ReadyLine::AddressMismatch) => {
                let reason = "OpenCode 报告的监听地址与受管回环地址不一致";
                shared.set_failed(reason, "请停止占用回环端口的其他程序后重新启动 OpenCode");
                shared.kill_child();
                return Err(RuntimeError::NotReady(reason.to_string()));
            }
            Ok(ReadyLine::StreamClosed) | Err(RecvTimeoutError::Disconnected) => {
                let reason = "OpenCode 未确认受管回环服务已监听";
                shared.set_failed(
                    reason,
                    "请确认 OpenCode 可执行文件有效且能以回环地址启动后重试",
                );
                shared.kill_child();
                return Err(RuntimeError::NotReady(reason.to_string()));
            }
            Err(RecvTimeoutError::Timeout) => {
                let reason = "OpenCode 回环服务监听确认超时";
                shared.set_failed(
                    reason,
                    "请确认 OpenCode 未被安全策略阻止，并检查是否有程序占用回环端口",
                );
                shared.kill_child();
                return Err(RuntimeError::NotReady(reason.to_string()));
            }
        }
    }
    loop {
        match shared.request("GET", "/global/health", Duration::from_millis(500)) {
            Ok(health) => {
                let healthy = health.get("healthy").and_then(Value::as_bool);
                match healthy {
                    Some(true) => {
                        let Some(version) = health.get("version").and_then(Value::as_str) else {
                            let reason = format!(
                                "OpenCode 健康检查缺少兼容性档案所需的版本能力（{OPENCODE_COMPATIBILITY_PROFILE}）"
                            );
                            shared.set_failed(
                                &reason,
                                "请安装稳定版 OpenCode 1.18.5 或更高的 1.x 版本后重新启动",
                            );
                            shared.kill_child();
                            return Err(RuntimeError::VersionMismatch(
                                "OpenCode 健康检查缺少版本能力".to_string(),
                            ));
                        };
                        if !OpenCodeRuntime::is_compatible_version(version) {
                            let detected = safe_version_for_reason(version).unwrap_or("格式无效");
                            let reason = format!(
                                "OpenCode 版本不受兼容性档案支持（RUNTIME_VERSION_MISMATCH）：检测到 {detected}，需要稳定版 {OPENCODE_MIN_SUPPORTED_VERSION} 或更高的 1.x"
                            );
                            shared.set_failed(
                                &reason,
                                "请安装稳定版 OpenCode 1.18.5 或更高的 1.x 版本后重新启动",
                            );
                            shared.kill_child();
                            return Err(RuntimeError::VersionMismatch(
                                "OpenCode 版本不符合兼容性档案".to_string(),
                            ));
                        }
                        shared.set_state(RuntimeState::Ready);
                        return Ok(OpenCodeHandle { shared });
                    }
                    Some(false) | None => {}
                }
            }
            Err(RuntimeError::Unauthorized) => {
                shared.set_failed(
                    "OpenCode 拒绝了本次启动认证",
                    "请重新启动运行时以生成新的认证信息；若问题持续，请检查 OpenCode 是否被其他程序占用",
                );
                shared.kill_child();
                return Err(RuntimeError::Unauthorized);
            }
            Err(_) => {}
        }

        if Instant::now() >= deadline {
            let reason = "OpenCode 健康检查超时：服务未在限定时间内就绪".to_string();
            shared.set_failed(
                &reason,
                "请确认 OpenCode 可执行文件有效，查看其日志后重新启动运行时",
            );
            shared.kill_child();
            return Err(RuntimeError::NotReady(reason));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub struct OpenCodeHandle {
    shared: Arc<OcShared>,
}

// 手写 Debug：端口和认证密码绝不出现。
impl std::fmt::Debug for OpenCodeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCodeHandle")
            .field("state", &*lock(&self.shared.state))
            .finish_non_exhaustive()
    }
}

enum EventDisposition {
    Continue,
    CompletionHint,
}

fn is_safe_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn initial_prompt_parts(spec: &RunTaskSpec) -> Vec<Value> {
    let mut text = spec.instructions.clone();
    if let Some(notes) = spec.notes.as_deref().filter(|notes| !notes.is_empty()) {
        text.push_str("\n\n补充说明：\n");
        text.push_str(notes);
    }
    if !spec.files.is_empty() {
        text.push_str("\n\n开发者选定的文件：\n");
        for file in &spec.files {
            text.push_str("- ");
            text.push_str(file);
            text.push('\n');
        }
    }
    if let Some(base_diff) = spec.base_diff.as_deref().filter(|diff| !diff.is_empty()) {
        text.push_str("\n开发者提供的基线 Diff：\n");
        text.push_str(base_diff);
    }
    vec![json!({"type": "text", "text": text})]
}

fn event_session_id(properties: &Value) -> Option<&str> {
    properties
        .get("sessionID")
        .or_else(|| properties.get("session_id"))
        .and_then(Value::as_str)
}

fn event_is_for_session(properties: &Value, session_id: &str) -> bool {
    event_session_id(properties).is_some_and(|id| id == session_id)
}

fn send_trace(shared: &OcShared, kind: &str, text: &str, detail: Value) {
    let _ = shared.tx.send(RuntimeEvent::Trace(RuntimeTraceItem {
        kind: kind.to_string(),
        text: limit_text(text, MAX_TRACE_TEXT),
        detail,
    }));
}

fn limit_text(value: &str, maximum: usize) -> String {
    let mut limited = String::new();
    for character in value.chars().take(maximum) {
        limited.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    limited
}

/// 从 SSE 中取一个 data 帧。未知 SSE 字段和空 keepalive 帧均不影响会话。
fn next_sse_event(reader: &mut dyn BufRead) -> Result<Option<Value>, RuntimeError> {
    let mut data = String::new();
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|_| RuntimeError::Io("读取 OpenCode 事件流失败".to_string()))?;
        if read == 0 {
            if data.is_empty() {
                return Ok(None);
            }
            return serde_json::from_str(&data)
                .map(Some)
                .map_err(|_| RuntimeError::Io("OpenCode 事件流包含无效帧".to_string()));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if data.is_empty() {
                continue;
            }
            return serde_json::from_str(&data)
                .map(Some)
                .map_err(|_| RuntimeError::Io("OpenCode 事件流包含无效帧".to_string()));
        }
        if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.trim_start());
        }
    }
}

fn completed_assistant_reply(messages: &Value) -> Result<Option<String>, RuntimeError> {
    let entries = messages
        .as_array()
        .or_else(|| messages.get("messages").and_then(Value::as_array))
        .or_else(|| messages.get("data").and_then(Value::as_array));
    let Some(entries) = entries else {
        return Err(RuntimeError::CapabilityUnavailable(
            "OpenCode 真实消息能力返回了不兼容的响应".to_string(),
        ));
    };

    for entry in entries.iter().rev() {
        let info = entry.get("info").unwrap_or(entry);
        if info.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        if info.get("error").is_some_and(|error| !error.is_null()) {
            return Err(RuntimeError::Io("OpenCode 本轮助手回复失败".to_string()));
        }
        let completed = info
            .pointer("/time/completed")
            .is_some_and(|completed| !completed.is_null());
        if !completed {
            continue;
        }
        let parts = entry
            .get("parts")
            .or_else(|| info.get("parts"))
            .and_then(Value::as_array)
            .ok_or_else(|| {
                RuntimeError::CapabilityUnavailable(
                    "OpenCode 真实消息能力缺少助手文本分段".to_string(),
                )
            })?;
        let text = parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        return Ok((!text.trim().is_empty()).then_some(text));
    }
    Ok(None)
}

fn handle_event(
    shared: &OcShared,
    session_id: &str,
    event: &Value,
) -> Result<EventDisposition, RuntimeError> {
    let event_type = event.get("type").and_then(Value::as_str);
    let null = Value::Null;
    let properties = event.get("properties").unwrap_or(&null);
    match event_type {
        Some("session.status") if event_is_for_session(properties, session_id) => {
            match properties.pointer("/status/type").and_then(Value::as_str) {
                Some("busy") | Some("retry") => {
                    if shared.should_trace_busy_round() {
                        send_trace(
                            shared,
                            "phase",
                            "OpenCode 正在处理任务",
                            json!({"state": "running"}),
                        );
                    }
                    Ok(EventDisposition::Continue)
                }
                Some("idle") if shared.note_idle_event() => Ok(EventDisposition::CompletionHint),
                Some("idle") => Ok(EventDisposition::Continue),
                _ => Ok(EventDisposition::Continue),
            }
        }
        Some("message.part.updated") if event_is_for_session(properties, session_id) => {
            let null = Value::Null;
            let part = properties.get("part").unwrap_or(&null);
            if part.get("type").and_then(Value::as_str) == Some("tool") {
                let state = part
                    .pointer("/state/status")
                    .or_else(|| part.pointer("/state/type"))
                    .and_then(Value::as_str)
                    .unwrap_or("updated");
                let state = match state {
                    "pending" | "running" | "completed" | "error" => state,
                    _ => "updated",
                };
                send_trace(
                    shared,
                    "agent_note",
                    "OpenCode 工具状态已更新",
                    json!({"state": state}),
                );
            } else if part.get("type").and_then(Value::as_str) == Some("text") {
                send_trace(
                    shared,
                    "phase",
                    "OpenCode 正在整理回复",
                    json!({"state": "responding"}),
                );
            }
            Ok(EventDisposition::Continue)
        }
        Some("file.edited") if event_is_for_session(properties, session_id) => {
            send_trace(shared, "file_hint", "OpenCode 报告了文件改动", json!({}));
            Ok(EventDisposition::Continue)
        }
        Some("session.diff") if event_is_for_session(properties, session_id) => {
            send_trace(
                shared,
                "file_hint",
                "OpenCode 更新了会话文件变更",
                json!({}),
            );
            Ok(EventDisposition::Continue)
        }
        Some("todo.updated") if event_is_for_session(properties, session_id) => {
            send_trace(
                shared,
                "phase",
                "OpenCode 更新了任务进度",
                json!({"state": "running"}),
            );
            Ok(EventDisposition::Continue)
        }
        Some("session.error") if event_is_for_session(properties, session_id) => {
            Err(RuntimeError::Io("OpenCode 本轮会话报告失败".to_string()))
        }
        // 心跳、逐字 delta 和将来新增事件都不携带到 Halo 的受控运行轨迹。
        _ => Ok(EventDisposition::Continue),
    }
}

fn session_is_idle(shared: &OcShared, session_id: &str) -> Result<bool, RuntimeError> {
    let path = shared.instance_path("/session/status");
    let status = shared.request("GET", &path, Duration::from_secs(5))?;
    let status = status.get(session_id).ok_or_else(|| {
        RuntimeError::CapabilityUnavailable("OpenCode 真实会话状态能力未返回当前会话".to_string())
    })?;
    match status.get("type").and_then(Value::as_str) {
        Some("idle") => Ok(true),
        Some("busy") | Some("retry") => Ok(false),
        _ => Err(RuntimeError::CapabilityUnavailable(
            "OpenCode 真实会话状态能力返回了不兼容的状态".to_string(),
        )),
    }
}

fn fetch_completed_assistant_reply_once(
    shared: &OcShared,
    session_id: &str,
) -> Result<Option<String>, RuntimeError> {
    let path = shared.instance_path(&format!("/session/{session_id}/message?limit=20"));
    let messages = shared.request("GET", &path, Duration::from_secs(5))?;
    completed_assistant_reply(&messages)
}

fn fetch_completed_assistant_reply(
    shared: &OcShared,
    session_id: &str,
    attempts: usize,
) -> Result<Option<String>, RuntimeError> {
    for attempt in 0..attempts {
        if let Some(reply) = fetch_completed_assistant_reply_once(shared, session_id)? {
            return Ok(Some(reply));
        }
        if attempt + 1 < attempts {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    Ok(None)
}

enum CompletionAttempt {
    Completed,
    Pending,
}

/// 只有抢到完成检查的路径可以查询最终消息并发出回复。idle 事件、状态快照和 EOF
/// 都会走这里，因此乱序与重复事件不能追加两条 Agent 消息。
fn try_complete_round(
    shared: &OcShared,
    session_id: &str,
    message_attempts: usize,
) -> Result<CompletionAttempt, RuntimeError> {
    if !shared.claim_completion_check() {
        return Ok(if shared.round_is_completed() {
            CompletionAttempt::Completed
        } else {
            CompletionAttempt::Pending
        });
    }

    let result: Result<Option<String>, RuntimeError> = (|| {
        if !session_is_idle(shared, session_id)? {
            return Ok(None);
        }
        fetch_completed_assistant_reply(shared, session_id, message_attempts)
    })();
    match result {
        Ok(Some(text)) => {
            if shared.finish_completion_check(true) {
                let _ = shared.tx.send(RuntimeEvent::SessionReply { text });
                Ok(CompletionAttempt::Completed)
            } else {
                Ok(CompletionAttempt::Pending)
            }
        }
        Ok(None) => {
            shared.finish_completion_check(false);
            Ok(CompletionAttempt::Pending)
        }
        Err(error) => {
            shared.finish_completion_check(false);
            Err(error)
        }
    }
}

fn consume_event_stream(
    shared: &OcShared,
    session_id: &str,
    stream: Box<dyn Read + Send + Sync>,
) -> Result<(), RuntimeError> {
    let mut reader = BufReader::new(stream);
    loop {
        let Some(event) = next_sse_event(&mut reader)? else {
            return match try_complete_round(shared, session_id, 6)? {
                CompletionAttempt::Completed => Ok(()),
                CompletionAttempt::Pending => Err(RuntimeError::Io(
                    "OpenCode 事件流在本轮回复完成前意外结束".to_string(),
                )),
            };
        };
        if matches!(
            handle_event(shared, session_id, &event)?,
            EventDisposition::CompletionHint
        ) {
            match try_complete_round(shared, session_id, 1)? {
                CompletionAttempt::Completed => return Ok(()),
                CompletionAttempt::Pending => continue,
            }
        }
    }
}

impl OpenCodeHandle {
    /// 服务就绪后持续监督宿主进程。进程自行退出时，不能继续显示 ready。
    fn monitor_child_exit(&self) {
        let shared = Arc::downgrade(&self.shared);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let Some(shared) = shared.upgrade() else {
                return;
            };

            let active = matches!(
                *lock(&shared.state),
                RuntimeState::Starting | RuntimeState::Ready
            );
            if !active {
                return;
            }

            let exited = {
                let mut child = lock(&shared.child);
                child.as_mut().is_some_and(|child| child.has_exited())
            };
            if !exited {
                continue;
            }

            let failure = RuntimeState::Failed {
                reason: "OpenCode 进程已退出，运行时不再可用".to_string(),
                recovery_hint: "请检查 OpenCode 的本地日志并重新启动运行时".to_string(),
            };
            let changed = {
                let mut state = lock(&shared.state);
                if matches!(*state, RuntimeState::Starting | RuntimeState::Ready) {
                    *state = failure.clone();
                    true
                } else {
                    false
                }
            };
            if changed {
                let _ = shared.tx.send(RuntimeEvent::State(failure));
            }
            shared.cleanup_runtime_dir();
            return;
        });
    }

    fn create_session(&self) -> Result<String, RuntimeError> {
        let path = self.shared.instance_path("/session");
        let response = self
            .shared
            .request_json("POST", &path, Some(&json!({})), Duration::from_secs(5))
            .map_err(|error| match error {
                RuntimeError::Unauthorized => RuntimeError::Unauthorized,
                _ => RuntimeError::CapabilityUnavailable("OpenCode 真实会话能力不可用".to_string()),
            })?;
        let session_id = response.get("id").and_then(Value::as_str).ok_or_else(|| {
            RuntimeError::CapabilityUnavailable(
                "OpenCode 真实会话能力未返回兼容的会话标识".to_string(),
            )
        })?;
        if !is_safe_session_id(session_id) {
            return Err(RuntimeError::CapabilityUnavailable(
                "OpenCode 真实会话能力返回了不安全的会话标识".to_string(),
            ));
        }
        Ok(session_id.to_string())
    }

    fn start_event_listener(&self, session_id: String) -> Result<(), RuntimeError> {
        let (ready_tx, ready_rx) = bounded(1);
        let shared = Arc::clone(&self.shared);
        std::thread::spawn(move || match shared.open_event_stream() {
            Ok(stream) => {
                let _ = ready_tx.send(Ok(()));
                if consume_event_stream(&shared, &session_id, stream).is_err()
                    && shared.note_event_stream_failure()
                {
                    shared.set_session_failed_if_active();
                }
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
        });

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(RuntimeError::CapabilityUnavailable(
                "OpenCode 真实事件能力在限定时间内未就绪".to_string(),
            )),
            Err(RecvTimeoutError::Disconnected) => Err(RuntimeError::CapabilityUnavailable(
                "OpenCode 真实事件能力未能建立".to_string(),
            )),
        }
    }

    fn submit_initial_prompt(
        &self,
        session_id: &str,
        spec: &RunTaskSpec,
    ) -> Result<(), RuntimeError> {
        let path = self
            .shared
            .instance_path(&format!("/session/{session_id}/prompt_async"));
        let body = json!({"parts": initial_prompt_parts(spec)});
        self.shared
            .request_json("POST", &path, Some(&body), Duration::from_secs(5))
            .map(|_| ())
    }

    fn confirm_initial_round_started(&self, session_id: &str) -> Result<(), RuntimeError> {
        for attempt in 0..6 {
            if self.shared.round_is_completed() {
                return Ok(());
            }

            if matches!(
                try_complete_round(&self.shared, session_id, 1)?,
                CompletionAttempt::Completed
            ) {
                return Ok(());
            }

            if !session_is_idle(&self.shared, session_id)? {
                if self.shared.event_stream_ended() {
                    return Err(RuntimeError::CapabilityUnavailable(
                        "OpenCode 真实事件流在首轮开始前意外结束".to_string(),
                    ));
                }
                return Ok(());
            }

            if attempt < 5 {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let reason = if self.shared.event_stream_ended() {
            "OpenCode 真实事件流结束后未找到完成的助手回复"
        } else {
            "OpenCode 真实会话在提示后未返回完成的助手回复"
        };
        Err(RuntimeError::CapabilityUnavailable(reason.to_string()))
    }

    /// 用真实 OpenCode session/message/event 协议执行首轮任务消息。远程 session id
    /// 仅由该句柄保存，轮次结束后只发出受控 SessionReply，不会生成交付终态。
    pub fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError> {
        if *lock(&self.shared.state) != RuntimeState::Ready {
            return Err(RuntimeError::InvalidState);
        }
        {
            let mut session = lock(&self.shared.session_id);
            if session.is_some() {
                return Err(RuntimeError::InvalidState);
            }
            // 占位防止并发的 task.create 重复建立远程会话。
            *session = Some(String::new());
        }

        let session_id = match self.create_session() {
            Ok(session_id) => session_id,
            Err(error) => {
                *lock(&self.shared.session_id) = None;
                return Err(error);
            }
        };
        if !self.shared.begin_initial_round_submission() {
            *lock(&self.shared.session_id) = None;
            return Err(RuntimeError::InvalidState);
        }
        if let Err(error) = self.start_event_listener(session_id.clone()) {
            self.shared.reset_initial_round();
            *lock(&self.shared.session_id) = None;
            return Err(error);
        }
        *lock(&self.shared.session_id) = Some(session_id.clone());
        if let Err(error) = self.submit_initial_prompt(&session_id, spec) {
            self.shared.reset_initial_round();
            *lock(&self.shared.session_id) = None;
            return Err(error);
        }
        if !self.shared.mark_initial_prompt_accepted() {
            self.shared.reset_initial_round();
            *lock(&self.shared.session_id) = None;
            return Err(RuntimeError::InvalidState);
        }
        if let Err(error) = self.confirm_initial_round_started(&session_id) {
            self.shared.reset_initial_round();
            *lock(&self.shared.session_id) = None;
            return Err(error);
        }
        Ok(())
    }

    /// #11 不扩大为取消/显式结束流程；绝不回退到旧的 `/cancel` 协议。
    pub fn cancel_native(&self) {}

    /// OpenCode 的 dispose 只释放实例资源，不会退出 server 进程。
    /// 请求成功后主动回收子进程才是该协议下的 Graceful；dispose 失败或超时则 Forced。
    pub fn stop(&self, grace: Duration) -> StopOutcome {
        if matches!(*lock(&self.shared.state), RuntimeState::Stopped) {
            return StopOutcome::Graceful;
        }
        self.shared.set_state(RuntimeState::Stopping);
        let dispose_ok = self
            .shared
            .request("POST", "/global/dispose", grace)
            .is_ok();
        self.shared.kill_child();
        self.shared.cleanup_runtime_dir();
        let outcome = if dispose_ok {
            StopOutcome::Graceful
        } else {
            StopOutcome::Forced
        };
        self.shared.set_state(RuntimeState::Stopped);
        outcome
    }

    pub fn state(&self) -> RuntimeState {
        lock(&self.shared.state).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::{unbounded, Receiver};
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};

    const PASSWORD: &str = "test-password-not-for-output";

    fn short_opts() -> Timeouts {
        Timeouts {
            ready: Duration::from_millis(800),
            cancel_grace: Duration::from_secs(2),
            shutdown_grace: Duration::from_secs(2),
        }
    }

    struct TestServer {
        port: u16,
        seen: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    /// 临时 OpenCode Server：仅接受文档规定的 Basic 认证，不记录认证内容。
    fn spawn_server<F>(expected_password: &str, handler: F) -> TestServer
    where
        F: Fn(&str, &str) -> (u16, String) + Send + 'static,
    {
        let server = tiny_http::Server::http(("127.0.0.1", 0)).expect("无法启动假服务");
        let port = match server.server_addr() {
            tiny_http::ListenAddr::IP(address) => address.port(),
            #[allow(unreachable_patterns)]
            _ => panic!("假服务必须绑定 IP"),
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let expected = basic_authorization(expected_password);
        let seen_bg = Arc::clone(&seen);
        let stop_bg = Arc::clone(&stop);
        let join = std::thread::spawn(move || {
            while !stop_bg.load(Ordering::SeqCst) {
                let request = match server.recv_timeout(Duration::from_millis(50)) {
                    Ok(Some(request)) => request,
                    _ => continue,
                };
                let method = request.method().as_str().to_string();
                let url = request.url().to_string();
                let authorized = request.headers().iter().any(|header| {
                    header.field.equiv("Authorization") && header.value.as_str() == expected
                });
                seen_bg.lock().unwrap().push(format!("{method} {url}"));
                let (status, body) = if authorized {
                    handler(&method, &url)
                } else {
                    (401, json!({"error": "unauthorized"}).to_string())
                };
                let header =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .unwrap();
                let _ = request.respond(
                    tiny_http::Response::from_string(body)
                        .with_status_code(status)
                        .with_header(header),
                );
            }
        });
        TestServer {
            port,
            seen,
            stop,
            join: Some(join),
        }
    }

    fn healthy_handler(method: &str, path: &str) -> (u16, String) {
        match (method, path) {
            ("GET", "/global/health") => (
                200,
                json!({"healthy": true, "version": OPENCODE_MIN_SUPPORTED_VERSION}).to_string(),
            ),
            ("POST", "/global/dispose") => (200, json!({}).to_string()),
            _ => (404, json!({"error": "not_found"}).to_string()),
        }
    }

    fn wait_event(
        rx: &Receiver<RuntimeEvent>,
        pred: impl Fn(&RuntimeEvent) -> bool,
    ) -> RuntimeEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("等待目标事件超时");
            let event = rx.recv_timeout(remaining).expect("事件通道超时或断开");
            if pred(&event) {
                return event;
            }
        }
    }

    #[test]
    fn start_ready_after_basic_authenticated_real_health_check() {
        let server = spawn_server(PASSWORD, healthy_handler);
        let (tx, rx) = unbounded();
        let handle = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect("Basic 认证和健康检查通过后应就绪");
        assert_eq!(handle.state(), RuntimeState::Ready);
        wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Ready))
        });
        assert!(
            server
                .seen
                .lock()
                .unwrap()
                .iter()
                .any(|entry| entry == "GET /global/health"),
            "启动必须调用 OpenCode 的真实健康端点"
        );
    }

    #[test]
    fn unhealthy_health_response_times_out_as_failed() {
        let server = spawn_server(PASSWORD, |method, path| match (method, path) {
            ("GET", "/global/health") => (200, json!({"healthy": false}).to_string()),
            _ => (404, "{}".to_string()),
        });
        let (tx, rx) = unbounded();
        let error = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect_err("未健康的服务不能就绪");
        assert!(matches!(error, RuntimeError::NotReady(_)));
        let event = wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match event {
            RuntimeEvent::State(RuntimeState::Failed {
                reason,
                recovery_hint,
            }) => {
                assert!(reason.contains("健康检查"));
                assert!(!recovery_hint.is_empty());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn incompatible_health_version_fails_closed_with_recovery_hint() {
        let server = spawn_server(PASSWORD, |method, path| match (method, path) {
            ("GET", "/global/health") => (
                200,
                json!({"healthy": true, "version": "1.18.4"}).to_string(),
            ),
            _ => (404, "{}".to_string()),
        });
        let (tx, rx) = unbounded();
        let error = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect_err("低于档案范围的版本必须失败");
        assert!(matches!(error, RuntimeError::VersionMismatch(_)));
        let event = wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match event {
            RuntimeEvent::State(RuntimeState::Failed {
                reason,
                recovery_hint,
            }) => {
                assert!(reason.contains("兼容性档案"));
                assert!(reason.contains("1.18.4"));
                assert!(!recovery_hint.is_empty());
            }
            _ => unreachable!(),
        }
        assert!(
            !server
                .seen
                .lock()
                .unwrap()
                .iter()
                .any(|entry| entry == "GET /version"),
            "不得退回旧版本端点"
        );
    }

    #[test]
    fn missing_health_version_capability_fails_closed() {
        let server = spawn_server(PASSWORD, |method, path| match (method, path) {
            ("GET", "/global/health") => (200, json!({"healthy": true}).to_string()),
            _ => (404, "{}".to_string()),
        });
        let (tx, rx) = unbounded();
        let error = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect_err("缺少健康版本能力必须失败");
        assert!(matches!(error, RuntimeError::VersionMismatch(_)));
        let event = wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match event {
            RuntimeEvent::State(RuntimeState::Failed { reason, .. }) => {
                assert!(reason.contains("缺少兼容性档案"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn basic_authentication_failure_fails_fast() {
        let server = spawn_server("a-different-password", healthy_handler);
        let (tx, rx) = unbounded();
        let started = Instant::now();
        let error = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect_err("认证失败必须失败关闭");
        assert!(matches!(error, RuntimeError::Unauthorized));
        assert!(started.elapsed() < Duration::from_millis(700));
        wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
    }

    #[test]
    fn missing_real_session_capability_fails_closed_without_legacy_fallback() {
        let server = spawn_server(PASSWORD, healthy_handler);
        let (tx, _rx) = unbounded();
        let handle =
            connect(server.port, PASSWORD.to_string(), None, tx, short_opts()).expect("应就绪");
        let error = handle
            .run_task(&RunTaskSpec {
                instructions: "实现功能 X".into(),
                files: vec![],
                base_diff: None,
                notes: None,
            })
            .expect_err("缺少真实会话能力时必须明确拒绝");
        assert!(matches!(error, RuntimeError::CapabilityUnavailable(_)));
        handle.cancel_native();
        assert_eq!(
            handle.stop(Duration::from_millis(100)),
            StopOutcome::Graceful
        );
        let seen = server.seen.lock().unwrap().clone();
        assert!(
            seen.iter().any(|entry| entry == "POST /session"),
            "必须探测真实 OpenCode session 能力"
        );
        for legacy_path in [
            "/task",
            "/events",
            "/cancel",
            "/shutdown",
            "/health",
            "/version",
        ] {
            assert!(
                !seen.iter().any(|entry| entry.contains(legacy_path)),
                "不得请求旧假设协议端点 {legacy_path}：{seen:?}"
            );
        }
        assert!(seen.iter().any(|entry| entry == "POST /global/dispose"));
    }

    #[test]
    fn completed_assistant_reply_uses_final_text_parts_without_tool_or_reasoning_output() {
        let messages = json!([
            {
                "info": {"role": "assistant", "time": {"completed": 1}},
                "parts": [{"type": "text", "text": "较早回复"}]
            },
            {
                "info": {"role": "assistant", "time": {"completed": 2}},
                "parts": [
                    {"type": "reasoning", "text": "不应进入活动会话"},
                    {"type": "tool", "output": "原始工具日志"},
                    {"type": "text", "text": "真实最终回复"}
                ]
            }
        ]);

        assert_eq!(
            completed_assistant_reply(&messages).expect("消息形状应兼容"),
            Some("真实最终回复".to_string())
        );
    }

    #[test]
    fn completed_assistant_reply_requires_nonempty_displayable_text() {
        let messages = json!([
            {
                "info": {"role": "assistant", "time": {"completed": 1}},
                "parts": [
                    {"type": "reasoning", "text": "内部推理"},
                    {"type": "tool", "output": "原始工具日志"},
                    {"type": "text", "text": "  \n\t"}
                ]
            }
        ]);

        assert_eq!(
            completed_assistant_reply(&messages).expect("消息形状应兼容"),
            None
        );
    }

    #[test]
    fn sse_parser_reads_data_frames_without_exposing_transport_fields() {
        let payload = b"event: ignored\ndata: {\"id\":\"evt-1\",\"type\":\"session.status\",\"properties\":{\"sessionID\":\"ses_1\",\"status\":{\"type\":\"idle\"}}}\n\n";
        let mut reader = BufReader::new(std::io::Cursor::new(payload));
        let event = next_sse_event(&mut reader)
            .expect("SSE 帧应可解析")
            .expect("应存在事件");
        assert_eq!(event["type"], "session.status");
        assert_eq!(event["properties"]["status"]["type"], "idle");
    }

    #[test]
    fn initial_prompt_keeps_explicit_task_context_in_the_first_text_part() {
        let parts = initial_prompt_parts(&RunTaskSpec {
            instructions: "修复首轮会话".into(),
            files: vec!["src/runtime.rs".into()],
            base_diff: Some("diff --git a/a b/a".into()),
            notes: Some("只改指定范围".into()),
        });
        let text = parts[0]["text"].as_str().expect("首段必须是文本");
        assert!(text.starts_with("修复首轮会话"));
        assert!(text.contains("src/runtime.rs"));
        assert!(text.contains("diff --git"));
        assert!(text.contains("只改指定范围"));
    }

    #[test]
    fn stop_forces_kill_when_global_dispose_fails() {
        use crate::process::testchild::TestChild;

        let server = spawn_server(PASSWORD, |method, path| match (method, path) {
            ("GET", "/global/health") => (
                200,
                json!({"healthy": true, "version": OPENCODE_MIN_SUPPORTED_VERSION}).to_string(),
            ),
            ("POST", "/global/dispose") => (500, json!({"error": "dispose_failed"}).to_string()),
            _ => (404, json!({"error": "not_found"}).to_string()),
        });
        let child = TestChild::new();
        let (tx, _rx) = unbounded();
        let handle = connect(
            server.port,
            PASSWORD.to_string(),
            Some(Box::new(child.clone())),
            tx,
            short_opts(),
        )
        .expect("应就绪");
        assert_eq!(handle.stop(Duration::from_millis(150)), StopOutcome::Forced);
        assert!(
            child.killed.load(Ordering::SeqCst),
            "dispose 失败后必须回收子进程"
        );
    }

    #[test]
    fn successful_global_dispose_reclaims_child_as_graceful() {
        use crate::process::testchild::TestChild;

        let server = spawn_server(PASSWORD, healthy_handler);
        let child = TestChild::new();
        let (tx, _rx) = unbounded();
        let handle = connect(
            server.port,
            PASSWORD.to_string(),
            Some(Box::new(child.clone())),
            tx,
            short_opts(),
        )
        .expect("应就绪");

        assert_eq!(
            handle.stop(Duration::from_millis(150)),
            StopOutcome::Graceful
        );
        assert!(child.killed.load(Ordering::SeqCst));
    }

    #[test]
    fn debug_and_errors_never_leak_connection_credentials() {
        let server = spawn_server(PASSWORD, healthy_handler);
        let (tx, _rx) = unbounded();
        let handle =
            connect(server.port, PASSWORD.to_string(), None, tx, short_opts()).expect("应就绪");
        let debug = format!("{handle:?}");
        assert!(!debug.contains(PASSWORD));
        assert!(!debug.contains(&server.port.to_string()));

        let error = handle
            .shared
            .request("GET", "/not-found", Duration::from_millis(300))
            .expect_err("404 应报错");
        let message = format!("{error}");
        assert!(!message.contains(PASSWORD));
        assert!(!message.contains(&server.port.to_string()));
        assert!(!message.contains("Authorization"));
    }

    #[test]
    fn isolated_state_overrides_global_opencode_locations() {
        let mut env = HashMap::from([
            (
                "USERPROFILE".to_string(),
                "C:\\Users\\developer".to_string(),
            ),
            ("PATH".to_string(), "C:\\Windows".to_string()),
        ]);
        let runtime_dir =
            isolated_opencode_state(&mut env).expect("OpenCode 启动必须能创建隔离状态目录");

        for (variable, _) in OPENCODE_ISOLATED_STATE_DIRS {
            let path = std::path::PathBuf::from(
                env.get(variable)
                    .expect("隔离环境必须覆盖每个 OpenCode 状态根"),
            );
            assert!(path.starts_with(runtime_dir.path()));
            assert!(path.is_dir());
        }
        assert_eq!(env["USERPROFILE"], "C:\\Users\\developer");
    }

    #[test]
    fn compatibility_profile_accepts_stable_supported_opencode_1x_versions() {
        for version in ["1.18.5", "1.18.6", "1.25.0", "1.99.42"] {
            assert!(
                OpenCodeRuntime::is_compatible_version(version),
                "{version} 应通过 OpenCode 1.x 兼容性档案"
            );
        }
    }

    #[test]
    fn compatibility_profile_rejects_versions_outside_the_known_stable_1x_range() {
        for version in [
            "1.18.4",
            "1.18.5-beta.1",
            "2.0.0",
            "v1.18.5",
            "1.18",
            "unknown",
        ] {
            assert!(
                !OpenCodeRuntime::is_compatible_version(version),
                "{version} 不得被当作已知兼容档案"
            );
        }
    }

    #[test]
    fn ready_line_requires_the_expected_loopback_listener() {
        assert_eq!(
            listening_url("opencode server listening on http://127.0.0.1:43123"),
            Some("http://127.0.0.1:43123")
        );
        assert_eq!(
            listening_url("opencode server listening on http://localhost:43123"),
            Some("http://localhost:43123")
        );
        assert_eq!(listening_url("unrelated output"), None);
    }

    #[test]
    fn mismatched_ready_line_fails_before_health_authentication() {
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        ready_tx.send(ReadyLine::AddressMismatch).unwrap();
        let (tx, rx) = unbounded();

        let error = connect_after_ready_line(
            43123,
            PASSWORD.to_string(),
            None,
            tx,
            short_opts(),
            Some(ready_rx),
            None,
            String::new(),
        )
        .expect_err("监听地址不一致时不得发送健康检查");
        assert!(matches!(error, RuntimeError::NotReady(_)));
        wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
    }
}
