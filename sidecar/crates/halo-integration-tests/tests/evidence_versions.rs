//! 场景 7：追加式交付证据。
//! 交接路径：handoff.create 后带 handoff_id 再 task.create —— 本实现语义下产生
//! 一个新任务的新证据，旧任务证据保持可读且不可覆盖。
//! 同任务多版本语义：经真实 halo-store 在同一数据目录追加第二版证据后重启，
//! 断言旧版本仍可读（is_latest=false）且不可被接受（EVIDENCE_NOT_LATEST）。

mod support;

use serde_json::json;
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn handoff_retry_appends_evidence_and_old_version_not_acceptable() {
    let repo = TestRepo::new();
    let data_tmp = tempfile::tempdir().expect("创建数据目录失败");
    let data_dir = data_tmp.path().join("证据 数据");

    let task_a;
    let task_b;
    {
        let mut sc = Sidecar::start_with_data_dir(&data_dir, &[]);
        sc.hello();
        sc.open_and_trust(&repo.path_str());
        let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);
        sc.start_runtime("pi", &cfg);

        // 任务 A：第一版证据
        task_a = sc.create_task("pi", &cfg, "第一次交付");
        assert_eq!(sc.wait_task_finished(&task_a)["outcome"], "finished");

        // 用户审阅后创建交接包，另一 Agent 接续（实现语义：新任务）
        let handoff = sc.ok(
            "handoff.create",
            json!({
                "task_id": task_a,
                "target_agent": "opencode",
                "selected_files": ["hello_from_agent.txt"]
            }),
        );
        let handoff_id = handoff["handoff_id"].as_str().expect("缺少 handoff_id").to_string();

        // 一个活动工作区同一时刻只允许一个非终态任务：先对 A 作出结论才能接续
        sc.err(
            "task.create",
            json!({
                "agent": "pi", "config_id": cfg, "title": "过早接续",
                "instructions": "A 尚未有结论", "handoff_id": handoff_id
            }),
            "TASK_ALREADY_RUNNING",
        );
        let decision = sc.ok(
            "delivery.accept",
            json!({"task_id": task_a, "evidence_version": 1}),
        );
        assert_eq!(decision["decision"]["kind"], "accepted");

        // 不存在的 handoff_id 被拒绝
        sc.err(
            "task.create",
            json!({
                "agent": "pi",
                "config_id": cfg,
                "title": "无效交接",
                "instructions": "x",
                "handoff_id": "ho-00000000-0000-0000-0000-000000000000"
            }),
            "HANDOFF_NOT_FOUND",
        );

        // 让接续任务产生真实变更：开发者先移除上一轮产物
        std::fs::remove_file(repo.root.join("hello_from_agent.txt")).expect("删除文件失败");

        let created = sc.ok(
            "task.create",
            json!({
                "agent": "pi",
                "config_id": cfg,
                "title": "交接接续任务",
                "instructions": "按交接包继续任务",
                "handoff_id": handoff_id
            }),
        );
        task_b = created["task"]["task_id"].as_str().expect("缺少 task_id").to_string();
        assert_ne!(task_b, task_a, "交接接续在本实现语义下是新的任务");
        assert_eq!(sc.wait_task_finished(&task_b)["outcome"], "finished");

        // 旧任务证据仍可读且未被覆盖
        let bundle_a = sc.ok("review.get", json!({"task_id": task_a}));
        assert_eq!(bundle_a["evidence_version"], 1);
        assert_eq!(bundle_a["is_latest"], true);
        let bundle_b = sc.ok("review.get", json!({"task_id": task_b}));
        assert_eq!(bundle_b["evidence_version"], 1);

        let status = sc.shutdown();
        assert!(status.success());
    }

    // 经真实 halo-store 在同一任务上追加第二版证据（重试语义的持久化事实）
    {
        let store = halo_store::Store::open(
            &data_dir.join("halo.db"),
            halo_store::StoreLimits::default(),
        )
        .expect("打开本地存储失败");
        let version = store
            .append_evidence(
                &task_a,
                &halo_store::EvidenceDraft {
                    outcome: "finished".to_string(),
                    attribution: "agent_only".to_string(),
                    attribution_reasons: vec![],
                    summary: "重试产生的第二版证据".to_string(),
                    files: vec![],
                    verification_status: "passed".to_string(),
                    verification_detail: "重试自检通过".to_string(),
                    verification_source: "agent".to_string(),
                    baseline_dirty_files: vec![],
                    created_at: "2026-07-27T00:00:00Z".to_string(),
                },
            )
            .expect("追加证据失败");
        assert_eq!(version, 2, "追加式证据只能得到下一个版本号");
    }

    let mut sc2 = Sidecar::start_with_data_dir(&data_dir, &[]);
    sc2.hello();

    // 最新版本为 2；旧版本 1 仍可读，且 is_latest=false
    let latest = sc2.ok("review.get", json!({"task_id": task_a}));
    assert_eq!(latest["evidence_version"], 2);
    assert_eq!(latest["is_latest"], true);
    let old = sc2.ok("review.get", json!({"task_id": task_a, "version": 1}));
    assert_eq!(old["evidence_version"], 1);
    assert_eq!(old["is_latest"], false);
    assert!(
        old["files"]
            .as_array()
            .expect("files 应为数组")
            .iter()
            .any(|f| f["path"] == "hello_from_agent.txt"),
        "旧版本内容必须保持原样可读"
    );

    // 旧版本不可被接受：EVIDENCE_NOT_LATEST
    let error = sc2.err(
        "delivery.accept",
        json!({"task_id": task_a, "evidence_version": 1}),
        "EVIDENCE_NOT_LATEST",
    );
    assert_eq!(error["details"]["latest_version"], 2);

    // 历史证据列出全部版本；摘要形式不含逐文件 diff 正文
    let evidence = sc2.ok("history.evidence", json!({"task_id": task_a}));
    let versions = evidence["versions"].as_array().expect("versions 应为数组");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["evidence_version"], 1);
    assert_eq!(versions[0]["is_latest"], false);
    assert_eq!(versions[1]["evidence_version"], 2);
    assert_eq!(versions[1]["is_latest"], true);
    for v in versions {
        for f in v["files"].as_array().expect("files 应为数组") {
            assert!(
                f.get("diff").is_none(),
                "history.evidence 不得携带逐文件 diff 正文：{f}"
            );
        }
    }
}
