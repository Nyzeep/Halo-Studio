//! 场景 11：事件恢复。
//! task.snapshot 的 after_seq 语义（增量、尾部、越界容忍）与环形缓冲不足时的
//! EVENT_GAP（details 携带 oldest_available_seq，UI 应整体重建视图）。

mod support;

use serde_json::json;
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn snapshot_after_seq_semantics_and_event_gap() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "事件恢复任务");
    sc.wait_task_finished(&task_id);

    // after_seq=0：从 seq 1 起完整返回，seq 连续且与 last_seq 一致
    let snap = sc.ok("task.snapshot", json!({"after_seq": 0}));
    let events = snap["events"].as_array().expect("events 应为数组");
    assert!(!events.is_empty());
    let last_seq = snap["last_seq"].as_u64().expect("last_seq 应为数字");
    let mut expect = 1u64;
    for e in events {
        assert_eq!(e["seq"].as_u64(), Some(expect), "seq 必须连续：{e}");
        expect += 1;
    }
    assert_eq!(expect - 1, last_seq, "events 应覆盖到 last_seq");
    assert_eq!(events[0]["event"], "sidecar.state");
    assert!(
        events.iter().any(|e| e["event"] == "task.finished"),
        "快照应包含任务终局事件"
    );
    // 快照携带当前任务状态
    assert_eq!(snap["task"]["task_id"], task_id.as_str());
    assert_eq!(snap["task"]["state"], "review_ready");

    // 增量语义：after_seq = last-3 → 恰好尾部 3 条
    let tail = sc.ok("task.snapshot", json!({"after_seq": last_seq - 3}));
    let tail_events = tail["events"].as_array().expect("events 应为数组");
    assert_eq!(tail_events.len(), 3);
    assert_eq!(tail_events[0]["seq"].as_u64(), Some(last_seq - 2));

    // 尾部语义：after_seq = last → 空增量；越界 after_seq 容忍为空
    let empty = sc.ok("task.snapshot", json!({"after_seq": last_seq}));
    assert_eq!(empty["events"].as_array().map(Vec::len), Some(0));
    assert_eq!(empty["last_seq"].as_u64(), Some(last_seq));
    let beyond = sc.ok("task.snapshot", json!({"after_seq": last_seq + 100}));
    assert_eq!(beyond["events"].as_array().map(Vec::len), Some(0));

    // 制造环形缓冲淘汰：每次 task.mark_verification 都推送一条 task.verification 事件
    for i in 0..1100u32 {
        let r = sc.request(
            "task.mark_verification",
            json!({"task_id": task_id, "status": "not_run", "note": format!("填充事件 {i}")}),
        );
        assert_eq!(r["ok"], true, "mark_verification 第 {i} 次失败：{r}");
    }

    // 读取当前 last_seq：用超出尾部的 after_seq（容忍语义，返回空增量）
    let snap = sc.ok("task.snapshot", json!({"after_seq": 100_000_000u64}));
    let new_last = snap["last_seq"].as_u64().expect("last_seq 应为数字");
    assert!(new_last > 1024, "应已产生超过缓冲容量的事件");

    // 缓冲不足覆盖 after_seq=1 → EVENT_GAP，details 告知最早可用 seq
    let error = sc.err("task.snapshot", json!({"after_seq": 1}), "EVENT_GAP");
    let oldest = error["details"]["oldest_available_seq"]
        .as_u64()
        .expect("EVENT_GAP 必须携带 oldest_available_seq");
    assert_eq!(oldest, new_last - 1024 + 1, "环形缓冲应保留最近 1024 条");
    assert!(
        error["message"].as_str().unwrap_or_default().contains("重建"),
        "错误文案应指引整体重建视图：{error}"
    );

    // 恰好在缓冲边界内：after_seq = oldest-1 可正常增量
    let ok_edge = sc.ok("task.snapshot", json!({"after_seq": oldest - 1}));
    assert_eq!(
        ok_edge["events"].as_array().map(Vec::len),
        Some(1024),
        "边界处应返回完整缓冲"
    );
    let gap_edge = sc.err("task.snapshot", json!({"after_seq": oldest - 2}), "EVENT_GAP");
    assert_eq!(gap_edge["details"]["oldest_available_seq"].as_u64(), Some(oldest));
}
