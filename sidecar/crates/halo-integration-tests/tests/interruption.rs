//! 场景 5：中断恢复。
//! happy 任务运行中直接强杀 sidecar 进程 → 用同一 HALO_DATA_DIR 重启 →
//! 任务被如实标记 interrupted，不自动恢复或重放。

mod support;

use serde_json::json;
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn killed_sidecar_marks_task_interrupted_on_restart_without_replay() {
    let repo = TestRepo::new();
    // 数据目录由测试自持，跨两次 sidecar 生命周期
    let data_tmp = tempfile::tempdir().expect("创建数据目录失败");
    let data_dir = data_tmp.path().join("中断 数据");

    let task_id;
    {
        let mut sc = Sidecar::start_with_data_dir(&data_dir, &[]);
        sc.hello();
        sc.open_and_trust(&repo.path_str());
        // 放慢脚本步进，确保强杀发生在任务运行中
        let cfg = sc.save_config(
            "pi",
            &fake_pi_exe(),
            &["--mode", "happy", "--step-delay-ms", "300"],
            None,
        );
        sc.start_runtime("pi", &cfg);
        task_id = sc.create_task("pi", &cfg, "中断恢复用任务");
        sc.wait_event("task.phase editing", |e| {
            e["event"] == "task.phase"
                && e["task_id"] == task_id.as_str()
                && e["payload"]["phase"] == "editing"
        });
        // 模拟应用崩溃：不走关闭流程直接杀进程
        sc.kill();
    }

    let mut sc2 = Sidecar::start_with_data_dir(&data_dir, &[]);
    sc2.hello();

    // 任务如实标记 interrupted
    let status = sc2.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "interrupted");
    assert_eq!(status["task"]["cancel_mode"], serde_json::Value::Null);

    // 不自动重放：没有当前任务、运行时不自动拉起、工作区不自动恢复
    let current = sc2.ok("task.status", json!({}));
    assert_eq!(current["task"], serde_json::Value::Null, "重启后不得自动恢复当前任务");
    let runtime = sc2.ok("runtime.status", json!({}));
    assert_eq!(runtime["pi"]["state"], "not_probed", "重启后不得自动拉起运行时");
    assert_eq!(runtime["opencode"]["state"], "not_probed");
    let ws = sc2.ok("workspace.status", json!({}));
    assert_eq!(ws["active"], false, "重启后不得自动恢复活动工作区");

    // 重启后除 sidecar.state 外没有任何任务重放事件
    let replayed: Vec<String> = sc2
        .events_snapshot()
        .iter()
        .filter(|e| e["event"] != "sidecar.state")
        .map(|e| e["event"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(replayed.is_empty(), "重启后不得重放任务事件：{replayed:?}");

    // 历史中该任务同样为 interrupted
    let history = sc2.ok("history.list", json!({"limit": 20}));
    let entry = history["tasks"]
        .as_array()
        .expect("tasks 应为数组")
        .iter()
        .find(|t| t["task_id"] == task_id.as_str())
        .expect("历史应包含中断任务")
        .clone();
    assert_eq!(entry["state"], "interrupted");
}
