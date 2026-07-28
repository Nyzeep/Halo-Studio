//! fake-opencode：仅用于 Sidecar 集成测试的 OpenCode 1.x Server 替身。
//!
//! 它严格模拟本票的公开 Server 边界：`serve --hostname 127.0.0.1 --port <n>`、
//! `OPENCODE_SERVER_PASSWORD` Basic 认证、真实 session/message/SSE event 端点和
//! `POST /global/dispose`。不存在旧 `/task`、`/events`、`/cancel` 或 `/shutdown`。

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Write as _};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Default)]
struct ServeArgs {
    hostname: String,
    port: u16,
    password_digest_file: Option<String>,
    required_credential_env: Option<String>,
    require_isolated_state: bool,
    lock_file: Option<String>,
    dispose_marker_file: Option<String>,
}

#[derive(Default)]
struct FakeState {
    next_session: u64,
    next_action: u64,
    sessions: BTreeMap<String, FakeSession>,
    pending_permissions: BTreeMap<String, PendingFakeAction>,
    pending_questions: BTreeMap<String, PendingFakeAction>,
    event_clients: Vec<mpsc::Sender<Vec<u8>>>,
}

struct FakeSession {
    status: &'static str,
    messages: Vec<Value>,
}

struct PendingFakeAction {
    session_id: String,
    prompt: String,
}

/// 让 tiny-http 在独立请求线程中维持一个真实的、可晚到数据的 SSE body。
struct SseReader {
    receiver: mpsc::Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
}

impl Read for SseReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let read = self.current.read(buffer)?;
            if read != 0 {
                return Ok(read);
            }
            let next = self.receiver.recv().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "SSE 已关闭")
            })?;
            self.current = Cursor::new(next);
        }
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    args.extend(halo_testkit::test_harness_args());
    let mode = configured_mode(&args);

    if args.iter().any(|arg| arg == "--version") {
        println!("{}", version_for_mode(&mode));
        return;
    }

    let serve = match parse_serve_args(&args) {
        Ok(serve) => serve,
        Err(message) => {
            eprintln!("fake-opencode: {message}");
            std::process::exit(2);
        }
    };
    if serve.hostname != "127.0.0.1" {
        eprintln!("fake-opencode: 只允许绑定 127.0.0.1");
        std::process::exit(2);
    }

    let password = std::env::var("OPENCODE_SERVER_PASSWORD").ok();
    if let Some(env_var) = &serve.required_credential_env {
        if std::env::var_os(env_var).is_none() {
            eprintln!("fake-opencode: 缺少受管 Provider 凭据变量");
            std::process::exit(1);
        }
    }
    if serve.require_isolated_state && !has_isolated_state_dirs() {
        eprintln!("fake-opencode: 缺少受管 OpenCode 隔离状态目录");
        std::process::exit(1);
    }
    if let (Some(path), Some(password)) = (&serve.password_digest_file, &password) {
        if let Err(error) = append_digest(path, password) {
            eprintln!("fake-opencode: 写入认证摘要失败：{error}");
            std::process::exit(1);
        }
    }
    let expected_auth = password.map(|password| basic_authorization("opencode", &password));

    let _lock_file = match &serve.lock_file {
        Some(path) => Some(open_exclusive_lock(path).unwrap_or_else(|error| {
            eprintln!("fake-opencode: 无法独占锁文件 {path}：{error}");
            std::process::exit(1);
        })),
        None => None,
    };

    let server = match Server::http((serve.hostname.as_str(), serve.port)) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("fake-opencode: 绑定回环服务失败：{error}");
            std::process::exit(1);
        }
    };
    let state = Arc::new(Mutex::new(FakeState::default()));
    emit_listening_line(&mode, serve.port);

    if mode == "exit_early" {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(1);
        });
    }

    for request in server.incoming_requests() {
        let mode = mode.clone();
        let expected_auth = expected_auth.clone();
        let dispose_marker_file = serve.dispose_marker_file.clone();
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            handle(
                request,
                &mode,
                expected_auth.as_deref(),
                dispose_marker_file.as_deref(),
                state,
            );
        });
    }
}

