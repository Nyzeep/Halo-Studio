//! fake-opencode：仅用于 Sidecar 集成测试的 OpenCode 1.x Server 替身。
//!
//! 它严格模拟本票的公开 Server 边界：`serve --hostname 127.0.0.1 --port <n>`、
//! `OPENCODE_SERVER_PASSWORD` Basic 认证、`GET /global/health`、
//! `POST /global/dispose`。不存在旧 `/task`、`/events`、`/cancel` 或 `/shutdown`。

use std::fs::{File, OpenOptions};
use std::io::Write as _;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server};

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
    emit_listening_line(&mode, serve.port);

    if mode == "exit_early" {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(1);
        });
    }

    for request in server.incoming_requests() {
        handle(
            request,
            &mode,
            expected_auth.as_deref(),
            serve.dispose_marker_file.as_deref(),
        );
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

fn handle(
    request: Request,
    mode: &str,
    expected_auth: Option<&str>,
    dispose_marker_file: Option<&str>,
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

    let path = request.url().to_string();
    match (request.method().clone(), path.as_str()) {
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
                _ => respond_json(request, 200, &json!({})),
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
