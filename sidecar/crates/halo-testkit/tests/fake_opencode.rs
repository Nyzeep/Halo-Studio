//! fake-opencode 的独立契约测试：只模拟 OpenCode 1.x Server 的真实启动边界。

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;
use ureq::{Agent, Response};

const PASSWORD: &str = "fake-opencode-test-password";

struct OcProc {
    child: Child,
    port: u16,
}

impl OcProc {
    fn try_running(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    fn kill_now(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OcProc {
    fn drop(&mut self) {
        self.kill_now();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("无法申请测试端口");
    listener.local_addr().expect("无法读取测试端口").port()
}

fn spawn_oc(mode: &str, dir: &std::path::Path) -> OcProc {
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_fake-opencode"))
        .args([
            "serve",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .env("FAKE_OC_MODE", mode)
        .env("OPENCODE_SERVER_PASSWORD", PASSWORD)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 fake-opencode 失败");
    let mut oc = OcProc { child, port };
    wait_until_server_accepts_requests(&mut oc);
    oc
}

fn agent() -> Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(2))
        .build()
}

fn basic_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

fn get_with_password(port: u16, path: &str, password: &str) -> Result<Response, ureq::Error> {
    agent()
        .get(&format!("http://127.0.0.1:{port}{path}"))
        .set("Authorization", &basic_header("opencode", password))
        .call()
}

fn post_with_password(port: u16, path: &str, password: &str) -> Result<Response, ureq::Error> {
    agent()
        .post(&format!("http://127.0.0.1:{port}{path}"))
        .set("Authorization", &basic_header("opencode", password))
        .call()
}

fn post_with_password_timeout(
    port: u16,
    path: &str,
    password: &str,
    timeout: Duration,
) -> Result<Response, ureq::Error> {
    ureq::AgentBuilder::new()
        .timeout(timeout)
        .build()
        .post(&format!("http://127.0.0.1:{port}{path}"))
        .set("Authorization", &basic_header("opencode", password))
        .call()
}

/// 子进程与首个 HTTP 请求之间存在正常的启动竞争。用真实健康端点重试，
/// 并把任意 HTTP 状态都当作“已开始监听”，以便 bad_auth 也能被测试。
fn wait_until_server_accepts_requests(oc: &mut OcProc) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match get_with_password(oc.port, "/global/health", PASSWORD) {
            Ok(_) | Err(ureq::Error::Status(_, _)) => return,
            Err(ureq::Error::Transport(_)) => {
                assert!(oc.try_running(), "fake-opencode 在监听前异常退出");
                assert!(Instant::now() < deadline, "等待 fake-opencode 监听超时");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

fn status_of(result: Result<Response, ureq::Error>) -> u16 {
    match result {
        Ok(response) => response.status(),
        Err(ureq::Error::Status(status, _)) => status,
        Err(_) => panic!("请求失败"),
    }
}

fn body_of(result: Result<Response, ureq::Error>) -> Value {
    match result {
        Ok(response) => response.into_json().expect("响应应为 JSON"),
        Err(_) => panic!("请求失败"),
    }
}

#[test]
fn serves_authenticated_opencode_1x_health_and_global_dispose_contract() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("happy", dir.path());

    let health = body_of(get_with_password(oc.port, "/global/health", PASSWORD));
    assert_eq!(health["healthy"], true);
    assert_eq!(health["version"], halo_testkit::OPENCODE_VERSION);

    assert_eq!(
        status_of(post_with_password(oc.port, "/global/dispose", PASSWORD)),
        200
    );
    assert!(
        oc.try_running(),
        "dispose 仅释放服务资源，不应替监督者退出进程"
    );
    oc.kill_now();
}

#[test]
fn rejects_wrong_or_missing_basic_authentication() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("happy", dir.path());

    assert_eq!(
        status_of(get_with_password(
            oc.port,
            "/global/health",
            "different-test-password"
        )),
        401
    );
    let wrong_username = agent()
        .get(&format!("http://127.0.0.1:{}/global/health", oc.port))
        .set("Authorization", &basic_header("not-opencode", PASSWORD))
        .call();
    assert_eq!(status_of(wrong_username), 401);
    let missing = agent()
        .get(&format!("http://127.0.0.1:{}/global/health", oc.port))
        .call();
    assert_eq!(status_of(missing), 401);
    assert_eq!(
        status_of(get_with_password(oc.port, "/global/health", PASSWORD)),
        200
    );
    assert!(oc.try_running());
}

#[test]
fn exposes_profile_failure_modes_without_legacy_endpoints() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    for (mode, expected) in [
        ("old_version", Value::String("1.18.4".to_string())),
        ("newer_1x", Value::String("1.19.0".to_string())),
        ("major_version", Value::String("2.0.0".to_string())),
        ("malformed_version", Value::String("1.18".to_string())),
        (
            "pre_release_version",
            Value::String("1.18.5-beta.1".to_string()),
        ),
    ] {
        let mut oc = spawn_oc(mode, dir.path());
        let health = body_of(get_with_password(oc.port, "/global/health", PASSWORD));
        assert_eq!(health["healthy"], true);
        assert_eq!(health["version"], expected, "mode={mode}");
        assert_eq!(
            status_of(post_with_password(oc.port, "/global/dispose", PASSWORD)),
            200
        );
        assert!(oc.try_running(), "成功 dispose 后进程仍应由监督者回收");
        oc.kill_now();
    }

    let mut missing_version = spawn_oc("missing_health_version", dir.path());
    let health = body_of(get_with_password(
        missing_version.port,
        "/global/health",
        PASSWORD,
    ));
    assert_eq!(health["healthy"], true);
    assert!(health.get("version").is_none());
    assert_eq!(
        status_of(post_with_password(
            missing_version.port,
            "/global/dispose",
            PASSWORD
        )),
        200
    );
    assert!(
        missing_version.try_running(),
        "成功 dispose 后进程仍应由监督者回收"
    );
    missing_version.kill_now();

    let mut oc = spawn_oc("happy", dir.path());
    for path in ["/health", "/version", "/events"] {
        assert_eq!(status_of(get_with_password(oc.port, path, PASSWORD)), 404);
    }
    for path in ["/task", "/cancel", "/shutdown", "/instance/dispose"] {
        assert_eq!(status_of(post_with_password(oc.port, path, PASSWORD)), 404);
    }
    assert_eq!(
        status_of(post_with_password(oc.port, "/global/dispose", PASSWORD)),
        200
    );
    assert!(oc.try_running(), "成功 dispose 后进程仍应由监督者回收");
    oc.kill_now();
}

#[test]
fn unhealthy_and_bad_auth_modes_fail_closed() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut unhealthy = spawn_oc("unhealthy", dir.path());
    let health = body_of(get_with_password(
        unhealthy.port,
        "/global/health",
        PASSWORD,
    ));
    assert_eq!(health["healthy"], false);
    unhealthy.kill_now();

    let mut bad_auth = spawn_oc("bad_auth", dir.path());
    assert_eq!(
        status_of(get_with_password(bad_auth.port, "/global/health", PASSWORD)),
        401
    );
    bad_auth.kill_now();
}

#[test]
fn dispose_failure_and_timeout_require_force_kill() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut failed = spawn_oc("dispose_failure", dir.path());

    assert_eq!(
        status_of(post_with_password(failed.port, "/global/dispose", PASSWORD)),
        500
    );
    assert!(failed.try_running(), "dispose 失败后进程必须由监督者强杀");
    failed.kill_now();

    let mut hanging = spawn_oc("hang_on_dispose", dir.path());
    let result = post_with_password_timeout(
        hanging.port,
        "/global/dispose",
        PASSWORD,
        Duration::from_millis(250),
    );
    assert!(matches!(result, Err(ureq::Error::Transport(_))));
    assert!(hanging.try_running(), "dispose 超时后进程必须由监督者强杀");
    hanging.kill_now();
}
