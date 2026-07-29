//! 场景 4：就绪失败与版本失败。
//! not_ready（就绪超时）/ garbage（坏帧）/ wrong_version（兼容性档案不匹配）/
//! unhealthy（健康检查不过）/ bad_auth（401 失败关闭）。
//! 断言：错误码、中文 reason、recovery_hint，以及 runtime.state failed 事件。

mod support;

use serde_json::{json, Value};
use support::{fake_opencode_exe, fake_pi_exe, require_test_credential, Sidecar, TestRepo};

/// 等待 runtime.state failed 事件并断言中文 reason 与 recovery_hint 存在。
fn assert_failed_event(sc: &Sidecar, agent: &str, reason_contains: &str) -> Value {
    let ev = sc.wait_event("runtime.state failed", |e| {
        e["event"] == "runtime.state"
            && e["payload"]["agent"] == agent
            && e["payload"]["state"] == "failed"
    });
    let reason = ev["payload"]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let hint = ev["payload"]["recovery_hint"].as_str().unwrap_or_default();
    assert!(
        reason.contains(reason_contains),
        "failed reason 应包含“{reason_contains}”"
    );
    assert!(
        reason.chars().any(|c| !c.is_ascii()),
        "reason 必须是中文用户可读文案"
    );
    assert!(!hint.is_empty(), "failed 必须携带 recovery_hint");
    ev
}

fn setup(extra_env: &[(&str, &str)]) -> (TestRepo, Sidecar) {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(extra_env);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    (repo, sc)
}

#[test]
fn pi_not_ready_times_out_with_reason_and_hint() {
    let (_repo, mut sc) = setup(&[("HALO_READY_TIMEOUT_MS", "500")]);
    let cfg = sc.save_config("pi", &fake_pi_exe(), &["--mode", "not_ready"], None);
    let error = sc.err(
        "runtime.start",
        json!({"agent": "pi", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("就绪"), "错误文案应说明就绪失败");
    assert_failed_event(&sc, "pi", "超时");

    // runtime.status 如实呈现独立失败状态
    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["pi"]["state"], "failed");
    assert!(status["pi"]["reason"].is_string());
    assert!(status["pi"]["recovery_hint"].is_string());
    assert_eq!(status["opencode"]["state"], "not_probed");
}

#[test]
fn pi_garbage_frames_fail_startup() {
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config("pi", &fake_pi_exe(), &["--mode", "garbage"], None);
    let error = sc.err(
        "runtime.start",
        json!({"agent": "pi", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("协议帧") || msg.contains("JSON"));
    assert_failed_event(&sc, "pi", "协议帧");
}

#[test]
fn opencode_unknown_major_version_is_version_mismatch() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "wrong_version"],
        Some(credential.reference()),
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_VERSION_MISMATCH",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("兼容性档案") && msg.contains("1.x"));
    assert_failed_event(&sc, "opencode", "兼容性档案");
}

#[test]
fn opencode_probe_only_accepts_the_known_stable_1x_profile() {
    let (_repo, mut sc) = setup(&[]);
    for (mode, version, supported) in [
        ("happy", "1.18.5", true),
        ("newer_1x", "1.19.0", true),
        ("old_version", "1.18.4", false),
        ("pre_release_version", "1.18.5-pre-release", false),
        ("major_version", "2.0.0", false),
    ] {
        let config_id = sc.save_config("opencode", &fake_opencode_exe(), &["--mode", mode], None);
        let result = sc.ok(
            "runtime.probe",
            json!({"agent": "opencode", "config_id": config_id}),
        );
        assert_eq!(result["version"], version, "mode={mode}");
        assert_eq!(result["supported"], supported, "mode={mode}");
    }

    let config_id = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "malformed_version"],
        None,
    );
    let error = sc.err(
        "runtime.probe",
        json!({"agent": "opencode", "config_id": config_id}),
        "RUNTIME_PROBE_FAILED",
    );
    assert!(error["message"].as_str().unwrap_or_default().contains("版本"));
}

