//! 场景 3：任务取消两态。
//! - 原生取消：fake-pi 处于 action_request 模式且步进放慢，中途 task.cancel，
//!   Agent 在宽限内经原生通道结束 → mode=native。
//! - 强制取消：fake-pi hang_on_cancel 忽略取消，HALO_CANCEL_GRACE_MS=500 超时
//!   后 Sidecar 强杀 → mode=forced，且假进程确实被终止。

mod support;

use std::time::Duration;

use serde_json::json;
use support::{fake_pi_exe, wait_process_lock_held, wait_process_lock_released, Sidecar, TestRepo};

#[test]
fn cancel_native_when_agent_stops_in_grace() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    // 放慢脚本步进，保证取消发生在任务中途
    let cfg = sc.save_config(
        "pi",
        &fake_pi_exe(),
        &["--mode", "action_request", "--step-delay-ms", "150"],
        None,
    );
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "中途原生取消");

    // Agent 操作请求：任务暂停等待用户经原生通道决定
    let action = sc.wait_event("task.action_request", |e| {
        e["event"] == "task.action_request" && e["task_id"] == task_id.as_str()
    });
    assert_eq!(action["payload"]["kind"], "permission");
    assert_eq!(action["payload"]["channel"], "native");

    let result = sc.ok_with_timeout(
        "task.cancel",
        json!({"task_id": task_id}),
        Duration::from_secs(5),
    );
    assert_eq!(result["accepted"], true);

    let cancelled = sc.wait_event_with_timeout("task.cancelled", Duration::from_secs(5), |e| {
        e["event"] == "task.cancelled" && e["task_id"] == task_id.as_str()
    });
    assert_eq!(
        cancelled["payload"]["mode"], "native",
        "宽限内原生结束必须记 native"
    );

    let status = sc.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "cancelled");
    assert_eq!(status["task"]["cancel_mode"], "native");

    // 取消也留下可审查交付证据（outcome=cancelled），不是无结论中断
    let bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(bundle["outcome"], "cancelled");
}

#[test]
fn cancel_forced_after_grace_timeout_kills_agent() {
    let repo = TestRepo::new();
    let pid_dir = tempfile::tempdir().expect("创建 PID 目录失败");
    let lock_file = pid_dir.path().join("fake-pi.lock");
    let lock_arg = lock_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[("HALO_CANCEL_GRACE_MS", "500")]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config(
        "pi",
        &fake_pi_exe(),
        &["--mode", "hang_on_cancel", "--lock-file", &lock_arg],
        None,
    );
    sc.start_runtime("pi", &cfg);
    assert!(
        wait_process_lock_held(&lock_file, Duration::from_secs(5)),
        "fake-pi 运行时应独占锁文件"
    );

    let task_id = sc.create_task_with_timeout("pi", &cfg, "挂死后强制取消", Duration::from_secs(5));
    sc.wait_event_with_timeout("task.phase planning", Duration::from_secs(5), |e| {
        e["event"] == "task.phase" && e["task_id"] == task_id.as_str()
    });

    let result = sc.ok_with_timeout(
        "task.cancel",
        json!({"task_id": task_id}),
        Duration::from_secs(5),
    );
    assert_eq!(result["accepted"], true);

    let cancelled = sc.wait_event_with_timeout("task.cancelled", Duration::from_secs(5), |e| {
        e["event"] == "task.cancelled" && e["task_id"] == task_id.as_str()
    });
    assert_eq!(
        cancelled["payload"]["mode"], "forced",
        "宽限超时未原生退出必须记 forced"
    );

    let status = sc.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "cancelled");
    assert_eq!(status["task"]["cancel_mode"], "forced");

    // 强制终止必须真实生效：忽略取消的假进程不得存活
    assert!(
        wait_process_lock_released(&lock_file, Duration::from_secs(5)),
        "强制取消后 fake-pi 进程必须释放锁文件"
    );
}
