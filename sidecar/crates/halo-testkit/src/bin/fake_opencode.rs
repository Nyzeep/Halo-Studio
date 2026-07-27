//! fake-opencode：受控假 OpenCode 回环服务。
//! `--version` 输出版本首行（FAKE_OC_VERSION 覆盖，默认 0.4.2），供 Sidecar 探测使用；
//! 解析 `serve --hostname 127.0.0.1 --port <n>`；只绑 127.0.0.1；
//! 全部端点校验 `Authorization: Bearer == HALO_OC_TOKEN`，不符回 401。
//! 行为由 FAKE_OC_MODE 脚本化：happy | unhealthy | wrong_version | bad_token |
//! exit_early | hang_on_shutdown。token 值只在内存中比对，绝不写入日志或响应。
//!
//! Sidecar 以白名单环境启动子进程（宿主 FAKE_* 变量不会传入），因此脚本开关亦可经
//! 命令行参数注入（Sidecar 会把 LaunchConfig.extra_args 附加在 serve 参数后）：
//! `--mode <m>`（优先于 FAKE_OC_MODE）、`--token-digest-file <path>`（启动时把收到的
//! HALO_OC_TOKEN 的 SHA-256 十六进制摘要**追加**写入该文件——只写摘要不写明文，
//! 供“每次启动新认证信息”测试对比两次启动的 token 确实不同）。

use std::io::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server};

use halo_testkit::ScriptStep;

#[derive(Default)]
struct TaskShared {
    events: Vec<Value>,
    done: bool,
    outcome: Option<String>,
    summary: Option<String>,
    cancel_requested: bool,
}

/// serve 子命令解析结果。
struct ServeArgs {
    hostname: String,
    port: u16,
    mode: Option<String>,
    token_digest_file: Option<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        let version = std::env::var("FAKE_OC_VERSION")
            .unwrap_or_else(|_| halo_testkit::OPENCODE_VERSION.to_string());
        println!("{version}");
        return;
    }
    let serve = match parse_serve_args(&args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("fake-opencode: {msg}");
            std::process::exit(2);
        }
    };
    if serve.hostname != "127.0.0.1" {
        eprintln!("fake-opencode: 只允许绑定 127.0.0.1");
        std::process::exit(2);
    }
    let (hostname, port) = (serve.hostname, serve.port);
    let mode = serve
        .mode
        .or_else(|| std::env::var("FAKE_OC_MODE").ok())
        .unwrap_or_else(|| "happy".to_string());
    let token = std::env::var("HALO_OC_TOKEN").ok();
    if let (Some(path), Some(t)) = (&serve.token_digest_file, &token) {
        // 只落 SHA-256 摘要，明文 token 绝不写入任何文件
        let digest = {
            let mut h = Sha256::new();
            h.update(t.as_bytes());
            let out = h.finalize();
            out.iter().fold(String::with_capacity(64), |mut s, b| {
                use std::fmt::Write as _;
                let _ = write!(s, "{b:02x}");
                s
            })
        };
        let write_result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| writeln!(f, "{digest}"));
        if let Err(e) = write_result {
            eprintln!("fake-opencode: 写入 token 摘要文件失败：{e}");
            std::process::exit(1);
        }
    }
    let expected_auth = token.map(|t| format!("Bearer {t}"));

    let server = match Server::http((hostname.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fake-opencode: 绑定 127.0.0.1:{port} 失败：{e}");
            std::process::exit(1);
        }
    };

    if mode == "exit_early" {
        // 模拟服务在启动后不久非预期死亡
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(1);
        });
    }

    let state: Arc<Mutex<TaskShared>> = Arc::new(Mutex::new(TaskShared::default()));

    // 每个请求独立线程处理：/events 长轮询期间 /cancel、/shutdown 仍须可达
    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let mode = mode.clone();
        let expected_auth = expected_auth.clone();
        std::thread::spawn(move || handle(request, &mode, expected_auth.as_deref(), &state));
    }
}

fn parse_serve_args(args: &[String]) -> Result<ServeArgs, String> {
    if args.first().map(String::as_str) != Some("serve") {
        return Err(
            "用法：fake-opencode --version | fake-opencode serve --hostname 127.0.0.1 --port <n> [--mode <m>] [--token-digest-file <path>]"
                .to_string(),
        );
    }
    let mut hostname: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut mode: Option<String> = None;
    let mut token_digest_file: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--hostname" => {
                i += 1;
                hostname = args.get(i).cloned();
            }
            "--port" => {
                i += 1;
                port = args.get(i).and_then(|p| p.parse().ok());
            }
            "--mode" => {
                i += 1;
                mode = args.get(i).cloned();
            }
            "--token-digest-file" => {
                i += 1;
                token_digest_file = args.get(i).cloned();
            }
            _ => {}
        }
        i += 1;
    }
    match (hostname, port) {
        (Some(h), Some(p)) => Ok(ServeArgs {
            hostname: h,
            port: p,
            mode,
            token_digest_file,
        }),
        _ => Err("缺少 --hostname 或 --port 参数".to_string()),
    }
}

