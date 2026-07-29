//! 差异化功能场景：活跃任务期间的 fs.* 写入必须诚实标记人工介入，
//! 但不能阻断写入；交付证据同时携带结束树哈希供 IDE freshness 门禁使用。

mod support;

use serde_json::json;
use sha2::{Digest, Sha256};
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn fs_writes_are_eventful_deduped_and_leave_a_hash_verified_review_bundle() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config(
        "pi",
        &fake_pi_exe(),
        &["--mode", "happy", "--step-delay-ms", "250"],
        None,
    );
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "运行期工作台保存");

    sc.wait_event("task.phase planning", |event| {
        event["event"] == "task.phase" && event["task_id"] == task_id.as_str()
    });

    let before = sc.ok("fs.read", json!({"path": "base.txt"}));
    let first = sc.ok(
        "fs.write",
        json!({
            "path": "base.txt",
            "content": "第一次经工作台保存\n",
            "expected_hash": before["hash"],
            "encoding": "utf-8"
        }),
    );
    assert_eq!(first["path"], "base.txt");
    let second = sc.ok(
        "fs.write",
        json!({
            "path": "base.txt",
            "content": "第二次经工作台保存\n",
            "expected_hash": first["hash"],
            "encoding": "utf-8"
        }),
    );
    assert_eq!(second["path"], "base.txt");

    let finished = sc.wait_task_finished(&task_id);
    assert_eq!(finished["outcome"], "finished");

    let events = sc.events_snapshot();
    let manual_events: Vec<_> = events
        .iter()
        .filter(|event| {
            event["event"] == "task.manual_edit"
                && event["task_id"] == task_id.as_str()
                && event["payload"]["source"] == "fs_write"
        })
        .collect();
    assert_eq!(manual_events.len(), 2, "每次保存都要发出过程事件");
    assert!(manual_events
        .iter()
        .all(|event| event["payload"]["path"] == "base.txt"));

    let bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(bundle["attribution"], "mixed");
    assert_eq!(bundle["manual_edit_paths"], json!(["base.txt"]));
    let reasons = bundle["attribution_reasons"].as_array().expect("归因原因应为数组");
    assert_eq!(reasons.len(), 1, "同一路径重复保存只记一次归因原因");
    assert!(reasons[0]
        .as_str()
        .unwrap_or_default()
        .contains("经工作台保存 base.txt"));

    let files = bundle["files"].as_array().expect("files 应为数组");
    let base_file = files
        .iter()
        .find(|file| file["path"] == "base.txt")
        .unwrap_or_else(|| panic!("人工保存后的文件应进入交付证据：{files:?}"));
    let bytes = std::fs::read(repo.root.join("base.txt")).expect("读取结束树文件失败");
    let expected_hash = format!("sha256:{:x}", Sha256::digest(bytes));
    assert_eq!(base_file["end_hash"], expected_hash);

    // evidence 在 review_ready 已定稿；后续保存仍成功，但不能再改变任务归因或发送事件。
    let events_before = sc.events_snapshot().len();
    let current = sc.ok("fs.read", json!({"path": "base.txt"}));
    let late = sc.ok(
        "fs.write",
        json!({
            "path": "base.txt",
            "content": "审查后的本地保存\n",
            "expected_hash": current["hash"],
            "encoding": "utf-8"
        }),
    );
    assert_eq!(late["path"], "base.txt");
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(sc.events_snapshot().len(), events_before);
    let stable_bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(stable_bundle["manual_edit_paths"], json!(["base.txt"]));
}
