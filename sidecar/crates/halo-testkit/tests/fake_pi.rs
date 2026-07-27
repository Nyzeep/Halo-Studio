//! fake-pi 集成测试：spawn 真实 bin，按第 5 节 Pi 线协议交互。

mod support;

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::KillOnDrop;

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

struct PiProc {
    child: KillOnDrop,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
}

fn spawn_pi(mode: &str, dir: &std::path::Path) -> PiProc {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fake-pi"))
        .arg("--rpc")
        .env("FAKE_PI_MODE", mode)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 fake-pi 失败");
    let stdout = child.stdout.take().expect("缺少子进程 stdout");
    let stdin = child.stdin.take().expect("缺少子进程 stdin");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    PiProc {
        child: KillOnDrop::new(child),
        stdin: Some(stdin),
        rx,
    }
}

impl PiProc {
    fn send(&mut self, v: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin 已关闭");
        writeln!(stdin, "{v}").expect("写入 fake-pi stdin 失败");
        stdin.flush().expect("flush 失败");
    }

    fn recv_line(&self, timeout: Duration) -> Option<String> {
        self.rx.recv_timeout(timeout).ok()
    }

    fn recv_json(&self, timeout: Duration) -> Value {
        let line = self.rx.recv_timeout(timeout).expect("等待 fake-pi 输出超时");
        serde_json::from_str(&line).expect("fake-pi 输出了非法 JSON 帧")
    }
}

/// 收集事件直到收到指定 id 的 result 帧。
fn collect_until_result(pi: &PiProc, id: i64) -> (Vec<Value>, Value) {
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let msg = pi.recv_json(RECV_TIMEOUT);
        if msg["method"] == "event" {
            events.push(msg["params"].clone());
            continue;
        }
        if msg["id"] == id {
            return (events, msg);
        }
    }
    panic!("超时未收到 run_task 结果");
}

fn run_task_request(id: i64) -> Value {
    json!({
        "id": id,
        "method": "run_task",
        "params": {"instructions": "写一个文件", "files": [], "base_diff": null, "notes": null}
    })
}

#[test]
fn version_flag_outputs_default_and_override() {
    let out = Command::new(env!("CARGO_BIN_EXE_fake-pi"))
        .arg("--version")
        .env_remove("FAKE_PI_VERSION")
        .output()
        .expect("运行 fake-pi --version 失败");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1.4.0");

    let out = Command::new(env!("CARGO_BIN_EXE_fake-pi"))
        .arg("--version")
        .env("FAKE_PI_VERSION", "7.8.9")
        .output()
        .expect("运行 fake-pi --version 失败");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7.8.9");
}

#[test]
fn happy_full_script() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("happy", dir.path());

    pi.send(&json!({"id": 1, "method": "get_state"}));
    let resp = pi.recv_json(RECV_TIMEOUT);
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["state"], "idle");

    pi.send(&run_task_request(2));
    let (events, result) = collect_until_result(&pi, 2);

    let phases: Vec<String> = events
        .iter()
        .filter(|e| e["kind"] == "phase")
        .map(|e| e["detail"]["phase"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(phases, ["planning", "editing", "verifying"]);
    assert!(events.iter().any(|e| e["kind"] == "agent_note"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "file_hint" && e["detail"]["path"] == "hello_from_agent.txt"));
    assert!(events
        .iter()
        .any(|e| e["kind"] == "verification" && e["detail"]["status"] == "passed"));
    assert_eq!(result["result"]["outcome"], "finished");
    let summary = result["result"]["summary"].as_str().unwrap_or("");
    assert!(!summary.is_empty(), "result 必须携带非空 summary");

    let content = std::fs::read_to_string(dir.path().join("hello_from_agent.txt"))
        .expect("happy 模式必须真实写入文件");
    assert_eq!(content, "hello from agent");

    // 关闭 stdin（EOF）后 fake-pi 应自行退出
    drop(pi.stdin.take());
    assert!(
        pi.child.wait_exit(Duration::from_secs(5)).is_some(),
        "EOF 后 fake-pi 应退出"
    );
}

#[test]
fn not_ready_never_answers_get_state() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("not_ready", dir.path());
    pi.send(&json!({"id": 1, "method": "get_state"}));
    assert!(
        pi.recv_line(Duration::from_millis(800)).is_none(),
        "not_ready 模式不应回应 get_state"
    );
    assert!(pi.child.try_running(), "not_ready 模式进程应保持存活");
}