fn configured_mode(args: &[String]) -> String {
    args.windows(2)
        .find_map(|pair| (pair[0] == "--mode").then(|| pair[1].clone()))
        .or_else(|| std::env::var("FAKE_OC_MODE").ok())
        .unwrap_or_else(|| "happy".to_string())
}

fn version_for_mode(mode: &str) -> &'static str {
    match mode {
        "old_version" => "1.18.4",
        "newer_1x" => "1.19.0",
        "wrong_version" | "major_version" => halo_testkit::OPENCODE_WRONG_VERSION,
        "malformed_version" => "1.18",
        "pre_release_version" => "1.18.5-beta.1",
        _ => halo_testkit::OPENCODE_VERSION,
    }
}

/// 真实 OpenCode 会在 stdout 报告监听地址；受管运行时据此验证它确实是所要求的
/// 回环端点。该行只写入受管子进程 stdout，由运行时私下消费，绝不转发到 IPC。
fn emit_listening_line(mode: &str, port: u16) {
    if mode == "missing_ready_line" {
        return;
    }
    let host = if mode == "wrong_ready_address" {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "opencode server listening on http://{host}:{port}")
        .and_then(|_| stdout.flush())
        .is_err()
    {
        std::process::exit(1);
    }
}

fn parse_serve_args(args: &[String]) -> Result<ServeArgs, String> {
    if args.first().map(String::as_str) != Some("serve") {
        return Err(
            "用法：fake-opencode --version | fake-opencode serve --hostname 127.0.0.1 --port <n>"
                .to_string(),
        );
    }
    let mut serve = ServeArgs::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--hostname" => {
                index += 1;
                serve.hostname = args.get(index).cloned().unwrap_or_default();
            }
            "--port" => {
                index += 1;
                serve.port = args
                    .get(index)
                    .and_then(|port| port.parse().ok())
                    .unwrap_or_default();
            }
            "--mode" => {
                index += 1;
            }
            "--password-digest-file" => {
                index += 1;
                serve.password_digest_file = args.get(index).cloned();
            }
            "--require-credential-env" => {
                index += 1;
                serve.required_credential_env = args.get(index).cloned();
            }
            "--require-isolated-state" => serve.require_isolated_state = true,
            "--lock-file" => {
                index += 1;
                serve.lock_file = args.get(index).cloned();
            }
            "--dispose-marker-file" => {
                index += 1;
                serve.dispose_marker_file = args.get(index).cloned();
            }
            _ => {}
        }
        index += 1;
    }
    if serve.hostname.is_empty() || serve.port == 0 {
        return Err("缺少 --hostname 或 --port 参数".to_string());
    }
    Ok(serve)
}

fn has_isolated_state_dirs() -> bool {
    [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
    ]
    .iter()
    .all(|name| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| std::path::Path::new(&value).is_dir())
    })
}

fn basic_authorization(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

fn append_digest(path: &str, password: &str) -> std::io::Result<()> {
    let digest = Sha256::digest(password.as_bytes());
    let digest = digest
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        });
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{digest}"))
}

fn open_exclusive_lock(path: &str) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(windows)]
    options.share_mode(0);
    options.open(path)
}

fn lock_state(state: &Mutex<FakeState>) -> std::sync::MutexGuard<'_, FakeState> {
    match state.lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn emit_event(state: &Arc<Mutex<FakeState>>, event: Value) {
    let data = format!("data: {}\n\n", event);
    let bytes = data.into_bytes();
    let mut state = lock_state(state);
    state
        .event_clients
        .retain(|client| client.send(bytes.clone()).is_ok());
}

