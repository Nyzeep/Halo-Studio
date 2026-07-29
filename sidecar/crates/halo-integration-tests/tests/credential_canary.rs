//! 场景 9：凭据 canary 全链路。
//! 用 `halo-sidecar cred set` 写入随机 canary 值 → 配置引用它 → 跑一次 happy 全链路。
//! 断言：canary 明文不出现在全部 IPC 收发行、HALO_DATA_DIR 下所有文件字节、
//! 评审 diff 与交接包 JSON 中；同时 fake-pi 侧证明凭据环境变量注入真实发生
//! （trace 只记录“存在性”，不记录值）。

mod support;

use std::io::Write;
use std::process::{Command, Stdio};

use halo_config::{CredentialStore, WindowsCredentialStore};
use serde_json::json;
use support::{
    fake_pi_exe, lock_credential_manager_for_test, sidecar_exe, walk_files, Sidecar, TestRepo,
};

/// 测试结束（含失败路径）后清理写入 Windows 凭据管理器的条目。
/// halo-sidecar CLI 没有删除子命令，故经 keyring 直接删除（service 与生产一致）。
struct CanaryGuard {
    reference: String,
}

impl Drop for CanaryGuard {
    fn drop(&mut self) {
        if let Ok(entry) = keyring::Entry::new("HaloStudio", &self.reference) {
            let _ = entry.delete_credential();
        }
    }
}

#[test]
fn credential_canary_never_leaks_across_full_chain() {
    // 随机 canary：值只存在于凭据存储与被注入的子进程环境中
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("时钟异常")
        .as_nanos();
    let canary_ref = format!(
        "halo/integration/opencode-canary-{}-{nonce}",
        std::process::id()
    );
    let canary = format!("canary-secret-{:x}-{:x}", std::process::id(), nonce);
    let _credential_manager_guard = lock_credential_manager_for_test();
    let _guard = CanaryGuard {
        reference: canary_ref.clone(),
    };

    if !WindowsCredentialStore::new().available() {
        let mut cred_set = Command::new(sidecar_exe())
            .args(["cred", "set", canary_ref.as_str()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("启动 cred set 失败");
        cred_set
            .stdin
            .take()
            .expect("缺少 stdin")
            .write_all(format!("{canary}\n").as_bytes())
            .expect("写入密钥失败");
        let out = cred_set.wait_with_output().expect("cred set 未退出");
        assert!(!out.status.success(), "不可用的系统凭据存储必须失败关闭");
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!output.contains(&canary), "失败路径不得回显凭据明文");
        return;
    }

    // 经 Sidecar CLI 从 stdin 录入（凭据红线：不走命令行参数，不回显内容）
    let mut cred_set = Command::new(sidecar_exe())
        .args(["cred", "set", canary_ref.as_str()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 cred set 失败");
    cred_set
        .stdin
        .take()
        .expect("缺少 stdin")
        .write_all(format!("{canary}\n").as_bytes())
        .expect("写入密钥失败");
    let out = cred_set.wait_with_output().expect("cred set 未退出");
    assert!(out.status.success(), "cred set 应成功");
    let echoed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!echoed.contains(&canary), "cred set 输出不得回显密钥");

    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());

    let check = sc.ok(
        "config.credential_check",
        json!({"credential_ref": canary_ref}),
    );
    assert_eq!(check["exists"], true);
    assert_eq!(check["store_available"], true);

    // 配置引用 canary，并让 fake-pi 汇报“凭据环境变量是否存在”（只写存在性）
    let cfg = sc.save_config(
        "pi",
        &fake_pi_exe(),
        &["--report-env", "HALO_PROVIDER_API_KEY"],
        Some(canary_ref.as_str()),
    );
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "凭据注入 canary 任务");
    assert_eq!(sc.wait_task_finished(&task_id)["outcome"], "finished");

    // 注入真实发生：fake-pi 在其真实子进程环境中看到了凭据变量
    let note = sc.wait_event("凭据存在性 trace", |e| {
        e["event"] == "trace.item"
            && e["task_id"] == task_id.as_str()
            && e["payload"]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("HALO_PROVIDER_API_KEY 存在=")
    });
    assert!(
        note["payload"]["text"]
            .as_str()
            .unwrap_or_default()
            .ends_with("存在=true"),
        "凭据环境变量应真实注入"
    );

    // 评审 diff 与交接包 JSON 均不含 canary
    let bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert!(!bundle.to_string().contains(&canary), "评审证据泄漏 canary");
    let preview = sc.ok("handoff.preview", json!({"task_id": task_id}));
    assert!(!preview.to_string().contains(&canary), "交接包泄漏 canary");
    let created = sc.ok(
        "handoff.create",
        json!({"task_id": task_id, "target_agent": "opencode", "selected_files": []}),
    );
    assert!(!created.to_string().contains(&canary), "交接包泄漏 canary");

    // 全部 IPC 收发行不含 canary
    for line in sc.transcript_snapshot() {
        assert!(!line.contains(&canary), "IPC 行泄漏 canary");
    }

    // 关闭后扫描 HALO_DATA_DIR 全部文件字节（含 SQLite 主库与日志文件）
    let data_dir = sc.data_dir.clone();
    let status = sc.shutdown();
    assert!(status.success());
    let mut files = Vec::new();
    walk_files(&data_dir, &mut files);
    assert!(!files.is_empty(), "数据目录应有持久化文件");
    let needle = canary.as_bytes();
    for file in files {
        let bytes =
            std::fs::read(&file).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", file.display()));
        let leaked = bytes.windows(needle.len()).any(|w| w == needle);
        assert!(!leaked, "数据文件泄漏 canary：{}", file.display());
    }

    // 工作区内的 Agent 产物同样不得含 canary
    let hello = std::fs::read_to_string(repo.root.join("hello_from_agent.txt")).expect("应有产物");
    assert!(!hello.contains(&canary));
}