#[test]
fn garbage_mode_outputs_bad_frames() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("garbage", dir.path());
    let line = pi.recv_line(RECV_TIMEOUT).expect("garbage 模式应输出坏帧");
    assert!(
        serde_json::from_str::<Value>(&line).is_err(),
        "garbage 模式输出了合法 JSON：{line}"
    );

    // 请求也只会得到坏帧
    pi.send(&json!({"id": 1, "method": "get_state"}));
    let mut got_bad_reply = false;
    while let Some(l) = pi.recv_line(Duration::from_millis(800)) {
        if serde_json::from_str::<Value>(&l).is_err() {
            got_bad_reply = true;
            break;
        }
    }
    assert!(got_bad_reply, "garbage 模式对请求也应输出坏帧");
}

#[test]
fn crash_mid_task_exits_with_code_3() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("crash_mid_task", dir.path());

    pi.send(&json!({"id": 1, "method": "get_state"}));
    assert_eq!(pi.recv_json(RECV_TIMEOUT)["result"]["state"], "idle");

    pi.send(&run_task_request(2));
    // 崩溃前应至少产出一条事件
    let first = pi.recv_json(RECV_TIMEOUT);
    assert_eq!(first["method"], "event");

    let status = pi
        .child
        .wait_exit(Duration::from_secs(5))
        .expect("crash_mid_task 应在任务中途退出");
    assert_eq!(status.code(), Some(3));
}

#[test]
fn action_request_mode_emits_permission_request_then_finishes() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("action_request", dir.path());

    pi.send(&json!({"id": 1, "method": "get_state"}));
    assert_eq!(pi.recv_json(RECV_TIMEOUT)["result"]["state"], "idle");

    pi.send(&run_task_request(2));
    let (events, result) = collect_until_result(&pi, 2);

    assert!(
        events
            .iter()
            .any(|e| e["kind"] == "action_request" && e["detail"]["kind"] == "permission"),
        "应出现 kind=permission 的 action_request"
    );
    assert_eq!(result["result"]["outcome"], "finished");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("hello_from_agent.txt")).unwrap_or_default(),
        "hello from agent"
    );
}

#[test]
fn verify_fail_mode_reports_failed_verification_but_finishes() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("verify_fail", dir.path());

    pi.send(&json!({"id": 1, "method": "get_state"}));
    assert_eq!(pi.recv_json(RECV_TIMEOUT)["result"]["state"], "idle");

    pi.send(&run_task_request(2));
    let (events, result) = collect_until_result(&pi, 2);

    assert!(events
        .iter()
        .any(|e| e["kind"] == "verification" && e["detail"]["status"] == "failed"));
    assert!(
        !events
            .iter()
            .any(|e| e["kind"] == "verification" && e["detail"]["status"] == "passed"),
        "verify_fail 模式不应出现 passed 验证结论"
    );
    assert_eq!(result["result"]["outcome"], "finished");
}

#[test]
fn hang_on_cancel_ignores_cancel_and_must_be_killed() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut pi = spawn_pi("hang_on_cancel", dir.path());

    pi.send(&json!({"id": 1, "method": "get_state"}));
    assert_eq!(pi.recv_json(RECV_TIMEOUT)["result"]["state"], "idle");

    pi.send(&run_task_request(2));
    let first = pi.recv_json(RECV_TIMEOUT);
    assert_eq!(first["method"], "event");

    pi.send(&json!({"id": 3, "method": "cancel"}));
    std::thread::sleep(Duration::from_millis(600));

    assert!(
        pi.recv_line(Duration::from_millis(200)).is_none(),
        "hang_on_cancel 不应回应 cancel，也不应给出 run_task 结果"
    );
    assert!(pi.child.try_running(), "hang_on_cancel 模式不应自行退出");

    // 即便关闭 stdin（EOF），仍应保持挂起，只有强杀才能结束
    drop(pi.stdin.take());
    std::thread::sleep(Duration::from_millis(400));
    assert!(pi.child.try_running(), "EOF 后 hang_on_cancel 仍应挂起");

    pi.child.kill_now();
    assert!(
        pi.child.wait_exit(Duration::from_secs(5)).is_some(),
        "强杀后进程应结束"
    );
}