fn respond_sse(request: Request, state: Arc<Mutex<FakeState>>, mode: &str) {
    let (sender, receiver) = mpsc::channel();
    let initial_session = {
        let mut state = lock_state(&state);
        state.event_clients.push(sender.clone());
        matches!(mode, "initial_idle" | "initial_busy_then_idle")
            .then(|| state.sessions.keys().next().cloned())
            .flatten()
    };
    let _ = sender.send(
        b"data: {\"id\":\"evt_connected\",\"type\":\"server.connected\",\"properties\":{}}\n\n"
            .to_vec(),
    );
    if let Some(session_id) = initial_session {
        let statuses: &[&str] = match mode {
            "initial_idle" => &["idle"],
            "initial_busy_then_idle" => &["busy", "idle"],
            _ => &[],
        };
        for (index, status) in statuses.iter().enumerate() {
            let event = json!({"id": format!("evt_initial_{index}"), "type": "session.status", "properties": {
                "sessionID": session_id.as_str(), "status": {"type": status}
            }});
            let _ = sender.send(format!("data: {event}\n\n").into_bytes());
        }
    }
    // 后续生命周期只由 event_clients 中的克隆控制；测试模式清空该列表时能真正 EOF。
    drop(sender);
    let header = Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..])
        .expect("SSE content type 应有效");
    let response = Response::new(
        StatusCode(200),
        vec![header],
        SseReader {
            receiver,
            current: Cursor::new(Vec::new()),
        },
        None,
        None,
    );
    let _ = request.respond(response);
}

fn respond_session_created(request: Request, state: &Arc<Mutex<FakeState>>) {
    let id = {
        let mut state = lock_state(state);
        state.next_session += 1;
        let id = format!("ses_fake_{}", state.next_session);
        state.sessions.insert(
            id.clone(),
            FakeSession {
                status: "idle",
                messages: Vec::new(),
            },
        );
        id
    };
    respond_json(request, 200, &json!({"id": id}));
}

fn respond_session_status(request: Request, state: &Arc<Mutex<FakeState>>) {
    let sessions = {
        let state = lock_state(state);
        state
            .sessions
            .iter()
            .map(|(id, session)| (id.clone(), json!({"type": session.status})))
            .collect::<serde_json::Map<String, Value>>()
    };
    respond_json(request, 200, &Value::Object(sessions));
}

fn respond_session_messages(request: Request, state: &Arc<Mutex<FakeState>>, session_id: &str) {
    let messages = {
        let state = lock_state(state);
        state
            .sessions
            .get(session_id)
            .map(|session| session.messages.clone())
    };
    match messages {
        Some(messages) => respond_json(request, 200, &Value::Array(messages)),
        None => respond_json(request, 404, &json!({"error": "session_not_found"})),
    }
}

