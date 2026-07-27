//! fake-opencode 集成测试：spawn 真实 bin，按第 5 节 OpenCode 回环服务协议交互。

mod support;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use support::KillOnDrop;

const TOKEN: &str = "test-token-1234567890abcdef";

struct OcProc {
    child: KillOnDrop,
    port: u16,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("申请空闲端口失败")
        .local_addr()
        .expect("读取本地地址失败")
        .port()
}

fn spawn_oc(mode: &str, dir: &std::path::Path) -> OcProc {
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_fake-opencode"))
        .args(["serve", "--hostname", "127.0.0.1", "--port", &port.to_string()])
        .env("FAKE_OC_MODE", mode)
        .env("HALO_OC_TOKEN", TOKEN)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 fake-opencode 失败");
    let oc = OcProc {
        child: KillOnDrop::new(child),
        port,
    };
    wait_listening(port);
    oc
}

/// 用纯 TCP 连接探测端口就绪，避开鉴权与模式差异。
fn wait_listening(port: u16) {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "等待 fake-opencode 监听超时");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build()
}

fn get_with_token(port: u16, path: &str, token: &str) -> Result<ureq::Response, ureq::Error> {
    agent()
        .get(&format!("http://127.0.0.1:{port}{path}"))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
}

fn post_with_token(port: u16, path: &str, token: &str) -> Result<ureq::Response, ureq::Error> {
    agent()
        .post(&format!("http://127.0.0.1:{port}{path}"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_string("{}")
}

fn status_of(result: Result<ureq::Response, ureq::Error>) -> u16 {
    match result {
        Ok(resp) => resp.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(e) => panic!("HTTP 传输错误：{e}"),
    }
}

fn body_of(result: Result<ureq::Response, ureq::Error>) -> Value {
    match result {
        Ok(resp) => resp.into_json().expect("响应体不是合法 JSON"),
        Err(e) => panic!("HTTP 请求失败：{e}"),
    }
}

#[test]
fn happy_task_flow_and_graceful_shutdown() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("happy", dir.path());

    let health = body_of(get_with_token(oc.port, "/health", TOKEN));
    assert_eq!(health["status"], "ok");

    let version = body_of(get_with_token(oc.port, "/version", TOKEN));
    assert_eq!(version["version"], "0.4.2");

    assert_eq!(status_of(post_with_token(oc.port, "/task", TOKEN)), 200);

    // 长轮询直到 done
    let mut events: Vec<Value> = Vec::new();
    let outcome;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "等待事件流完成超时");
        let page = body_of(get_with_token(
            oc.port,
            &format!("/events?after={}", events.len()),
            TOKEN,
        ));
        if let Some(arr) = page["events"].as_array() {
            events.extend(arr.iter().cloned());
        }
        if page["done"] == true {
            outcome = page["outcome"].clone();
            break;
        }
    }
    assert_eq!(outcome, "finished");
    let phases: Vec<&str> = events
        .iter()
        .filter(|e| e["kind"] == "phase")
        .filter_map(|e| e["detail"]["phase"].as_str())
        .collect();
    assert_eq!(phases, ["planning", "editing", "verifying"]);
    assert!(events.iter().any(|e| e["kind"] == "agent_note"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "file_hint" && e["detail"]["path"] == "hello_from_agent.txt"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "verification" && e["detail"]["status"] == "passed"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("hello_from_agent.txt"))
            .expect("happy 模式必须真实写入文件"),
        "hello from agent"
    );

    assert_eq!(status_of(post_with_token(oc.port, "/shutdown", TOKEN)), 200);
    let status = oc
        .child
        .wait_exit(Duration::from_secs(5))
        .expect("shutdown 后应优雅退出");
    assert_eq!(status.code(), Some(0));
}

#[test]
fn rejects_wrong_or_missing_token_with_401() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("happy", dir.path());

    // 错误 token
    assert_eq!(status_of(get_with_token(oc.port, "/health", "wrong-token")), 401);
    // 缺失 Authorization 头
    let missing = agent()
        .get(&format!("http://127.0.0.1:{}/health", oc.port))
        .call();
    assert_eq!(status_of(missing), 401);
    // 正确 token 正常通过
    assert_eq!(status_of(get_with_token(oc.port, "/health", TOKEN)), 200);

    assert!(oc.child.try_running());
}

#[test]
fn bad_token_mode_rejects_even_correct_token() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("bad_token", dir.path());
    assert_eq!(status_of(get_with_token(oc.port, "/health", TOKEN)), 401);
    assert!(oc.child.try_running());
}

#[test]
fn wrong_version_mode_reports_mismatched_version() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let oc = spawn_oc("wrong_version", dir.path());
    let version = body_of(get_with_token(oc.port, "/version", TOKEN));
    // 与锁定版本 0.4.2 不相等，上游应据此判 RUNTIME_VERSION_MISMATCH
    assert_eq!(version["version"], "9.9.9");
}

#[test]
fn unhealthy_mode_returns_500_on_health() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let oc = spawn_oc("unhealthy", dir.path());
    assert_eq!(status_of(get_with_token(oc.port, "/health", TOKEN)), 500);
    // 版本端点不受 unhealthy 影响
    let version = body_of(get_with_token(oc.port, "/version", TOKEN));
    assert_eq!(version["version"], "0.4.2");
}

#[test]
fn hang_on_shutdown_mode_requires_kill() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("hang_on_shutdown", dir.path());

    assert_eq!(status_of(post_with_token(oc.port, "/shutdown", TOKEN)), 200);
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        oc.child.try_running(),
        "hang_on_shutdown 模式不应因 /shutdown 退出，需要强杀"
    );
    oc.child.kill_now();
    assert!(oc.child.wait_exit(Duration::from_secs(5)).is_some());
}

#[test]
fn exit_early_mode_exits_by_itself() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("exit_early", dir.path());
    // 启动后约 2 秒自行退出
    assert!(
        oc.child.wait_exit(Duration::from_secs(6)).is_some(),
        "exit_early 模式应自行退出"
    );
}
