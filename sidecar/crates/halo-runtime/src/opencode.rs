//! OpenCode 回环服务适配器：锁定版本、回环端口、每次启动新 token、
//! 健康检查 + 精确版本握手、HTTP 任务与事件长轮询。
//! 端口与 token 仅存于句柄私有字段，不出现在任何公开 getter、Debug 或错误 message 中。

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use rand::RngCore;
use serde_json::Value;

use crate::process::{probe_version, wait_exit, ChildProcess, RealChild};
use crate::{
    lock, map_trace_event, LaunchCmd, RunTaskSpec, RuntimeError, RuntimeEvent, RuntimeState,
    StopOutcome, Timeouts,
};

pub const OPENCODE_LOCKED_VERSION: &str = "0.4.2";

pub struct OpenCodeRuntime;

impl OpenCodeRuntime {
    /// 探测 `<exe> --version`，返回首行中的 semver。
    pub fn probe(exe: &str) -> Result<String, RuntimeError> {
        probe_version(exe, "OpenCode")
    }

    /// 启动 `<exe> serve --hostname 127.0.0.1 --port <p>`，完成健康检查与精确版本握手。
    pub fn start(
        cmd: LaunchCmd,
        tx: Sender<RuntimeEvent>,
        opts: Timeouts,
    ) -> Result<OpenCodeHandle, RuntimeError> {
        let port = pick_free_port()?;
        let token = random_hex_token();

        let mut command = Command::new(&cmd.exe);
        command
            .arg("serve")
            .arg("--hostname")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .args(&cmd.args);
        // 子进程环境 = halo-config 构好的白名单环境 + 本次启动的新认证信息
        command.env_clear();
        command.envs(&cmd.env);
        command.env("HALO_OC_TOKEN", &token);
        if !cmd.cwd.is_empty() {
            command.current_dir(&cmd.cwd);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command
            .spawn()
            .map_err(|e| RuntimeError::Spawn(format!("无法启动 OpenCode 进程：{e}")))?;

        connect(
            port,
            token,
            Some(Box::new(RealChild::new(child))),
            tx,
            opts,
        )
    }
}

/// 绑定 127.0.0.1:0 取得空闲回环端口后立即释放。
fn pick_free_port() -> Result<u16, RuntimeError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| RuntimeError::Spawn(format!("无法申请空闲回环端口：{e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| RuntimeError::Spawn(format!("无法读取回环端口号：{e}")))?
        .port();
    drop(listener);
    Ok(port)
}

/// 32 字节随机 hex token（64 个 hex 字符），每次启动全新生成。
fn random_hex_token() -> String {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut s = String::with_capacity(64);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

struct OcShared {
    state: Mutex<RuntimeState>,
    tx: Sender<RuntimeEvent>,
    agent: ureq::Agent,
    // 端口与 token 为私有字段；错误 message 与 Debug 输出中一律不出现
    port: u16,
    token: String,
    child: Mutex<Option<Box<dyn ChildProcess>>>,
    /// 事件长轮询游标（已消费事件数）
    events_cursor: AtomicU64,
}

impl OcShared {
    fn set_state(&self, s: RuntimeState) {
        *lock(&self.state) = s.clone();
        let _ = self.tx.send(RuntimeEvent::State(s));
    }

    fn set_failed(&self, reason: &str, recovery_hint: &str) {
        self.set_state(RuntimeState::Failed {
            reason: reason.to_string(),
            recovery_hint: recovery_hint.to_string(),
        });
    }

    fn is_shutting_down(&self) -> bool {
        matches!(
            *lock(&self.state),
            RuntimeState::Stopping | RuntimeState::Stopped
        )
    }

    fn kill_child(&self) {
        if let Some(mut child) = lock(&self.child).take() {
            child.kill();
        }
    }

    /// 统一请求入口。错误 message 不包含 URL/端口/token。
    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, RuntimeError> {
        let url = format!("http://127.0.0.1:{}{}", self.port, path);
        let req = match method {
            "GET" => self.agent.get(&url),
            _ => self.agent.post(&url),
        }
        .set("Authorization", &format!("Bearer {}", self.token))
        .timeout(timeout);
        let resp = match body {
            Some(b) => req.send_json(b),
            None => req.call(),
        };
        match resp {
            Ok(r) => Ok(r.into_json::<Value>().unwrap_or(Value::Null)),
            Err(ureq::Error::Status(401, _)) => Err(RuntimeError::Unauthorized),
            Err(ureq::Error::Status(code, _)) => {
                Err(RuntimeError::Io(format!("OpenCode 返回了 HTTP {code}")))
            }
            Err(ureq::Error::Transport(_)) => {
                Err(RuntimeError::Io("与 OpenCode 的本地连接失败".to_string()))
            }
        }
    }
}

/// 就绪流程（生产与测试共用）：健康轮询 → 精确版本握手 → Ready。
fn connect(
    port: u16,
    token: String,
    child: Option<Box<dyn ChildProcess>>,
    tx: Sender<RuntimeEvent>,
    opts: Timeouts,
) -> Result<OpenCodeHandle, RuntimeError> {
    let shared = Arc::new(OcShared {
        state: Mutex::new(RuntimeState::Starting),
        tx,
        agent: ureq::agent(),
        port,
        token,
        child: Mutex::new(child),
        events_cursor: AtomicU64::new(0),
    });
    let _ = shared.tx.send(RuntimeEvent::State(RuntimeState::Starting));

    // 健康检查：轮询 GET /health 直到 200 {"status":"ok"}；401 立即失败关闭
    let deadline = Instant::now() + opts.ready;
    loop {
        match shared.request("GET", "/health", None, Duration::from_millis(500)) {
            Ok(v) if v.get("status").and_then(Value::as_str) == Some("ok") => break,
            Ok(_) => {}
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

    // 精确版本握手：必须与锁定版本完全相等
    let version = match shared.request("GET", "/version", None, Duration::from_secs(2)) {
        Ok(v) => v
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        Err(e) => {
            shared.set_failed(
                "无法获取 OpenCode 版本信息",
                "请查看 OpenCode 日志后重新启动运行时",
            );
            shared.kill_child();
            return Err(e);
        }
    };
    if version != OPENCODE_LOCKED_VERSION {
        let reason = format!(
            "OpenCode 版本不匹配（RUNTIME_VERSION_MISMATCH）：检测到 {version}，要求 {OPENCODE_LOCKED_VERSION}"
        );
        shared.set_failed(&reason, "请安装锁定版本的 OpenCode 后重新启动运行时");
        shared.kill_child();
        return Err(RuntimeError::VersionMismatch(format!(
            "检测到 {version}，要求 {OPENCODE_LOCKED_VERSION}"
        )));
    }

    shared.set_state(RuntimeState::Ready);
    Ok(OpenCodeHandle { shared })
}

/// 事件长轮询线程：GET /events?after=n → 规范化事件流 → done 时发 TaskDone。
fn poll_events(shared: &Arc<OcShared>) {
    loop {
        if shared.is_shutting_down() {
            return;
        }
        let after = shared.events_cursor.load(Ordering::SeqCst);
        let path = format!("/events?after={after}");
        match shared.request("GET", &path, None, Duration::from_secs(30)) {
            Ok(v) => {
                let events = v
                    .get("events")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for ev in &events {
                    let _ = shared.tx.send(map_trace_event(ev));
                }
                shared
                    .events_cursor
                    .fetch_add(events.len() as u64, Ordering::SeqCst);
                if v.get("done").and_then(Value::as_bool) == Some(true) {
                    let outcome = v
                        .get("outcome")
                        .and_then(Value::as_str)
                        .unwrap_or("failed")
                        .to_string();
                    let summary = v
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let _ = shared.tx.send(RuntimeEvent::TaskDone { outcome, summary });
                    return;
                }
                if events.is_empty() {
                    // 空轮询让步，避免对端立即返回空集时忙等
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(_) => {
                if shared.is_shutting_down() {
                    return;
                }
                shared.set_failed(
                    "OpenCode 事件流中断",
                    "OpenCode 服务可能已退出；请重新启动运行时后重试任务",
                );
                return;
            }
        }
    }
}

pub struct OpenCodeHandle {
    shared: Arc<OcShared>,
}

// 手写 Debug：端口与 token 绝不出现
impl std::fmt::Debug for OpenCodeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenCodeHandle")
            .field("state", &*lock(&self.shared.state))
            .finish_non_exhaustive()
    }
}

impl OpenCodeHandle {
    /// POST /task 提交任务，并启动事件长轮询线程；过程与终局经事件通道送出。
    pub fn run_task(&self, spec: &RunTaskSpec) -> Result<(), RuntimeError> {
        if *lock(&self.shared.state) != RuntimeState::Ready {
            return Err(RuntimeError::InvalidState);
        }
        let body = serde_json::to_value(spec)
            .map_err(|e| RuntimeError::Io(format!("任务参数序列化失败：{e}")))?;
        self.shared
            .request("POST", "/task", Some(&body), Duration::from_secs(10))?;
        self.shared.events_cursor.store(0, Ordering::SeqCst);
        let shared = Arc::clone(&self.shared);
        std::thread::spawn(move || poll_events(&shared));
        Ok(())
    }

    /// 经原生通道请求取消（POST /cancel，尽力而为）。
    pub fn cancel_native(&self) {
        let _ = self
            .shared
            .request("POST", "/cancel", None, Duration::from_secs(5));
    }

    /// 优雅停止：POST /shutdown，等 grace；子进程未退出则强杀 → Forced。
    pub fn stop(&self, grace: Duration) -> StopOutcome {
        if matches!(*lock(&self.shared.state), RuntimeState::Stopped) {
            return StopOutcome::Graceful;
        }
        self.shared.set_state(RuntimeState::Stopping);
        let shutdown_ok = self
            .shared
            .request("POST", "/shutdown", None, Duration::from_secs(2))
            .is_ok();
        let outcome = {
            let mut guard = lock(&self.shared.child);
            match guard.as_mut() {
                Some(child) => {
                    if wait_exit(child.as_mut(), grace) {
                        StopOutcome::Graceful
                    } else {
                        child.kill();
                        StopOutcome::Forced
                    }
                }
                // 无受监督子进程（测试注入场景）：以停止请求是否送达判定
                None => {
                    if shutdown_ok {
                        StopOutcome::Graceful
                    } else {
                        StopOutcome::Forced
                    }
                }
            }
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
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicBool;

    fn short_opts() -> Timeouts {
        Timeouts {
            ready: Duration::from_millis(800),
            cancel_grace: Duration::from_secs(2),
            shutdown_grace: Duration::from_secs(2),
        }
    }

    /// tiny_http 临时假服务：校验 Bearer token，按闭包应答。
    struct TestServer {
        port: u16,
        seen: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
        join: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    fn spawn_server<F>(expected_token: &str, handler: F) -> TestServer
    where
        F: Fn(&str, &str) -> (u16, String) + Send + 'static,
    {
        let server = tiny_http::Server::http(("127.0.0.1", 0)).expect("无法启动假服务");
        let port = match server.server_addr() {
            tiny_http::ListenAddr::IP(addr) => addr.port(),
            #[allow(unreachable_patterns)]
            _ => panic!("假服务必须绑定 IP"),
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let expected = format!("Bearer {expected_token}");
        let seen_bg = Arc::clone(&seen);
        let stop_bg = Arc::clone(&stop);
        let join = std::thread::spawn(move || {
            while !stop_bg.load(Ordering::SeqCst) {
                let req = match server.recv_timeout(Duration::from_millis(50)) {
                    Ok(Some(r)) => r,
                    _ => continue,
                };
                let method = req.method().as_str().to_string();
                let url = req.url().to_string();
                let auth = req
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.as_str().to_string());
                seen_bg.lock().unwrap().push(format!("{method} {url}"));
                let (code, body) = if auth.as_deref() != Some(expected.as_str()) {
                    (401, "{\"error\":\"unauthorized\"}".to_string())
                } else {
                    handler(&method, &url)
                };
                let header =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                        .unwrap();
                let _ = req.respond(
                    tiny_http::Response::from_string(body)
                        .with_status_code(code)
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

    fn happy_handler(method: &str, url: &str) -> (u16, String) {
        match (method, url) {
            ("GET", "/health") => (200, json!({"status": "ok"}).to_string()),
            ("GET", "/version") => (200, json!({"version": OPENCODE_LOCKED_VERSION}).to_string()),
            ("POST", "/task") => (200, json!({"accepted": true}).to_string()),
            ("POST", "/cancel") => (200, json!({"cancelled": true}).to_string()),
            ("POST", "/shutdown") => (200, json!({"stopping": true}).to_string()),
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
            let ev = rx.recv_timeout(remaining).expect("事件通道超时或断开");
            if pred(&ev) {
                return ev;
            }
        }
    }

    const TOKEN: &str = "aa11bb22cc33dd44ee55ff66aa77bb88cc99dd00ee11ff22aa33bb44cc55dd66";

    #[test]
    fn start_ready_when_health_and_version_ok() {
        let server = spawn_server(TOKEN, happy_handler);
        let (tx, rx) = unbounded();
        let handle = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect("健康 + 版本一致应就绪");
        assert_eq!(handle.state(), RuntimeState::Ready);
        wait_event(&rx, |e| matches!(e, RuntimeEvent::State(RuntimeState::Ready)));
        let seen = server.seen.lock().unwrap().clone();
        assert!(seen.iter().any(|s| s == "GET /health"));
        assert!(seen.iter().any(|s| s == "GET /version"));
    }

    #[test]
    fn health_failure_times_out_as_failed() {
        let server = spawn_server(TOKEN, |method, url| match (method, url) {
            ("GET", "/health") => (500, "{\"status\":\"error\"}".to_string()),
            _ => (404, "{}".to_string()),
        });
        let (tx, _rx) = unbounded();
        let err = connect(server.port, TOKEN.to_string(), None, tx.clone(), short_opts())
            .expect_err("健康检查失败应报错");
        assert!(matches!(err, RuntimeError::NotReady(_)));
        assert!(format!("{err}").contains("健康检查"));
    }

    #[test]
    fn version_mismatch_fails_with_marker() {
        let server = spawn_server(TOKEN, |method, url| match (method, url) {
            ("GET", "/health") => (200, json!({"status": "ok"}).to_string()),
            ("GET", "/version") => (200, json!({"version": "0.9.9"}).to_string()),
            _ => (404, "{}".to_string()),
        });
        let (tx, rx) = unbounded();
        let err = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect_err("版本不一致必须失败");
        assert!(matches!(err, RuntimeError::VersionMismatch(_)));
        assert!(format!("{err}").contains("RUNTIME_VERSION_MISMATCH"));
        let ev = wait_event(&rx, |e| {
            matches!(e, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
        match ev {
            RuntimeEvent::State(RuntimeState::Failed { reason, recovery_hint }) => {
                assert!(reason.contains("RUNTIME_VERSION_MISMATCH"), "reason={reason}");
                assert!(reason.contains("0.9.9"));
                assert!(!recovery_hint.is_empty());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn unauthorized_fails_fast() {
        let server = spawn_server("expected-token-that-wont-match", happy_handler);
        let (tx, rx) = unbounded();
        let started = Instant::now();
        let err = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect_err("401 必须失败关闭");
        assert!(matches!(err, RuntimeError::Unauthorized));
        // 401 应立即失败，而不是轮询到就绪超时
        assert!(started.elapsed() < Duration::from_millis(700));
        wait_event(&rx, |e| {
            matches!(e, RuntimeEvent::State(RuntimeState::Failed { .. }))
        });
    }

    #[test]
    fn run_task_streams_events_then_done() {
        let scripted: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::from([
            json!({
                "events": [
                    {"kind": "phase", "text": "planning", "detail": {}},
                    {"kind": "verification", "text": "", "detail": {"status": "passed", "detail": "验证通过"}}
                ],
                "done": false
            })
            .to_string(),
            json!({"events": [], "done": true, "outcome": "finished", "summary": "任务完成"}).to_string(),
        ])));
        let scripted_bg = Arc::clone(&scripted);
        let server = spawn_server(TOKEN, move |method, url| {
            if method == "GET" && url.starts_with("/events") {
                let body = scripted_bg
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| json!({"events": [], "done": true, "outcome": "finished", "summary": ""}).to_string());
                return (200, body);
            }
            happy_handler(method, url)
        });
        let (tx, rx) = unbounded();
        let handle = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect("应就绪");
        handle
            .run_task(&RunTaskSpec {
                instructions: "实现功能 X".into(),
                files: vec!["src/x.rs".into()],
                base_diff: None,
                notes: None,
            })
            .expect("run_task 应成功");

        let ev = wait_event(&rx, |e| matches!(e, RuntimeEvent::Trace(_)));
        match ev {
            RuntimeEvent::Trace(item) => assert_eq!(item.kind, "phase"),
            _ => unreachable!(),
        }
        let ev = wait_event(&rx, |e| matches!(e, RuntimeEvent::Verification { .. }));
        match ev {
            RuntimeEvent::Verification { status, detail } => {
                assert_eq!(status, "passed");
                assert_eq!(detail, "验证通过");
            }
            _ => unreachable!(),
        }
        let ev = wait_event(&rx, |e| matches!(e, RuntimeEvent::TaskDone { .. }));
        match ev {
            RuntimeEvent::TaskDone { outcome, summary } => {
                assert_eq!(outcome, "finished");
                assert_eq!(summary, "任务完成");
            }
            _ => unreachable!(),
        }
        // 游标推进：第二次长轮询必须携带 after=2
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let seen = server.seen.lock().unwrap().clone();
            if seen.iter().any(|s| s == "GET /events?after=0")
                && seen.iter().any(|s| s == "GET /events?after=2")
            {
                break;
            }
            assert!(Instant::now() < deadline, "事件游标未按已消费数推进：{seen:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(server.seen.lock().unwrap().iter().any(|s| s == "POST /task"));
    }

    #[test]
    fn cancel_native_posts_cancel() {
        let server = spawn_server(TOKEN, happy_handler);
        let (tx, _rx) = unbounded();
        let handle = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect("应就绪");
        handle.cancel_native();
        assert!(server.seen.lock().unwrap().iter().any(|s| s == "POST /cancel"));
    }

    #[test]
    fn stop_posts_shutdown_gracefully() {
        let server = spawn_server(TOKEN, happy_handler);
        let (tx, _rx) = unbounded();
        let handle = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect("应就绪");
        let outcome = handle.stop(Duration::from_millis(500));
        assert_eq!(outcome, StopOutcome::Graceful);
        assert_eq!(handle.state(), RuntimeState::Stopped);
        assert!(
            server.seen.lock().unwrap().iter().any(|s| s == "POST /shutdown"),
            "停止必须先走优雅停止请求路径"
        );
    }

    #[test]
    fn stop_forces_kill_when_child_hangs() {
        use crate::process::testchild::TestChild;
        let server = spawn_server(TOKEN, |method, url| match (method, url) {
            ("GET", "/health") => (200, json!({"status": "ok"}).to_string()),
            ("GET", "/version") => (200, json!({"version": OPENCODE_LOCKED_VERSION}).to_string()),
            // 模拟挂死：接受 shutdown 请求但进程永不退出
            ("POST", "/shutdown") => (200, "{}".to_string()),
            _ => (404, "{}".to_string()),
        });
        let child = TestChild::new();
        let (tx, _rx) = unbounded();
        let handle = connect(
            server.port,
            TOKEN.to_string(),
            Some(Box::new(child.clone())),
            tx,
            short_opts(),
        )
        .expect("应就绪");
        let outcome = handle.stop(Duration::from_millis(150));
        assert_eq!(outcome, StopOutcome::Forced);
        assert!(child.killed.load(Ordering::SeqCst), "超时后必须强杀");
    }

    #[test]
    fn debug_and_errors_never_leak_port_or_token() {
        let server = spawn_server(TOKEN, happy_handler);
        let (tx, _rx) = unbounded();
        let handle = connect(server.port, TOKEN.to_string(), None, tx, short_opts())
            .expect("应就绪");
        let dbg = format!("{handle:?}");
        assert!(!dbg.contains(TOKEN), "Debug 不得包含 token");
        assert!(!dbg.contains(&server.port.to_string()), "Debug 不得包含端口");

        // 错误 message 同样不得携带端口/token（用一个必然失败的请求验证）
        let err = handle
            .shared
            .request("GET", "/no-such-path", None, Duration::from_millis(300))
            .expect_err("404 应报错");
        let msg = format!("{err}");
        assert!(!msg.contains(TOKEN));
        assert!(!msg.contains(&server.port.to_string()));
    }

    #[test]
    fn locked_version_constant_is_exact() {
        assert_eq!(OPENCODE_LOCKED_VERSION, "0.4.2");
    }
}