fn begin_prompt(
    mut request: Request,
    mode: String,
    state: Arc<Mutex<FakeState>>,
    session_id: String,
) {
    let mut body = String::new();
    let prompt = request
        .as_reader()
        .read_to_string(&mut body)
        .ok()
        .and_then(|_| serde_json::from_str::<Value>(&body).ok())
        .and_then(|body| {
            body.get("parts")
                .and_then(Value::as_array)
                .and_then(|parts| parts.first())
                .and_then(|part| part.get("text"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let Some(prompt) = prompt else {
        respond_json(request, 400, &json!({"error": "invalid_prompt"}));
        return;
    };
    {
        let mut state = lock_state(&state);
        let Some(session) = state.sessions.get_mut(&session_id) else {
            respond_json(request, 404, &json!({"error": "session_not_found"}));
            return;
        };
        session.status = "busy";
    }
    if matches!(mode.as_str(), "permission_once" | "permission_reject") {
        respond_json(request, 204, &json!({}));
        std::thread::spawn(move || emit_permission_request(state, session_id, prompt));
        return;
    }
    if matches!(mode.as_str(), "clarification_once" | "clarification_reject") {
        respond_json(request, 204, &json!({}));
        std::thread::spawn(move || emit_question_request(state, session_id, prompt));
        return;
    }
    if mode == "fast_initial_round" {
        // 此模式让服务端在 `prompt_async` 取得 204 前已经完成一轮：适配器不能
        // 把首个状态快照中的 idle 或抢先抵达的 SSE 当成未开始。
        emit_round(state, mode, session_id, prompt);
        respond_json(request, 204, &json!({}));
    } else {
        respond_json(request, 204, &json!({}));
        std::thread::spawn(move || emit_round(state, mode, session_id, prompt));
    }
}

fn emit_round(state: Arc<Mutex<FakeState>>, mode: String, session_id: String, prompt: String) {
    let missing_busy_eof = mode == "missing_busy_eof";
    if !missing_busy_eof {
        emit_event(
            &state,
            json!({"id": "evt_busy", "type": "session.status", "properties": {
                "sessionID": session_id.as_str(), "status": {"type": "busy"}
            }}),
        );
        emit_event(
            &state,
            json!({"id": "evt_text", "type": "message.part.updated", "properties": {
                "sessionID": session_id.as_str(),
                "part": {"type": "text", "text": "fake-opencode 内部流式文本"}
            }}),
        );
        emit_event(
            &state,
            json!({"id": "evt_file", "type": "file.edited", "properties": {
                "sessionID": session_id.as_str(), "file": "src/fake.rs"
            }}),
        );
        emit_event(
            &state,
            json!({"id": "evt_unknown", "type": "future.unknown", "properties": {}}),
        );
    }

    if mode == "stale_idle" {
        emit_event(
            &state,
            json!({"id": "evt_stale_idle", "type": "session.status", "properties": {
                "sessionID": session_id.as_str(), "status": {"type": "idle"}
            }}),
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let message = if mode == "message_error" {
        json!({
            "info": {"id": "msg_fake_1", "sessionID": session_id.as_str(), "role": "assistant",
                "time": {"created": 1, "completed": 2}, "error": {"name": "APIError"}},
            "parts": []
        })
    } else {
        json!({
            "info": {"id": "msg_fake_1", "sessionID": session_id.as_str(), "role": "assistant",
                "time": {"created": 1, "completed": 2}},
            "parts": [
                {"type": "reasoning", "text": "不会作为活动会话回复"},
                {"type": "tool", "tool": "write", "state": {"status": "completed"}, "output": "原始工具输出"},
                {"type": "text", "text": "fake-opencode 已完成首轮回复。"}
            ]
        })
    };
    let user_message = json!({
        "info": {"id": "msg_fake_user_1", "sessionID": session_id.as_str(), "role": "user",
            "time": {"created": 0}},
        "parts": [{"type": "text", "text": prompt}]
    });
    {
        let mut state = lock_state(&state);
        if let Some(session) = state.sessions.get_mut(&session_id) {
            session.status = "idle";
            session.messages = vec![user_message, message];
        } else {
            return;
        }
        if missing_busy_eof {
            state.event_clients.clear();
            return;
        }
    }
    emit_event(
        &state,
        json!({"id": "evt_idle", "type": "session.status", "properties": {
            "sessionID": session_id.as_str(), "status": {"type": "idle"}
        }}),
    );
    if mode == "fast_initial_round" {
        emit_event(
            &state,
            json!({"id": "evt_idle_duplicate", "type": "session.status", "properties": {
                "sessionID": session_id.as_str(), "status": {"type": "idle"}
            }}),
        );
        lock_state(&state).event_clients.clear();
    }
}

fn emit_permission_request(state: Arc<Mutex<FakeState>>, session_id: String, prompt: String) {
    let request_id = {
        let mut state = lock_state(&state);
        state.next_action += 1;
        let request_id = format!("per_fake_{}", state.next_action);
        state.pending_permissions.insert(
            request_id.clone(),
            PendingFakeAction {
                session_id: session_id.clone(),
                prompt,
            },
        );
        request_id
    };
    emit_event(
        &state,
        json!({"id": "evt_permission_busy", "type": "session.status", "properties": {
            "sessionID": session_id.as_str(), "status": {"type": "busy"}
        }}),
    );
    emit_event(
        &state,
        json!({"id": "evt_permission_asked", "type": "permission.asked", "properties": {
            "id": request_id, "sessionID": session_id.as_str(), "permission": "edit",
            "patterns": ["src/fake.rs"], "metadata": {}
        }}),
    );
}

fn emit_question_request(state: Arc<Mutex<FakeState>>, session_id: String, prompt: String) {
    let request_id = {
        let mut state = lock_state(&state);
        state.next_action += 1;
        let request_id = format!("que_fake_{}", state.next_action);
        state.pending_questions.insert(
            request_id.clone(),
            PendingFakeAction {
                session_id: session_id.clone(),
                prompt,
            },
        );
        request_id
    };
    emit_event(
        &state,
        json!({"id": "evt_question_busy", "type": "session.status", "properties": {
            "sessionID": session_id.as_str(), "status": {"type": "busy"}
        }}),
    );
    emit_event(
        &state,
        json!({"id": "evt_question_asked", "type": "question.asked", "properties": {
            "id": request_id, "sessionID": session_id.as_str(),
            "questions": [{
                "question": "请提供继续任务所需的澄清", "header": "澄清",
                "options": [], "multiple": false, "custom": false
            }]
        }}),
    );
}

fn emit_action_rejection(state: Arc<Mutex<FakeState>>, session_id: String, prompt: String) {
    emit_event(
        &state,
        json!({"id": "evt_action_error", "type": "session.error", "properties": {
            "sessionID": session_id.as_str(), "error": {"name": "PermissionRejectedError"}
        }}),
    );
    // 保存原生失败结论，供任何在 session.error 后拉取快照的客户端如实读取。
    emit_round(state, "message_error".to_string(), session_id, prompt);
}

fn read_request_json(request: &mut Request) -> Option<Value> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .ok()
        .and_then(|_| serde_json::from_str(&body).ok())
}

fn respond_permission_decision(
    mut request: Request,
    state: Arc<Mutex<FakeState>>,
    request_id: &str,
) {
    let reply = read_request_json(&mut request).and_then(|body| {
        body.get("reply")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let Some(reply) = reply.filter(|reply| matches!(reply.as_str(), "once" | "reject")) else {
        respond_json(request, 400, &json!({"error": "invalid_permission_reply"}));
        return;
    };
    let pending = lock_state(&state).pending_permissions.remove(request_id);
    let Some(pending) = pending else {
        respond_json(request, 404, &json!({"error": "permission_not_found"}));
        return;
    };
    respond_json(request, 200, &json!({}));
    emit_event(
        &state,
        json!({"id": "evt_permission_replied", "type": "permission.replied", "properties": {
            "sessionID": pending.session_id.as_str(), "requestID": request_id, "reply": reply
        }}),
    );
    if reply == "once" {
        std::thread::spawn(move || {
            emit_round(
                state,
                "happy".to_string(),
                pending.session_id,
                pending.prompt,
            )
        });
    } else {
        std::thread::spawn(move || {
            emit_action_rejection(state, pending.session_id, pending.prompt)
        });
    }
}

fn scalar_question_answer(body: Option<Value>) -> bool {
    body.and_then(|body| {
        body.get("answers")
            .and_then(Value::as_array)
            .filter(|answers| answers.len() == 1)
            .and_then(|answers| answers[0].as_array())
            .filter(|answer| answer.len() == 1)
            .and_then(|answer| answer[0].as_str())
            .map(str::trim)
            .filter(|answer| !answer.is_empty())
            .map(str::to_string)
    })
    .is_some()
}

fn respond_question_answer(mut request: Request, state: Arc<Mutex<FakeState>>, request_id: &str) {
    if !scalar_question_answer(read_request_json(&mut request)) {
        respond_json(request, 400, &json!({"error": "invalid_question_answer"}));
        return;
    }
    let pending = lock_state(&state).pending_questions.remove(request_id);
    let Some(pending) = pending else {
        respond_json(request, 404, &json!({"error": "question_not_found"}));
        return;
    };
    respond_json(request, 200, &json!({}));
    emit_event(
        &state,
        json!({"id": "evt_question_replied", "type": "question.replied", "properties": {
            "sessionID": pending.session_id.as_str(), "requestID": request_id
        }}),
    );
    std::thread::spawn(move || {
        emit_round(
            state,
            "happy".to_string(),
            pending.session_id,
            pending.prompt,
        )
    });
}

fn respond_question_rejection(request: Request, state: Arc<Mutex<FakeState>>, request_id: &str) {
    let pending = lock_state(&state).pending_questions.remove(request_id);
    let Some(pending) = pending else {
        respond_json(request, 404, &json!({"error": "question_not_found"}));
        return;
    };
    respond_json(request, 200, &json!({}));
    emit_event(
        &state,
        json!({"id": "evt_question_rejected", "type": "question.rejected", "properties": {
            "sessionID": pending.session_id.as_str(), "requestID": request_id
        }}),
    );
    std::thread::spawn(move || emit_action_rejection(state, pending.session_id, pending.prompt));
}

fn respond_session_abort(request: Request, state: Arc<Mutex<FakeState>>, session_id: &str) {
    let exists = {
        let mut state_guard = lock_state(&state);
        if let Some(session) = state_guard.sessions.get_mut(session_id) {
            session.status = "idle";
            state_guard
                .pending_permissions
                .retain(|_, pending| pending.session_id != session_id);
            state_guard
                .pending_questions
                .retain(|_, pending| pending.session_id != session_id);
            true
        } else {
            false
        }
    };
    if !exists {
        respond_json(request, 404, &json!({"error": "session_not_found"}));
        return;
    }
    respond_json(request, 200, &json!(true));
    emit_event(
        &state,
        json!({"id": "evt_abort", "type": "session.error", "properties": {
            "sessionID": session_id, "error": {"name": "MessageAbortedError"}
        }}),
    );
}

fn session_id_from_path(path: &str, suffix: &str) -> Option<&str> {
    path.strip_prefix("/session/")?.strip_suffix(suffix)
}

fn action_id_from_path<'a>(path: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)?.strip_suffix(suffix)
}

fn handle(
    mut request: Request,
    mode: &str,
    expected_auth: Option<&str>,
    dispose_marker_file: Option<&str>,
    state: Arc<Mutex<FakeState>>,
) {
    let authorized = expected_auth.is_some_and(|expected| {
        request
            .headers()
            .iter()
            .any(|header| header.field.equiv("Authorization") && header.value.as_str() == expected)
    });
    if !authorized || mode == "bad_auth" {
        respond_json(request, 401, &json!({"error": "unauthorized"}));
        return;
    }

    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or_default();
    if request.method() == &Method::Get && path == "/event" {
        respond_sse(request, state, mode);
        return;
    }
    if request.method() == &Method::Post && path == "/session" {
        respond_session_created(request, &state);
        return;
    }
    if request.method() == &Method::Get && path == "/session/status" {
        respond_session_status(request, &state);
        return;
    }
    if request.method() == &Method::Get {
        if let Some(session_id) = session_id_from_path(path, "/message") {
            respond_session_messages(request, &state, session_id);
            return;
        }
    }
    if request.method() == &Method::Post {
        if let Some(session_id) = session_id_from_path(path, "/prompt_async") {
            begin_prompt(request, mode.to_string(), state, session_id.to_string());
            return;
        }
        if let Some(session_id) = session_id_from_path(path, "/abort") {
            respond_session_abort(request, state, session_id);
            return;
        }
        if let Some(request_id) = action_id_from_path(path, "/permission/", "/reply") {
            respond_permission_decision(request, state, request_id);
            return;
        }
        if let Some(request_id) = action_id_from_path(path, "/question/", "/reply") {
            respond_question_answer(request, state, request_id);
            return;
        }
        if let Some(request_id) = action_id_from_path(path, "/question/", "/reject") {
            respond_question_rejection(request, state, request_id);
            return;
        }
    }

    match (request.method().clone(), path) {
        (Method::Get, "/global/health") => match mode {
            "unhealthy" => respond_json(request, 200, &json!({"healthy": false})),
            "missing_health_version" => respond_json(request, 200, &json!({"healthy": true})),
            _ => respond_json(
                request,
                200,
                &json!({"healthy": true, "version": version_for_mode(mode)}),
            ),
        },
        // OpenCode 的 dispose 只回收服务器内部资源，不承诺退出宿主进程。
        // 受管监督者必须在宽限期后主动终止进程；三个模式分别覆盖成功、失败和超时。
        (Method::Post, "/global/dispose") => {
            if let Some(path) = dispose_marker_file {
                let _ = std::fs::write(path, "global_dispose");
            }
            match mode {
                "dispose_failure" => {
                    respond_json(request, 500, &json!({"error": "dispose_failed"}))
                }
                "hang_on_dispose" => std::thread::sleep(Duration::from_secs(30)),
                _ => {
                    emit_event(
                        &state,
                        json!({"id": "evt_disposed", "type": "server.instance.disposed", "properties": {}}),
                    );
                    respond_json(request, 200, &json!({}))
                }
            }
        }
        _ => respond_json(request, 404, &json!({"error": "not_found"})),
    }
}

fn respond_json(request: Request, status: u16, body: &Value) {
    let mut response = Response::from_data(body.to_string().into_bytes()).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
}