#[test]
fn opencode_supported_stable_1x_update_starts() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[("HALO_SHUTDOWN_GRACE_MS", "200")]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "newer_1x"],
        Some(credential.reference()),
    );
    sc.start_runtime("opencode", &cfg);
    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["opencode"]["state"], "ready");
    assert_eq!(status["opencode"]["version"], "1.19.0");
    assert_eq!(
        sc.ok("runtime.stop", json!({"agent": "opencode"}))["state"],
        "stopped"
    );
}

#[test]
fn opencode_exit_after_ready_transitions_to_failed_instead_of_staying_online() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "exit_early"],
        Some(credential.reference()),
    );
    sc.start_runtime("opencode", &cfg);

    assert_failed_event(&sc, "opencode", "进程已退出");
    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["opencode"]["state"], "failed");
    assert!(status["opencode"]["recovery_hint"]
        .as_str()
        .unwrap_or_default()
        .contains("重新启动"));
    sc.err(
        "task.create",
        json!({
            "agent": "opencode", "config_id": cfg, "title": "已退出的运行时",
            "instructions": "不得把已退出的服务伪造成就绪"
        }),
        "RUNTIME_NOT_READY",
    );
}

#[test]
fn opencode_retry_after_post_ready_exit_reports_the_latest_failure() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[]);
    let exited_config = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "exit_early"],
        Some(credential.reference()),
    );
    sc.start_runtime("opencode", &exited_config);
    assert_failed_event(&sc, "opencode", "进程已退出");

    let auth_failure_config = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "bad_auth"],
        Some(credential.reference()),
    );
    sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": auth_failure_config}),
        "RUNTIME_NOT_READY",
    );
    sc.wait_event("retry runtime.state failed", |event| {
        event["event"] == "runtime.state"
            && event["payload"]["agent"] == "opencode"
            && event["payload"]["state"] == "failed"
            && event["payload"]["reason"]
                .as_str()
                .unwrap_or_default()
                .contains("认证")
    });

    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["opencode"]["state"], "failed");
    assert!(status["opencode"]["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("认证"));
}

#[test]
fn opencode_unhealthy_times_out_health_check() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[("HALO_READY_TIMEOUT_MS", "500")]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "unhealthy"],
        Some(credential.reference()),
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("健康检查"));
    assert_failed_event(&sc, "opencode", "健康检查");
}

#[test]
fn opencode_non_loopback_ready_report_fails_closed() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "wrong_ready_address"],
        Some(credential.reference()),
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    assert!(error["message"]
        .as_str()
        .unwrap_or_default()
        .contains("监听"));
    assert_failed_event(&sc, "opencode", "监听地址");
}

#[test]
fn opencode_missing_ready_confirmation_fails_closed_before_health_probe() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[("HALO_READY_TIMEOUT_MS", "500")]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "missing_ready_line"],
        Some(credential.reference()),
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    assert!(error["message"]
        .as_str()
        .unwrap_or_default()
        .contains("监听确认"));
    assert_failed_event(&sc, "opencode", "监听确认");
}

#[test]
fn opencode_missing_health_version_capability_fails_closed() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "missing_health_version"],
        Some(credential.reference()),
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_VERSION_MISMATCH",
    );
    assert!(error["message"]
        .as_str()
        .unwrap_or_default()
        .contains("版本"));
    assert_failed_event(&sc, "opencode", "缺少兼容性档案");
}

#[test]
fn opencode_bad_basic_auth_fails_closed() {
    let credential = require_test_credential();
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "bad_auth"],
        Some(credential.reference()),
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("认证"));
    let ev = assert_failed_event(&sc, "opencode", "认证");
    // 失败事件同样不得携带 token 形态的敏感串
    assert!(!support::contains_lower_hex_run(&ev.to_string(), 64));

    // 未就绪时创建任务被拒
    sc.err(
        "task.create",
        json!({
            "agent": "opencode", "config_id": cfg, "title": "不应创建",
            "instructions": "runtime 未就绪"
        }),
        "RUNTIME_NOT_READY",
    );
}
