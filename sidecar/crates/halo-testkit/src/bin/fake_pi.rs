//! fake-pi：受控假 Pi 进程。
//! `--version` 输出 FAKE_PI_VERSION（默认 1.4.0）；`--rpc` 进入 stdio JSONL 循环。
//! 行为由 FAKE_PI_MODE 脚本化：happy | not_ready | garbage | crash_mid_task |
//! hang_on_cancel | action_request | verify_fail。
//!
//! Sidecar 以白名单环境启动子进程（宿主 FAKE_* 变量不会传入），因此同名脚本开关
//! 亦可经命令行参数注入（Sidecar 会把 LaunchConfig.extra_args 原样附加在 `--rpc` 后）：
//! `--mode <m>`（优先于 FAKE_PI_MODE）、`--step-delay-ms <n>`（脚本步进间隔，默认 10）、
//! `--report-env <VAR>`（happy 类脚本额外产出一条 agent_note，只写该环境变量的**存在性**，
//! 永不写值——供凭据注入 canary 测试断言注入真实发生）、`--pid-file <path>`（RPC 启动时
//! 写入自身 PID，供工作区切换测试断言旧运行时进程确实退出）、`--lock-file <path>`（独占
//! 打开文件直到进程退出，供强制终止测试通过资源释放而非进程枚举验收）。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, Write};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::time::Duration;

use serde_json::{json, Value};

use halo_testkit::ScriptStep;

/// 命令行/环境变量合并后的运行选项。
struct Options {
    mode: String,
    step_delay: Duration,
    report_env: Option<String>,
    pid_file: Option<String>,
    lock_file: Option<String>,
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        let version = std::env::var("FAKE_PI_VERSION")
            .unwrap_or_else(|_| halo_testkit::DEFAULT_PI_VERSION.to_string());
        println!("{version}");
        return;
    }
    if !args.iter().any(|a| a == "--rpc") {
        eprintln!("用法：fake-pi --version | fake-pi --rpc [--mode <m>] [--step-delay-ms <n>] [--report-env <VAR>] [--pid-file <path>]");
        std::process::exit(2);
    }
    let opts = Options {
        mode: arg_value(&args, "--mode")
            .or_else(|| std::env::var("FAKE_PI_MODE").ok())
            .unwrap_or_else(|| "happy".to_string()),
        step_delay: Duration::from_millis(
            arg_value(&args, "--step-delay-ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        ),
        report_env: arg_value(&args, "--report-env"),
        pid_file: arg_value(&args, "--pid-file"),
        lock_file: arg_value(&args, "--lock-file"),
    };
    if let Some(path) = &opts.pid_file {
        if let Err(e) = std::fs::write(path, std::process::id().to_string()) {
            eprintln!("fake-pi: 写入 PID 文件失败：{e}");
            std::process::exit(1);
        }
    }
    let _lock_file = match &opts.lock_file {
        Some(path) => Some(open_exclusive_lock(path).unwrap_or_else(|e| {
            eprintln!("fake-pi: 无法独占锁文件 {path}：{e}");
            std::process::exit(1);
        })),
        None => None,
    };
    rpc_loop(&opts);
}

fn open_exclusive_lock(path: &str) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(windows)]
    options.share_mode(0);
    options.open(path)
}

fn rpc_loop(opts: &Options) {
    let mode = opts.mode.as_str();
    if mode == "garbage" {
        // 一进入 RPC 就吐坏帧，让对端在任何请求前即可观察到协议损坏
        emit_line("{\"id\": 这不是合法 JSON");
        emit_line("%%% fake-pi garbage frame %%%");
    }
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        if mode == "garbage" {
            emit_line("!!! bad frame in reply !!!");
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        match msg.get("method").and_then(Value::as_str).unwrap_or("") {
            "get_state" => {
                if mode == "not_ready" {
                    // 永不应答，模拟就绪检查超时
                    continue;
                }
                respond(&id, json!({"state": "idle"}));
            }
            "run_task" => run_task(opts, &id),
            "cancel" => {
                if mode == "hang_on_cancel" {
                    // 忽略取消并继续挂起，验证监督方的强制终止路径
                    continue;
                }
                respond(&id, json!({"cancelled": true}));
            }
            _ => {}
        }
    }
    if mode == "hang_on_cancel" {
        // stdin EOF 后仍不退出：只有被强杀才能结束
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
}

fn run_task(opts: &Options, id: &Value) {
    // 只写环境变量存在性、绝不写值：凭据注入的证明不构成新的泄漏通道
    if let Some(var) = &opts.report_env {
        let present = std::env::var_os(var).is_some();
        emit_event(halo_testkit::trace_item(
            "agent_note",
            &format!("环境变量 {var} 存在={present}"),
            json!({}),
        ));
    }
    match opts.mode.as_str() {
        "crash_mid_task" => {
            emit_event(halo_testkit::trace_item(
                "phase",
                "规划中",
                json!({"phase": "planning"}),
            ));
            emit_event(halo_testkit::trace_item(
                "phase",
                "编辑中",
                json!({"phase": "editing"}),
            ));
            std::process::exit(3);
        }
        "hang_on_cancel" => {
            emit_event(halo_testkit::trace_item(
                "phase",
                "规划中",
                json!({"phase": "planning"}),
            ));
            // 不回 result，返回读循环继续吞掉后续 cancel
        }
        "verify_fail" => {
            execute(halo_testkit::verify_fail_script(), opts.step_delay);
            respond(
                id,
                json!({"outcome": "finished", "summary": "任务已结束，但验证未通过"}),
            );
        }
        "action_request" => {
            execute(halo_testkit::action_request_script(), opts.step_delay);
            respond(
                id,
                json!({"outcome": "finished", "summary": halo_testkit::HAPPY_SUMMARY}),
            );
        }
        _ => {
            execute(halo_testkit::happy_script(), opts.step_delay);
            respond(
                id,
                json!({"outcome": "finished", "summary": halo_testkit::HAPPY_SUMMARY}),
            );
        }
    }
}

fn execute(steps: Vec<ScriptStep>, delay: Duration) {
    for step in steps {
        match step {
            ScriptStep::Emit(item) => emit_event(item),
            ScriptStep::WriteAgentFile => {
                if let Err(e) = halo_testkit::write_agent_file() {
                    eprintln!("fake-pi: 写入 {} 失败：{e}", halo_testkit::AGENT_FILE_NAME);
                    std::process::exit(1);
                }
            }
        }
        std::thread::sleep(delay);
    }
}

fn emit_line(line: &str) {
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
        // 对端已关闭读端，继续运行没有意义
        std::process::exit(0);
    }
}

fn emit_json(v: &Value) {
    emit_line(&v.to_string());
}

fn respond(id: &Value, result: Value) {
    emit_json(&json!({"id": id, "result": result}));
}

fn emit_event(item: Value) {
    emit_json(&json!({"method": "event", "params": item}));
}
