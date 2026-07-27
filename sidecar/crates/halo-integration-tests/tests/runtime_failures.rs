//! 场景 4：就绪失败与版本失败。
//! not_ready（就绪超时）/ garbage（坏帧）/ wrong_version（版本握手不匹配）/
//! unhealthy（健康检查不过）/ bad_token（401 失败关闭）。
//! 断言：错误码、中文 reason、recovery_hint，以及 runtime.state failed 事件。

mod support;

use serde_json::{json, Value};
use support::{fake_opencode_exe, fake_pi_exe, Sidecar, TestRepo};

/// 等待 runtime.state failed 事件并断言中文 reason 与 recovery_hint 存在。
fn assert_failed_event(sc: &Sidecar, agent: &str, reason_contains: &str) -> Value {
    let ev = sc.wait_event("runtime.state failed", |e| {
        e["event"] == "runtime.state"
            && e["payload"]["agent"] == agent
            && e["payload"]["state"] == "failed"
    });
    let reason = ev["payload"]["reason"].as_str().unwrap_or_default().to_string();
    let hint = ev["payload"]["recovery_hint"].as_str().unwrap_or_default();
    assert!(
        reason.contains(reason_contains),
        "failed reason 应包含“{reason_contains}”：{reason}"
    );
    assert!(
        reason.chars().any(|c| !c.is_ascii()),
        "reason 必须是中文用户可读文案：{reason}"
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
    assert!(msg.contains("就绪"), "错误文案应说明就绪失败：{msg}");
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
    assert!(msg.contains("协议帧") || msg.contains("JSON"), "{msg}");
    assert_failed_event(&sc, "pi", "协议帧");
}

#[test]
fn opencode_wrong_version_is_version_mismatch() {
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "wrong_version"],
        None,
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_VERSION_MISMATCH",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("9.9.9") && msg.contains("0.4.2"), "应给出两侧版本：{msg}");
    assert_failed_event(&sc, "opencode", "版本不匹配");
}

#[test]
fn opencode_unhealthy_times_out_health_check() {
    let (_repo, mut sc) = setup(&[("HALO_READY_TIMEOUT_MS", "500")]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "unhealthy"],
        None,
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("健康检查"), "{msg}");
    assert_failed_event(&sc, "opencode", "健康检查");
}

#[test]
fn opencode_bad_token_fails_closed() {
    let (_repo, mut sc) = setup(&[]);
    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--mode", "bad_token"],
        None,
    );
    let error = sc.err(
        "runtime.start",
        json!({"agent": "opencode", "config_id": cfg}),
        "RUNTIME_NOT_READY",
    );
    let msg = error["message"].as_str().unwrap_or_default();
    assert!(msg.contains("认证"), "{msg}");
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
