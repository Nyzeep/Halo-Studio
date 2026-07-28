//! OpenCode 1.x 受管回环服务适配器。
//!
//! 仅接受经过验证的稳定 1.x 兼容性档案：服务绑定回环地址，每次启动生成
//! 新的 Basic 认证密码，并以 OpenCode Server 的 `/global/health` 完成真实就绪检查。
//! 端口、认证密码和 Authorization 值只保留在私有句柄中，不进入 Debug、错误或事件。

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use rand::RngCore;
use serde_json::Value;
use tempfile::TempDir;

use crate::process::{probe_version, ChildProcess, RealChild};
use crate::{lock, LaunchCmd, RunTaskSpec, RuntimeError, RuntimeEvent, RuntimeState, StopOutcome, Timeouts};

/// 已验证的 OpenCode Server 兼容性档案标识。新主版本须建立新档案后才能启动。
pub const OPENCODE_COMPATIBILITY_PROFILE: &str = "opencode-server-1.x";
/// 当前档案支持的最低稳定版 OpenCode。
pub const OPENCODE_MIN_SUPPORTED_VERSION: &str = "1.18.5";

const OPENCODE_DEFAULT_USERNAME: &str = "opencode";
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
        version.major == 1
            && (version.minor > 18 || (version.minor == 18 && version.patch >= 5))
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
        let LaunchCmd {
            exe,
            mut env,
            cwd,
        } = cmd;
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
        std::fs::create_dir_all(&path).map_err(|_| {
            RuntimeError::Spawn("无法创建 OpenCode 隔离运行时目录".to_string())
        })?;
        env.insert(variable.to_string(), path.to_string_lossy().into_owned());
    }

    Ok(runtime_dir)
}

fn basic_authorization(password: &str) -> String {
    let credentials = format!("{OPENCODE_DEFAULT_USERNAME}:{password}");
    format!("Basic {}", STANDARD.encode(credentials))
}

struct OcShared {
    state: Mutex<RuntimeState>,
    tx: Sender<RuntimeEvent>,
    agent: ureq::Agent,
    // 连接细节及密码只存在私有字段；任何可观察文本均不得包含它们。
    port: u16,
    password: String,
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
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let request = match method {
            "GET" => self.agent.get(&url),
            "POST" => self.agent.post(&url),
            _ => return Err(RuntimeError::Io("不支持的 OpenCode 请求方法".to_string())),
        }
        .set("Authorization", &basic_authorization(&self.password))
        .timeout(timeout);

        let response = request.call();
        match response {
            Ok(response) => Ok(response.into_json::<Value>().unwrap_or(Value::Null)),
            Err(ureq::Error::Status(401, _)) => Err(RuntimeError::Unauthorized),
            Err(ureq::Error::Status(code, _)) => {
                Err(RuntimeError::Io(format!("OpenCode 返回了 HTTP {code}")))
            }
            Err(ureq::Error::Transport(_)) => {
                Err(RuntimeError::Io("与 OpenCode 的本地服务连接失败".to_string()))
            }
        }
    }
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
    connect_after_ready_line(port, password, child, tx, opts, None, None)
}

fn connect_after_ready_line(
    port: u16,
    password: String,
    child: Option<Box<dyn ChildProcess>>,
    tx: Sender<RuntimeEvent>,
    opts: Timeouts,
    ready_line: Option<Receiver<ReadyLine>>,
    runtime_dir: Option<TempDir>,
) -> Result<OpenCodeHandle, RuntimeError> {
    let shared = Arc::new(OcShared {
        state: Mutex::new(RuntimeState::Starting),
        tx,
        agent: ureq::agent(),
        port,
        password,
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
                shared.set_failed(
                    reason,
                    "请停止占用回环端口的其他程序后重新启动 OpenCode",
                );
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

    /// 本票仅实现启动兼容性档案。真实 session/message/event 协议由下一张票接入，
    /// 不能退回旧的假设 `/task` 协议来伪造成功。
    pub fn run_task(&self, _spec: &RunTaskSpec) -> Result<(), RuntimeError> {
        if *lock(&self.shared.state) != RuntimeState::Ready {
            return Err(RuntimeError::InvalidState);
        }
        Err(RuntimeError::CapabilityUnavailable(
            "OpenCode 真实会话尚未接入；请先确认运行时已就绪，并等待受管会话票实现".to_string(),
        ))
    }

    /// 尚无 OpenCode session 时不存在原生取消请求；不能调用旧 `/cancel` 协议。
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
                let header = tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"application/json"[..],
                )
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

    fn wait_event(rx: &Receiver<RuntimeEvent>, pred: impl Fn(&RuntimeEvent) -> bool) -> RuntimeEvent {
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
        wait_event(&rx, |event| matches!(event, RuntimeEvent::State(RuntimeState::Ready)));
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
            ("GET", "/global/health") => (200, json!({"healthy": true, "version": "1.18.4"}).to_string()),
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
    fn legacy_task_protocol_is_not_a_production_fallback() {
        let server = spawn_server(PASSWORD, healthy_handler);
        let (tx, _rx) = unbounded();
        let handle = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect("应就绪");
        let error = handle
            .run_task(&RunTaskSpec {
                instructions: "实现功能 X".into(),
                files: vec![],
                base_diff: None,
                notes: None,
            })
            .expect_err("会话能力未接入时必须明确拒绝");
        assert!(matches!(error, RuntimeError::CapabilityUnavailable(_)));
        handle.cancel_native();
        assert_eq!(handle.stop(Duration::from_millis(100)), StopOutcome::Graceful);
        let seen = server.seen.lock().unwrap().clone();
        for legacy_path in ["/task", "/events", "/cancel", "/shutdown", "/health", "/version"] {
            assert!(
                !seen.iter().any(|entry| entry.contains(legacy_path)),
                "不得请求旧假设协议端点 {legacy_path}：{seen:?}"
            );
        }
        assert!(seen.iter().any(|entry| entry == "POST /global/dispose"));
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
        assert!(child.killed.load(Ordering::SeqCst), "dispose 失败后必须回收子进程");
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

        assert_eq!(handle.stop(Duration::from_millis(150)), StopOutcome::Graceful);
        assert!(child.killed.load(Ordering::SeqCst));
    }

    #[test]
    fn debug_and_errors_never_leak_connection_credentials() {
        let server = spawn_server(PASSWORD, healthy_handler);
        let (tx, _rx) = unbounded();
        let handle = connect(server.port, PASSWORD.to_string(), None, tx, short_opts())
            .expect("应就绪");
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
            ("USERPROFILE".to_string(), "C:\\Users\\developer".to_string()),
            ("PATH".to_string(), "C:\\Windows".to_string()),
        ]);
        let runtime_dir = isolated_opencode_state(&mut env)
            .expect("OpenCode 启动必须能创建隔离状态目录");

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
        for version in ["1.18.4", "1.18.5-beta.1", "2.0.0", "v1.18.5", "1.18", "unknown"] {
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
        )
        .expect_err("监听地址不一致时不得发送健康检查");
        assert!(matches!(error, RuntimeError::NotReady(_)));
        wait_event(&rx, |event| {
            matches!(event, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
    }
}
