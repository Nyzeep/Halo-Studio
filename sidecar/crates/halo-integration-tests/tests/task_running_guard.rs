//! TASK_RUNNING 守卫（契约 3.1）：存在非终态任务时 workspace.open / workspace.close
//! 一律返回 TASK_RUNNING 且活动工作区保持不变；任务终局化（accept）后关闭成功。
//! 用放慢步进的 fake-pi（--step-delay-ms 500，约 3.5s 运行窗口）保证两次工作区
//! 操作确实发生在任务运行期间。

mod support;

use serde_json::json;
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn running_task_blocks_workspace_switch_and_close() {
    let repo = TestRepo::new();
    let other = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    let ws_id = sc.open_and_trust(&repo.path_str());

    // 放慢脚本步进：happy 脚本 7 步 × 500ms ≈ 3.5s 的稳定运行窗口
    let cfg = sc.save_config("pi", &fake_pi_exe(), &["--step-delay-ms", "500"], None);
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "运行中阻断工作区操作");

    // 运行中：切换与关闭一律 TASK_RUNNING（中文文案）
    let err = sc.err(
        "workspace.open",
        json!({"path": other.path_str()}),
        "TASK_RUNNING",
    );
    assert!(
        err["message"].as_str().unwrap_or_default().contains("任务"),
        "{err}"
    );
    sc.err("workspace.close", json!({}), "TASK_RUNNING");

    // 被拒后活动工作区保持不变（workspace_id 未被替换）
    let status = sc.ok("workspace.status", json!({}));
    assert_eq!(status["active"], true);
    assert_eq!(status["workspace_id"], ws_id.as_str());

    // 任务结束进入 review_ready：仍是非终态，关闭依旧被阻断
    let finished = sc.wait_task_finished(&task_id);
    assert_eq!(finished["outcome"], "finished");
    sc.err("workspace.close", json!({}), "TASK_RUNNING");

    // accept 终局化后：workspace.close 成功
    let decision = sc.ok(
        "delivery.accept",
        json!({"task_id": task_id, "evidence_version": finished["evidence_version"]}),
    );
    assert_eq!(decision["decision"]["kind"], "accepted");
    let closed = sc.ok("workspace.close", json!({}));
    assert_eq!(closed["closed"], true);
}
