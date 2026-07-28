//! 场景 2：OpenCode 1.x 受管启动闭环。
//! 校验真实回环服务、每次启动新 Basic 认证、兼容性档案和独立运行时状态；
//! 本票不得经旧 `/task` 协议伪造任务完成。

mod support;

use std::time::Duration;

use serde_json::json;
use support::{
    contains_lower_hex_run, fake_opencode_exe, require_test_credential, wait_process_lock_held,
    wait_process_lock_released, Sidecar, TestRepo,
};

#[test]
fn opencode_1x_starts_with_fresh_basic_authentication_and_keeps_pi_unprobed() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let digest_dir = tempfile::tempdir().expect("创建摘要目录失败");
    let digest_file = digest_dir.path().join("password 摘要.txt");
    let digest_arg = digest_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[("HALO_SHUTDOWN_GRACE_MS", "200")]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let config_id = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &[
            "--password-digest-file",
            &digest_arg,
            "--require-credential-env",
            "OPENAI_API_KEY",
            "--require-isolated-state",
        ],
        Some(credential.reference()),
    );

    sc.start_runtime("opencode", &config_id);
    sc.wait_event("runtime.state ready", |event| {
        event["event"] == "runtime.state"
            && event["payload"]["agent"] == "opencode"
            && event["payload"]["state"] == "ready"
    });

    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["opencode"]["state"], "ready");
    assert_eq!(status["opencode"]["version"], "1.18.5");
    assert_eq!(
        status["pi"]["state"], "not_probed",
        "Pi 状态必须独立如实呈现"
    );
    for agent in ["pi", "opencode"] {
        let info = status[agent].as_object().expect("runtime 状态应为对象");
        let mut keys: Vec<&str> = info.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["reason", "recovery_hint", "state", "version"]);
    }

    // 受管会话属于下一张票。当前应明确拒绝，而非调用旧 `/task` 伪造在线完成。
    let error = sc.err(
        "task.create",
        json!({
            "agent": "opencode", "config_id": config_id, "title": "不应伪造任务",
            "instructions": "不能调用旧协议"
        }),
        "RUNTIME_CAPABILITY_UNAVAILABLE",
    );
    assert!(error["message"]
        .as_str()
        .unwrap_or_default()
        .contains("真实会话尚未接入"));

    let stopped = sc.ok("runtime.stop", json!({"agent": "opencode"}));
    assert_eq!(stopped["state"], "stopped");
    sc.start_runtime("opencode", &config_id);

    let digests: Vec<String> = std::fs::read_to_string(&digest_file)
        .expect("fake-opencode 应已写入认证摘要")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(digests.len(), 2, "两次启动应各记录一个密码摘要");
    assert_ne!(digests[0], digests[1], "每次启动必须生成新的 Basic 密码");
    for digest in &digests {
        assert_eq!(digest.len(), 64);
        assert!(digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
    }

    // 公开 IPC 不得暴露认证变量、端口或 Basic Authorization 内容。
    for line in sc.transcript_snapshot() {
        assert!(
            !line.contains("OPENCODE_SERVER_PASSWORD"),
            "IPC 泄漏认证变量名"
        );
        assert!(!line.contains("Authorization"), "IPC 泄漏认证头");
        assert!(!line.contains("\"port\""), "IPC 泄漏回环端口字段");
        assert!(
            !contains_lower_hex_run(&line, 64),
            "IPC 疑似泄漏本次 OpenCode 认证信息"
        );
    }

    assert!(sc.shutdown().success());
}

#[test]
fn opencode_stop_forces_process_exit_after_global_dispose_variants() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let locks = tempfile::tempdir().expect("创建锁目录失败");
    let mut sc = Sidecar::start(&[("HALO_SHUTDOWN_GRACE_MS", "200")]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());

    for mode in ["happy", "dispose_failure", "hang_on_dispose"] {
        let lock_file = locks.path().join(format!("{mode}.lock"));
        let lock_arg = lock_file.to_string_lossy().to_string();
        let dispose_marker = locks.path().join(format!("{mode}.dispose"));
        let dispose_marker_arg = dispose_marker.to_string_lossy().to_string();
        let config_id = sc.save_config(
            "opencode",
            &fake_opencode_exe(),
            &[
                "--mode",
                mode,
                "--lock-file",
                &lock_arg,
                "--dispose-marker-file",
                &dispose_marker_arg,
            ],
            Some(credential.reference()),
        );
        sc.start_runtime("opencode", &config_id);
        assert!(
            wait_process_lock_held(&lock_file, Duration::from_secs(5)),
            "{mode} OpenCode 进程应持有锁文件"
        );

        let stopped = sc.ok("runtime.stop", json!({"agent": "opencode"}));
        assert_eq!(stopped["state"], "stopped");
        assert_eq!(
            std::fs::read_to_string(&dispose_marker).as_deref(),
            Ok("global_dispose"),
            "{mode} 停止必须调用 OpenCode 的 /global/dispose"
        );
        assert!(
            wait_process_lock_released(&lock_file, Duration::from_secs(5)),
            "{mode} dispose 后 Sidecar 必须强制回收 OpenCode 进程"
        );
    }

    assert!(sc.shutdown().success());
}
