//! fake-opencode 的独立契约测试：只模拟 OpenCode 1.x Server 的真实启动边界。

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
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

fn post_json_with_password(
    port: u16,
    path: &str,
    password: &str,
    body: &Value,
) -> Result<Response, ureq::Error> {
    agent()
        .post(&format!("http://127.0.0.1:{port}{path}"))
        .set("Authorization", &basic_header("opencode", password))
        .send_json(body)
}

fn open_event_stream(port: u16) -> Receiver<Value> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (event_tx, event_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let response = agent()
            .get(&format!(
                "http://127.0.0.1:{port}/event?directory=C%3A%5Cfake"
            ))
            .set("Authorization", &basic_header("opencode", PASSWORD))
            .call()
            .expect("SSE 事件流应建立");
        let _ = ready_tx.send(());
        let mut reader = BufReader::new(response.into_reader());
        let mut data = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or_default() == 0 {
                return;
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !data.is_empty() {
                    if let Ok(event) = serde_json::from_str(&data) {
                        if event_tx.send(event).is_err() {
                            return;
                        }
                    }
                    data.clear();
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.trim_start());
            }
        }
    });
    ready_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("SSE 事件流应及时连接");
    event_rx
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
fn serves_real_session_prompt_status_message_and_sse_round_trip() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("happy", dir.path());
    let events = open_event_stream(oc.port);

    let session = body_of(post_json_with_password(
        oc.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 应返回 id")
        .to_string();
    assert!(session_id.starts_with("ses_"));

    let prompt = post_json_with_password(
        oc.port,
        &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
        PASSWORD,
        &json!({"parts": [{"type": "text", "text": "实现一个小改动"}]}),
    );
    assert_eq!(status_of(prompt), 204);

    let mut kinds = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let event = events
            .recv_timeout(Duration::from_millis(250))
            .expect("应收到脚本化 SSE 事件");
        let kind = event["type"].as_str().unwrap_or_default().to_string();
        let is_idle = kind == "session.status"
            && event["properties"]["sessionID"].as_str() == Some(session_id.as_str())
            && event["properties"]["status"]["type"].as_str() == Some("idle");
        kinds.push(kind);
        if is_idle {
            break;
        }
    }
    assert!(kinds.iter().any(|kind| kind == "session.status"));
    assert!(kinds.iter().any(|kind| kind == "message.part.updated"));
    assert!(kinds.iter().any(|kind| kind == "file.edited"));
    assert!(kinds.iter().any(|kind| kind == "future.unknown"));

    let status = body_of(get_with_password(
        oc.port,
        "/session/status?directory=C%3A%5Cfake",
        PASSWORD,
    ));
    assert_eq!(status[session_id.as_str()]["type"], "idle");
    let messages = body_of(get_with_password(
        oc.port,
        &format!("/session/{session_id}/message?limit=20&directory=C%3A%5Cfake"),
        PASSWORD,
    ));
    assert_eq!(messages[0]["info"]["role"], "user");
    assert_eq!(messages[0]["parts"][0]["text"], "实现一个小改动");
    assert_eq!(messages[1]["info"]["role"], "assistant");
    assert_eq!(
        messages[1]["parts"][2]["text"],
        "fake-opencode 已完成首轮回复。"
    );
    oc.kill_now();
}

