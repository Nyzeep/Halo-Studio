//! 集成测试装配：定位真实二进制、临时 Git 工作区、以 stdio JSONL 驱动真实
//! halo-sidecar 子进程并全程记录收发行。
//!
//! 纪律：这里只做驱动与观察，不复刻任何生产逻辑；断言一律面向契约可观察行为。
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// 单次等待（响应/事件/进程退出）的统一上限。
pub const WAIT: Duration = Duration::from_secs(30);
const CREDENTIAL_MANAGER_TEST_LOCK_TIMEOUT: Duration = Duration::from_secs(300);

const CREDENTIAL_MANAGER_TEST_LOCK_FILE: &str = "halo-studio-credential-manager-integration.lock";

/// Cargo runs integration-test binaries concurrently, while Windows Credential Manager
/// does not guarantee ordering for near-simultaneous mutations. Keep the real store
/// scenarios serialized across processes without changing the production adapter.
pub struct CredentialManagerTestGuard {
    lock_file: Option<std::fs::File>,
    path: PathBuf,
}

pub fn lock_credential_manager_for_test() -> CredentialManagerTestGuard {
    let path = std::env::temp_dir().join(CREDENTIAL_MANAGER_TEST_LOCK_FILE);
    let deadline = Instant::now() + CREDENTIAL_MANAGER_TEST_LOCK_TIMEOUT;
    loop {
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(windows)]
        options.share_mode(0);

        match options.open(&path) {
            Ok(lock_file) => {
                return CredentialManagerTestGuard {
                    lock_file: Some(lock_file),
                    path,
                };
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    || matches!(error.raw_os_error(), Some(32) | Some(33)) =>
            {
                if Instant::now() >= deadline {
                    panic!("等待 Windows 凭据集成测试锁超时");
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => panic!("无法建立 Windows 凭据集成测试锁"),
        }
    }
}

impl Drop for CredentialManagerTestGuard {
    fn drop(&mut self) {
        drop(self.lock_file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 安装仅供当前集成测试使用的凭据引用。真实 Windows 凭据库不可用时返回 None，
/// 调用方应跳过必须经过正向凭据注入的场景，而不是建立生产回退。
pub struct TestCredentialGuard {
    reference: String,
    _credential_manager_guard: CredentialManagerTestGuard,
}

impl TestCredentialGuard {
    pub fn reference(&self) -> &str {
        &self.reference
    }
}

impl Drop for TestCredentialGuard {
    fn drop(&mut self) {
        if let Ok(entry) = keyring::Entry::new("HaloStudio", &self.reference) {
            let _ = entry.delete_credential();
        }
    }
}

pub fn install_test_credential() -> Option<TestCredentialGuard> {
    use halo_config::{CredentialStore, Secret, WindowsCredentialStore};

    let credential_manager_guard = lock_credential_manager_for_test();
    let store = WindowsCredentialStore::new();
    if !store.available() {
        return None;
    }

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟应晚于 Unix epoch")
        .as_nanos();
    let reference = format!("halo/integration/opencode-{}-{nonce}", std::process::id());
    let secret = Secret::new(format!(
        "integration-credential-{}-{nonce}",
        std::process::id()
    ));
    store
        .set(&reference, &secret)
        .expect("可用的系统凭据库应能写入集成测试引用");
    Some(TestCredentialGuard {
        reference,
        _credential_manager_guard: credential_manager_guard,
    })
}

/// OpenCode 正向集成覆盖必须经过真实 Windows 凭据库。不可用不是通过条件，
/// 以明确失败暴露测试环境缺失，避免把未执行的正向覆盖记为通过。
pub fn require_test_credential() -> TestCredentialGuard {
    install_test_credential()
        .expect("Windows 凭据管理器不可用；OpenCode 正向启动集成测试必须在可写系统凭据存储环境运行")
}

// ---------- 二进制定位 ----------

/// 从 std::env::current_exe() 上溯到 target\debug 目录拼接真实二进制文件名。
pub fn bin_path(name: &str) -> PathBuf {
    let mut p = std::env::current_exe().expect("无法获取当前测试可执行文件路径");
    p.pop();
    if p.file_name().map(|n| n == "deps").unwrap_or(false) {
        p.pop();
    }
    let exe = p.join(format!("{name}.exe"));
    assert!(
        exe.exists(),
        "缺少集成测试所需真实二进制：{}（请先在 sidecar 目录 cargo build --workspace）",
        exe.display()
    );
    exe
}

pub fn sidecar_exe() -> PathBuf {
    bin_path("halo-sidecar")
}

pub fn fake_pi_exe() -> PathBuf {
    bin_path("fake-pi")
}

pub fn fake_opencode_exe() -> PathBuf {
    bin_path("fake-opencode")
}

/// 将测试脚本参数写到 fake 二进制同名旁路文件。生产 `LaunchConfig` 不再传递
/// 任意参数或环境覆盖，测试仍可在不扩大生产接口的前提下构造故障场景。
///
/// 每个数据目录下都有独立的测试目录，避免并行测试相互覆盖脚本；它与数据目录
/// 共同存活，因此“中断后重启”场景中的持久化配置仍能找到其测试替身。
fn fake_variant(executable: &Path, args: &[&str], variant_dir: &Path) -> PathBuf {
    if args.is_empty() {
        return executable.to_path_buf();
    }
    let serialized = serde_json::to_vec(args).expect("测试脚本参数必须可序列化");
    let digest = Sha256::digest(&serialized);
    let suffix: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let stem = executable
        .file_stem()
        .and_then(|name| name.to_str())
        .expect("fake 可执行文件应有有效名称");
    let extension = executable
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("exe");
    let variant = variant_dir.join(format!("{stem}-{suffix}.{extension}"));
    if !variant.exists() {
        std::fs::copy(executable, &variant).expect("无法创建 fake 可执行文件变体");
    }
    std::fs::write(variant.with_extension("args.json"), serialized)
        .expect("无法写入 fake 测试脚本参数");
    variant
}

// ---------- 临时 Git 工作区 ----------

pub fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git 不可用");
    assert!(
        out.status.success(),
        "git {:?} 失败：{}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 运行 git 并捕获 stdout（断言成功）；用于记录 HEAD/status 等仓库状态快照。
pub fn git_capture(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git 不可用");
    assert!(
        out.status.success(),
        "git {:?} 失败：{}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 独立临时 Git 工作区：路径含空格与中文；git init + 初始提交 + 预置脏文件
/// （一个已跟踪文件的未提交修改 + 一个未跟踪文件）。
pub struct TestRepo {
    pub root: PathBuf,
    _dir: tempfile::TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let root = dir.path().join("集成 工作区");
        std::fs::create_dir_all(&root).expect("创建工作区目录失败");
        git(&root, &["init", "-b", "main"]);
        std::fs::write(root.join("base.txt"), "基线内容\n").expect("写基线文件失败");
        std::fs::write(root.join("tracked_dirty.txt"), "初始内容\n").expect("写文件失败");
        git(&root, &["add", "-A"]);
        git(
            &root,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-m",
                "init",
                "--no-gpg-sign",
            ],
        );
        // 任务基线前已有修改：这些文件永不归因 Agent
        std::fs::write(root.join("tracked_dirty.txt"), "任务前的本地修改\n").expect("写文件失败");
        std::fs::write(root.join("untracked_dirty.txt"), "任务前的未跟踪文件\n")
            .expect("写文件失败");
        TestRepo { root, _dir: dir }
    }

    pub fn path_str(&self) -> String {
        self.root.to_string_lossy().to_string()
    }
}

// ---------- Sidecar 子进程驱动 ----------

/// 真实 halo-sidecar 子进程：stdin 写请求、后台线程读 stdout，
/// 响应按 id 归档、事件按到达顺序归档，全部收发行进入 transcript。
pub struct Sidecar {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    next_id: AtomicU64,
    /// ">> 行"（发出）与 "<< 行"（收到）的完整记录
    pub transcript: Arc<Mutex<Vec<String>>>,
    responses: Arc<Mutex<HashMap<String, Value>>>,
    pub events: Arc<Mutex<Vec<Value>>>,
    pub data_dir: PathBuf,
    _data_tmp: Option<tempfile::TempDir>,
    fake_variant_dir: PathBuf,
}

impl Sidecar {
    /// 以独立临时 HALO_DATA_DIR 启动（数据目录路径同样含空格与中文）。
    pub fn start(extra_env: &[(&str, &str)]) -> Sidecar {
        let tmp = tempfile::tempdir().expect("创建数据目录失败");
        let data_dir = tmp.path().join("halo 数据");
        Self::spawn(data_dir, Some(tmp), extra_env)
    }

    /// 复用既有数据目录重启（中断恢复 / 追加证据场景）。
    pub fn start_with_data_dir(data_dir: &Path, extra_env: &[(&str, &str)]) -> Sidecar {
        Self::spawn(data_dir.to_path_buf(), None, extra_env)
    }

    fn spawn(
        data_dir: PathBuf,
        data_tmp: Option<tempfile::TempDir>,
        extra_env: &[(&str, &str)],
    ) -> Sidecar {
        let fake_variant_dir = data_dir.join("test-fake-variants");
        std::fs::create_dir_all(&fake_variant_dir).expect("创建 fake 测试目录失败");
        let mut cmd = Command::new(sidecar_exe());
        cmd.env("HALO_DATA_DIR", &data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("启动 halo-sidecar 失败");
        let stdin = child.stdin.take().expect("缺少 sidecar stdin");
        let stdout = child.stdout.take().expect("缺少 sidecar stdout");

        let transcript: Arc<Mutex<Vec<String>>> = Arc::default();
        let responses: Arc<Mutex<HashMap<String, Value>>> = Arc::default();
        let events: Arc<Mutex<Vec<Value>>> = Arc::default();
        {
            let transcript = Arc::clone(&transcript);
            let responses = Arc::clone(&responses);
            let events = Arc::clone(&events);
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(_) => break,
                    };
                    transcript.lock().unwrap().push(format!("<< {line}"));
                    let v: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    match v.get("kind").and_then(Value::as_str) {
                        Some("response") => {
                            if let Some(id) = v.get("id").and_then(Value::as_str) {
                                responses.lock().unwrap().insert(id.to_string(), v.clone());
                            }
                        }
                        Some("event") => events.lock().unwrap().push(v),
                        _ => {}
                    }
                }
            });
        }
        Sidecar {
            child: Some(child),
            stdin: Some(stdin),
            next_id: AtomicU64::new(1),
            transcript,
            responses,
            events,
            data_dir,
            _data_tmp: data_tmp,
            fake_variant_dir,
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.as_ref().expect("sidecar 已回收").id()
    }

    /// 发送一行原始文本（协议边界测试用）。
    pub fn send_raw(&mut self, line: &str) {
        self.transcript.lock().unwrap().push(format!(">> {line}"));
        let stdin = self.stdin.as_mut().expect("sidecar stdin 已关闭");
        writeln!(stdin, "{line}").expect("写入 sidecar stdin 失败");
        stdin.flush().expect("flush sidecar stdin 失败");
    }

    /// 发出请求并等待同 id 响应（完整 Response 封包）。
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        self.request_with_timeout(method, params, WAIT)
    }

    pub fn request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Value {
        let id = format!("r-it-{:05}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let line = json!({
            "v": 1, "kind": "request", "id": id, "method": method, "params": params
        })
        .to_string();
        self.send_raw(&line);
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(resp) = self.responses.lock().unwrap().remove(&id) {
                return resp;
            }
            assert!(
                Instant::now() < deadline,
                "等待 {method} 响应超时；已收到 {} 条协议消息",
                self.transcript.lock().unwrap().len()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// 期望成功，返回 result。
    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        let resp = self.request(method, params);
        assert_eq!(
            resp["ok"],
            true,
            "{method} 应成功（{}）",
            response_summary(&resp)
        );
        resp["result"].clone()
    }

    pub fn ok_with_timeout(&mut self, method: &str, params: Value, timeout: Duration) -> Value {
        let resp = self.request_with_timeout(method, params, timeout);
        assert_eq!(
            resp["ok"],
            true,
            "{method} 应成功（{}）",
            response_summary(&resp)
        );
        resp["result"].clone()
    }

    /// 期望失败，返回 error（断言 code）。
    pub fn err(&mut self, method: &str, params: Value, expect_code: &str) -> Value {
        let resp = self.request(method, params);
        assert_eq!(
            resp["ok"],
            false,
            "{method} 应失败（{}）",
            response_summary(&resp)
        );
        let error = resp["error"].clone();
        assert_eq!(
            error["code"],
            expect_code,
            "{method} 错误码不符（{}）",
            response_summary(&resp)
        );
        error
    }

    /// 等待满足谓词的事件（含已收到的历史事件）。
    pub fn wait_event(&self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        self.wait_event_with_timeout(what, WAIT, pred)
    }

    pub fn wait_event_with_timeout(
        &self,
        what: &str,
        timeout: Duration,
        pred: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        let mut cursor = 0usize;
        loop {
            {
                let events = self.events.lock().unwrap();
                while cursor < events.len() {
                    if pred(&events[cursor]) {
                        return events[cursor].clone();
                    }
                    cursor += 1;
                }
            }
            assert!(
                Instant::now() < deadline,
                "等待事件超时：{what}；已收到 {} 个事件和 {} 条协议消息",
                self.events.lock().unwrap().len(),
                self.transcript.lock().unwrap().len()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn events_snapshot(&self) -> Vec<Value> {
        self.events.lock().unwrap().clone()
    }

    pub fn transcript_snapshot(&self) -> Vec<String> {
        self.transcript.lock().unwrap().clone()
    }

    /// 关闭 stdin（模拟 UI 正常退出）并等待进程退出。
    pub fn shutdown(&mut self) -> ExitStatus {
        drop(self.stdin.take());
        let child = self.child.as_mut().expect("sidecar 已回收");
        let deadline = Instant::now() + WAIT;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return status;
            }
            assert!(Instant::now() < deadline, "sidecar 未在限时内退出");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// 直接强杀（中断恢复场景：模拟应用崩溃）。
    pub fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            terminate_process_tree(child);
        }
    }

    // ---------- 常用契约流程 ----------

    pub fn hello(&mut self) -> Value {
        let result = self.ok(
            "sidecar.hello",
            json!({"app_protocol_versions": [1], "app_version": "0.1.0"}),
        );
        assert_eq!(result["protocol_version"], 1);
        result
    }

    /// workspace.open + trust，返回 workspace_id。
    pub fn open_and_trust(&mut self, path: &str) -> String {
        let ws = self.ok("workspace.open", json!({"path": path}));
        assert_eq!(ws["active"], true);
        let id = ws["workspace_id"]
            .as_str()
            .expect("缺少 workspace_id")
            .to_string();
        let ws = self.ok(
            "workspace.trust",
            json!({"workspace_id": id, "decision": "trust"}),
        );
        assert_eq!(ws["trust"], "trusted");
        id
    }

    /// config.save，返回 config_id。
    pub fn save_config(
        &mut self,
        agent: &str,
        exe: &Path,
        test_harness_args: &[&str],
        credential_ref: Option<&str>,
    ) -> String {
        let executable = fake_variant(exe, test_harness_args, &self.fake_variant_dir);
        let result = self.ok(
            "config.save",
            json!({
                "name": format!("{agent} 集成配置"),
                "agent": agent,
                "executable_path": executable.to_string_lossy(),
                "model": if agent == "opencode" { "openai/gpt-5" } else { "gpt-5" },
                "thinking_level": "medium",
                "credential_ref": credential_ref
            }),
        );
        result["config"]["config_id"]
            .as_str()
            .expect("缺少 config_id")
            .to_string()
    }

    /// runtime.start（期望 ready）。
    pub fn start_runtime(&mut self, agent: &str, config_id: &str) {
        let result = self.ok(
            "runtime.start",
            json!({"agent": agent, "config_id": config_id}),
        );
        assert_eq!(result["state"], "ready", "runtime.start 应就绪：{result}");
    }

    /// task.create，返回 task_id。
    pub fn create_task(&mut self, agent: &str, config_id: &str, title: &str) -> String {
        self.create_task_with_timeout(agent, config_id, title, WAIT)
    }

    pub fn create_task_with_timeout(
        &mut self,
        agent: &str,
        config_id: &str,
        title: &str,
        timeout: Duration,
    ) -> String {
        let result = self.ok_with_timeout(
            "task.create",
            json!({
                "agent": agent,
                "config_id": config_id,
                "title": title,
                "instructions": "在工作区写入 hello_from_agent.txt",
                "files": [],
                "base_diff": null,
                "notes": null
            }),
            timeout,
        );
        assert_eq!(result["task"]["state"], "running");
        result["task"]["task_id"]
            .as_str()
            .expect("缺少 task_id")
            .to_string()
    }

    /// 等待 task.finished 事件并返回其 payload。
    pub fn wait_task_finished(&self, task_id: &str) -> Value {
        let ev = self.wait_event("task.finished", |e| {
            e["event"] == "task.finished" && e["task_id"] == task_id
        });
        ev["payload"].clone()
    }
}

/// 失败信息只保留协议结构，避免测试基础设施在产品回归时把响应正文中的敏感值
/// 回显到测试输出。
fn response_summary(response: &Value) -> String {
    let ok = response.get("ok").and_then(Value::as_bool);
    let has_error = response.get("error").is_some();
    format!("ok={ok:?}, has_error={has_error}")
}

fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        // 用例失败也不泄漏子进程
        drop(self.stdin.take());
        if let Some(child) = self.child.as_mut() {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                if Instant::now() >= deadline {
                    terminate_process_tree(child);
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

// ---------- 进程与文件观察 ----------

fn can_open_exclusively(path: &Path) -> bool {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    options.share_mode(0);
    options.open(path).is_ok()
}

pub fn wait_process_lock_held(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() && !can_open_exclusively(path) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn wait_process_lock_released(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() && can_open_exclusively(path) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 递归收集目录下全部文件路径。
pub fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// 文本中是否存在长度 >= len 的连续小写十六进制串（token 形态探测）。
pub fn contains_lower_hex_run(text: &str, len: usize) -> bool {
    let mut run = 0usize;
    for c in text.chars() {
        if c.is_ascii_digit() || ('a'..='f').contains(&c) {
            run += 1;
            if run >= len {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}
