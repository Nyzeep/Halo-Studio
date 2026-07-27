//! 场景 6：人工介入。
//! 任务运行中 task.mark_manual_edit → review.get 的 attribution=mixed 且 reasons 非空；
//! 系统诚实标记人工介入，不把全部改动归因 Agent。

mod support;

use serde_json::json;
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn manual_edit_during_task_yields_mixed_attribution() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config(
        "pi",
        &fake_pi_exe(),
        &["--mode", "happy", "--step-delay-ms", "150"],
        None,
    );
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "运行期人工介入");

    sc.wait_event("task.phase planning", |e| {
        e["event"] == "task.phase" && e["task_id"] == task_id.as_str()
    });

    // 本地开发者在任务运行期间手动编辑代码，并显式标记人工介入
    std::fs::write(repo.root.join("手工修复.txt"), "开发者手动修改\n").expect("写文件失败");
    let marked = sc.ok(
        "task.mark_manual_edit",
        json!({"task_id": task_id, "note": "手动修好了一个边角问题"}),
    );
    assert_eq!(marked["attribution"], "mixed");

    let manual_ev = sc.wait_event("task.manual_edit", |e| {
        e["event"] == "task.manual_edit" && e["task_id"] == task_id.as_str()
    });
    assert!(
        manual_ev["payload"]["note"]
            .as_str()
            .unwrap_or_default()
            .contains("手动修好了"),
        "{manual_ev}"
    );

    let finished = sc.wait_task_finished(&task_id);
    assert_eq!(finished["outcome"], "finished");

    let bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(bundle["attribution"], "mixed", "人工介入后不得声称全部由 Agent 编写");
    let reasons = bundle["attribution_reasons"].as_array().expect("reasons 应为数组");
    assert!(!reasons.is_empty(), "mixed 归因必须携带原因");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str().unwrap_or_default().contains("手动修好了")),
        "原因应包含用户备注：{reasons:?}"
    );

    // 任务状态里的归因同样为 mixed
    let status = sc.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["attribution"], "mixed");
}