#[test]
fn serves_one_time_permission_and_clarification_reply_contracts() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");

    let mut permission = spawn_oc("permission_once", dir.path());
    let permission_events = open_event_stream(permission.port);
    let session = body_of(post_json_with_password(
        permission.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 必须有 id")
        .to_string();
    assert_eq!(
        status_of(post_json_with_password(
            permission.port,
            &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"parts": [{"type": "text", "text": "请求权限"}]}),
        )),
        204
    );
    let permission_request = loop {
        let event = permission_events
            .recv_timeout(Duration::from_secs(2))
            .expect("权限模式应发送 permission.asked");
        if event["type"] == "permission.asked" {
            break event;
        }
    };
    let permission_id = permission_request["properties"]["id"]
        .as_str()
        .expect("权限请求必须有远端 id")
        .to_string();
    assert_eq!(
        status_of(post_json_with_password(
            permission.port,
            &format!("/permission/{permission_id}/reply?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"reply": "always"}),
        )),
        400,
        "测试替身不得接受永久放行"
    );
    assert_eq!(
        status_of(post_json_with_password(
            permission.port,
            &format!("/permission/{permission_id}/reply?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"reply": "once"}),
        )),
        200
    );
    let mut permission_replied = false;
    let mut permission_idle = false;
    while !(permission_replied && permission_idle) {
        let event = permission_events
            .recv_timeout(Duration::from_secs(2))
            .expect("本次允许后应继续当前轮次");
        permission_replied |= event["type"] == "permission.replied"
            && event["properties"]["requestID"] == permission_id
            && event["properties"]["reply"] == "once";
        permission_idle |= event["type"] == "session.status"
            && event["properties"]["sessionID"] == session_id
            && event["properties"]["status"]["type"] == "idle";
    }
    permission.kill_now();

    let mut question = spawn_oc("clarification_once", dir.path());
    let question_events = open_event_stream(question.port);
    let session = body_of(post_json_with_password(
        question.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 必须有 id")
        .to_string();
    assert_eq!(
        status_of(post_json_with_password(
            question.port,
            &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"parts": [{"type": "text", "text": "请求澄清"}]}),
        )),
        204
    );
    let question_request = loop {
        let event = question_events
            .recv_timeout(Duration::from_secs(2))
            .expect("澄清模式应发送 question.asked");
        if event["type"] == "question.asked" {
            break event;
        }
    };
    let question_id = question_request["properties"]["id"]
        .as_str()
        .expect("澄清请求必须有远端 id")
        .to_string();
    assert_eq!(
        status_of(post_json_with_password(
            question.port,
            &format!("/question/{question_id}/reply?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"answers": [["继续"], ["多余答案"]]}),
        )),
        400,
        "替身仅接受本票支持的单项回答"
    );
    assert_eq!(
        status_of(post_json_with_password(
            question.port,
            &format!("/question/{question_id}/reply?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"answers": [["继续"]]}),
        )),
        200
    );
    let mut question_replied = false;
    let mut question_idle = false;
    while !(question_replied && question_idle) {
        let event = question_events
            .recv_timeout(Duration::from_secs(2))
            .expect("回答澄清后应继续当前轮次");
        question_replied |=
            event["type"] == "question.replied" && event["properties"]["requestID"] == question_id;
        question_idle |= event["type"] == "session.status"
            && event["properties"]["sessionID"] == session_id
            && event["properties"]["status"]["type"] == "idle";
    }
    question.kill_now();
}

#[test]
fn serves_permission_and_clarification_rejections_as_native_events() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    for (mode, event_kind, endpoint_suffix, body) in [
        (
            "permission_reject",
            "permission.asked",
            "reply",
            Some(json!({"reply": "reject"})),
        ),
        ("clarification_reject", "question.asked", "reject", None),
    ] {
        let mut oc = spawn_oc(mode, dir.path());
        let events = open_event_stream(oc.port);
        let session = body_of(post_json_with_password(
            oc.port,
            "/session?directory=C%3A%5Cfake",
            PASSWORD,
            &json!({}),
        ));
        let session_id = session["id"]
            .as_str()
            .expect("session 必须有 id")
            .to_string();
        assert_eq!(
            status_of(post_json_with_password(
                oc.port,
                &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
                PASSWORD,
                &json!({"parts": [{"type": "text", "text": "拒绝请求"}]}),
            )),
            204
        );
        let asked = loop {
            let event = events
                .recv_timeout(Duration::from_secs(2))
                .expect("应收到操作请求");
            if event["type"] == event_kind {
                break event;
            }
        };
        let request_id = asked["properties"]["id"]
            .as_str()
            .expect("操作请求必须有 id");
        let path = match event_kind {
            "permission.asked" => format!("/permission/{request_id}/{endpoint_suffix}"),
            _ => format!("/question/{request_id}/{endpoint_suffix}"),
        };
        let result = match body.as_ref() {
            Some(body) => post_json_with_password(oc.port, &path, PASSWORD, body),
            None => post_with_password(oc.port, &path, PASSWORD),
        };
        assert_eq!(status_of(result), 200);

        let expected_resolution = if event_kind == "permission.asked" {
            "permission.replied"
        } else {
            "question.rejected"
        };
        let mut resolved = false;
        let mut failed = false;
        while !(resolved && failed) {
            let event = events
                .recv_timeout(Duration::from_secs(2))
                .expect("拒绝后应收到原生回执和失败事件");
            resolved |= event["type"] == expected_resolution;
            failed |=
                event["type"] == "session.error" && event["properties"]["sessionID"] == session_id;
        }
        oc.kill_now();
    }
}

#[test]
fn initial_busy_then_idle_mode_announces_a_created_session_before_the_prompt() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("initial_busy_then_idle", dir.path());
    let session = body_of(post_json_with_password(
        oc.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 应返回 id")
        .to_string();

    let events = open_event_stream(oc.port);
    let connected = events
        .recv_timeout(Duration::from_secs(2))
        .expect("应先收到 SSE 已连接事件");
    assert_eq!(connected["type"], "server.connected");
    let initial_busy = events
        .recv_timeout(Duration::from_secs(2))
        .expect("初始 busy 应在 prompt 前送达");
    assert_eq!(initial_busy["type"], "session.status");
    assert_eq!(
        initial_busy["properties"]["sessionID"].as_str(),
        Some(session_id.as_str())
    );
    assert_eq!(initial_busy["properties"]["status"]["type"], "busy");
    let initial_idle = events
        .recv_timeout(Duration::from_secs(2))
        .expect("初始 idle 应在 prompt 前送达");
    assert_eq!(initial_idle["type"], "session.status");
    assert_eq!(initial_idle["properties"]["status"]["type"], "idle");
    oc.kill_now();
}

#[test]
fn missing_busy_eof_mode_closes_the_stream_after_persisting_the_reply() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("missing_busy_eof", dir.path());
    let events = open_event_stream(oc.port);
    let session = body_of(post_json_with_password(
        oc.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 应返回 id")
        .to_string();
    assert_eq!(
        status_of(post_json_with_password(
            oc.port,
            &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"parts": [{"type": "text", "text": "验证 EOF"}]}),
        )),
        204
    );
    let connected = events
        .recv_timeout(Duration::from_secs(2))
        .expect("应先收到 SSE 已连接事件");
    assert_eq!(connected["type"], "server.connected");
    assert!(
        events.recv_timeout(Duration::from_secs(2)).is_err(),
        "不含 busy 的模式必须关闭事件流"
    );

    let messages = body_of(get_with_password(
        oc.port,
        &format!("/session/{session_id}/message?limit=20&directory=C%3A%5Cfake"),
        PASSWORD,
    ));
    assert_eq!(messages[1]["info"]["role"], "assistant");
    oc.kill_now();
}

#[test]
fn fast_initial_round_completes_before_prompt_ack_and_closes_the_event_stream() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("fast_initial_round", dir.path());
    let events = open_event_stream(oc.port);
    let session = body_of(post_json_with_password(
        oc.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 应返回 id")
        .to_string();

    assert_eq!(
        status_of(post_json_with_password(
            oc.port,
            &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"parts": [{"type": "text", "text": "验证极速完成"}]}),
        )),
        204
    );
    let status = body_of(get_with_password(
        oc.port,
        "/session/status?directory=C%3A%5Cfake",
        PASSWORD,
    ));
    assert_eq!(status[session_id.as_str()]["type"], "idle");
    let messages = body_of(get_with_password(
        oc.port,
        &format!("/session/{session_id}/message?limit=20&directory=C%3A%5Cfake"),
        PASSWORD,
    ));
    assert_eq!(messages[1]["info"]["role"], "assistant");

    let mut statuses = Vec::new();
    while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
        if event["type"].as_str() == Some("session.status")
            && event["properties"]["sessionID"].as_str() == Some(session_id.as_str())
        {
            statuses.push(
                event["properties"]["status"]["type"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    assert_eq!(statuses, vec!["busy", "idle", "idle"]);
    oc.kill_now();
}

#[test]
fn stale_idle_mode_exposes_an_out_of_order_idle_before_the_real_round_completion() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut oc = spawn_oc("stale_idle", dir.path());
    let events = open_event_stream(oc.port);
    let session = body_of(post_json_with_password(
        oc.port,
        "/session?directory=C%3A%5Cfake",
        PASSWORD,
        &json!({}),
    ));
    let session_id = session["id"]
        .as_str()
        .expect("session 应返回 id")
        .to_string();
    assert_eq!(
        status_of(post_json_with_password(
            oc.port,
            &format!("/session/{session_id}/prompt_async?directory=C%3A%5Cfake"),
            PASSWORD,
            &json!({"parts": [{"type": "text", "text": "验证乱序"}]}),
        )),
        204
    );

    let mut statuses = Vec::new();
    while statuses.len() < 3 {
        let event = events
            .recv_timeout(Duration::from_secs(2))
            .expect("乱序模式仍应产生全部状态事件");
        if event["type"].as_str() == Some("session.status")
            && event["properties"]["sessionID"].as_str() == Some(session_id.as_str())
        {
            statuses.push(
                event["properties"]["status"]["type"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    assert_eq!(
        statuses,
        vec!["busy".to_string(), "idle".to_string(), "idle".to_string()]
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
