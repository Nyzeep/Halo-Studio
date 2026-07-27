//! 场景 10：工作区边界。
//! 未信任时 runtime.start / task.create 被拒；workspace.open 切换时旧运行时进程
//! 确实退出（记 PID 断言不存活）；信任决定按（real_path, root_commit）持久化。
//! 含空格与中文字符路径的全链路可用性由 happy_pi.rs 覆盖（所有测试仓库路径均含
//! 空格与中文）。

mod support;

use std::time::Duration;

use serde_json::json;
use support::{fake_pi_exe, wait_process_lock_held, wait_process_lock_released, Sidecar, TestRepo};

#[test]
fn untrusted_workspace_rejects_runtime_and_task() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();

    let ws = sc.ok("workspace.open", json!({"path": repo.path_str()}));
    assert_eq!(ws["trust"], "untrusted");
    let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);

    // 未信任：config.* 读操作允许，但启动运行时与创建任务一律拒绝
    let configs = sc.ok("config.list", json!({}));
    assert!(configs["configs"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false));
    sc.err(
        "runtime.start",
        json!({"agent": "pi", "config_id": cfg}),
        "WORKSPACE_NOT_TRUSTED",
    );
    sc.err(
        "task.create",
        json!({
            "agent": "pi", "config_id": cfg,
            "title": "未信任", "instructions": "不应创建"
        }),
        "WORKSPACE_NOT_TRUSTED",
    );

    // 没有活动工作区时同样拒绝
    sc.ok("workspace.close", json!({}));
    sc.err(
        "runtime.start",
        json!({"agent": "pi", "config_id": cfg}),
        "WORKSPACE_NOT_ACTIVE",
    );

    // 非 Git / 不存在路径的错误映射（中文文案）
    let bad = sc.err(
        "workspace.open",
        json!({"path": "Z:\\不存在\\目 录"}),
        "WORKSPACE_PATH_INVALID",
    );
    assert!(bad["message"]
        .as_str()
        .unwrap_or_default()
        .contains("工作区"));
    let plain_dir = tempfile::tempdir().expect("创建目录失败");
    let plain = plain_dir.path().join("非 git 目录");
    std::fs::create_dir_all(&plain).expect("创建目录失败");
    sc.err(
        "workspace.open",
        json!({"path": plain.to_string_lossy()}),
        "WORKSPACE_NOT_GIT",
    );
}

#[test]
fn switching_workspace_stops_old_runtime_process() {
    let repo1 = TestRepo::new();
    let repo2 = TestRepo::new();
    let lock_dir = tempfile::tempdir().expect("创建锁目录失败");
    let lock_file = lock_dir.path().join("fake-pi.lock");
    let lock_arg = lock_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo1.path_str());
    let cfg = sc.save_config("pi", &fake_pi_exe(), &["--lock-file", &lock_arg], None);
    sc.start_runtime("pi", &cfg);

    assert!(
        wait_process_lock_held(&lock_file, Duration::from_secs(5)),
        "旧工作区运行时应持有锁文件"
    );

    // 无运行中任务：切换自动停止旧运行时
    let ws2 = sc.ok("workspace.open", json!({"path": repo2.path_str()}));
    assert_eq!(ws2["trust"], "untrusted", "新路径必须重新确认信任");
    assert!(
        wait_process_lock_released(&lock_file, Duration::from_secs(5)),
        "切换工作区后旧运行时进程必须释放锁文件"
    );
    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["pi"]["state"], "stopped");

    // 信任决定按（real_path, root_commit）持久化：切回旧仓库直接恢复 trusted
    let ws1_again = sc.ok("workspace.open", json!({"path": repo1.path_str()}));
    assert_eq!(ws1_again["trust"], "trusted");
    assert_eq!(ws1_again["identity_changed"], false);
}

#[test]
fn revoke_trust_stops_runtime_immediately() {
    let repo = TestRepo::new();
    let lock_dir = tempfile::tempdir().expect("创建锁目录失败");
    let lock_file = lock_dir.path().join("fake-pi.lock");
    let lock_arg = lock_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[]);
    sc.hello();
    let ws_id = sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config("pi", &fake_pi_exe(), &["--lock-file", &lock_arg], None);
    sc.start_runtime("pi", &cfg);
    assert!(
        wait_process_lock_held(&lock_file, Duration::from_secs(5)),
        "受管运行时应持有锁文件"
    );

    let ws = sc.ok(
        "workspace.trust",
        json!({"workspace_id": ws_id, "decision": "revoke"}),
    );
    assert_eq!(ws["trust"], "untrusted");
    assert!(
        wait_process_lock_released(&lock_file, Duration::from_secs(5)),
        "撤销信任必须立即停止并释放运行时锁文件"
    );
    // 撤销后再次启动被拒
    sc.err(
        "runtime.start",
        json!({"agent": "pi", "config_id": cfg}),
        "WORKSPACE_NOT_TRUSTED",
    );
}

#[test]
fn dropping_sidecar_stops_managed_runtime() {
    let repo = TestRepo::new();
    let lock_dir = tempfile::tempdir().expect("创建锁目录失败");
    let lock_file = lock_dir.path().join("fake-pi.lock");
    let lock_arg = lock_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config("pi", &fake_pi_exe(), &["--lock-file", &lock_arg], None);
    sc.start_runtime("pi", &cfg);
    assert!(
        wait_process_lock_held(&lock_file, Duration::from_secs(5)),
        "受管运行时应持有锁文件"
    );

    drop(sc);

    assert!(
        wait_process_lock_released(&lock_file, Duration::from_secs(5)),
        "测试驱动关闭 Sidecar 后，受管 Pi 进程必须释放锁文件"
    );
}