fn handle(request: Request, mode: &str, expected_auth: Option<&str>, state: &Arc<Mutex<TaskShared>>) {
    let authorized = match expected_auth {
        Some(expect) => request
            .headers()
            .iter()
            .any(|h| h.field.equiv("Authorization") && h.value.as_str() == expect),
        // HALO_OC_TOKEN 未设置时没有任何合法 token：失败关闭
        None => false,
    };
    if !authorized || mode == "bad_token" {
        respond_json(request, 401, &json!({"error": "unauthorized"}));
        return;
    }

    let url = request.url().to_string();
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url, String::new()),
    };
    let method = request.method().clone();

    match (method, path.as_str()) {
        (Method::Get, "/health") => {
            if mode == "unhealthy" {
                respond_json(request, 500, &json!({"status": "error"}));
            } else {
                respond_json(request, 200, &json!({"status": "ok"}));
            }
        }
        (Method::Get, "/version") => {
            let v = if mode == "wrong_version" {
                halo_testkit::OPENCODE_WRONG_VERSION
            } else {
                halo_testkit::OPENCODE_VERSION
            };
            respond_json(request, 200, &json!({"version": v}));
        }
        (Method::Post, "/task") => {
            {
                let mut s = lock(state);
                *s = TaskShared::default();
            }
            let st = Arc::clone(state);
            std::thread::spawn(move || run_happy_task(&st));
            respond_json(request, 200, &json!({"accepted": true}));
        }
        (Method::Get, "/events") => {
            let after = parse_after(&query);
            let snapshot = wait_events(state, after);
            respond_json(request, 200, &snapshot);
        }
        (Method::Post, "/cancel") => {
            lock(state).cancel_requested = true;
            respond_json(request, 200, &json!({"accepted": true}));
        }
        (Method::Post, "/shutdown") => {
            respond_json(request, 200, &json!({"accepted": true}));
            if mode != "hang_on_shutdown" {
                // 留出把响应写回套接字的时间再优雅退出
                std::thread::sleep(Duration::from_millis(50));
                std::process::exit(0);
            }
        }
        _ => respond_json(request, 404, &json!({"error": "not_found"})),
    }
}

fn run_happy_task(state: &Arc<Mutex<TaskShared>>) {
    for step in halo_testkit::happy_script() {
        if lock(state).cancel_requested {
            let mut s = lock(state);
            s.done = true;
            s.outcome = Some("cancelled".to_string());
            return;
        }
        match step {
            ScriptStep::Emit(item) => lock(state).events.push(item),
            ScriptStep::WriteAgentFile => {
                if let Err(e) = halo_testkit::write_agent_file() {
                    eprintln!(
                        "fake-opencode: 写入 {} 失败：{e}",
                        halo_testkit::AGENT_FILE_NAME
                    );
                    let mut s = lock(state);
                    s.done = true;
                    s.outcome = Some("failed".to_string());
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut s = lock(state);
    s.done = true;
    s.outcome = Some("finished".to_string());
    s.summary = Some(halo_testkit::HAPPY_SUMMARY.to_string());
}

/// 长轮询：等到出现 after 之后的新事件或任务完成为止，最长约 10 秒。
fn wait_events(state: &Arc<Mutex<TaskShared>>, after: usize) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        {
            let s = lock(state);
            if s.done || s.events.len() > after {
                let events: Vec<Value> = s.events.iter().skip(after).cloned().collect();
                return json!({
                    "events": events,
                    "done": s.done,
                    "outcome": s.outcome,
                    "summary": s.summary,
                });
            }
        }
        if Instant::now() >= deadline {
            let s = lock(state);
            return json!({
                "events": [],
                "done": s.done,
                "outcome": s.outcome,
                "summary": s.summary,
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn parse_after(query: &str) -> usize {
    query
        .split('&')
        .find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            if k == "after" {
                v.parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn lock<'a>(state: &'a Arc<Mutex<TaskShared>>) -> MutexGuard<'a, TaskShared> {
    match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn respond_json(request: Request, status: u16, body: &Value) {
    let mut response = Response::from_data(body.to_string().into_bytes()).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
}
