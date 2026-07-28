//! 场景 5：中断恢复。
//! happy 任务运行中直接强杀 sidecar 进程 → 用同一 HALO_DATA_DIR 重启 →
//! 任务被如实标记 interrupted，不自动恢复或重放。

mod support;

use serde_json::json;
use support::{fake_opencode_exe, fake_pi_exe, require_test_credential, Sidecar, TestRepo};

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

#[test]
fn stdin_eof_marks_a_running_task_interrupted_without_recovery() {
    let repo = TestRepo::new();
    let data_tmp = tempfile::tempdir().expect("创建数据目录失败");
    let data_dir = data_tmp.path().join("EOF 中断 数据");

    let task_id;
    {
        let mut sc = Sidecar::start_with_data_dir(&data_dir, &[]);
        sc.hello();
        sc.open_and_trust(&repo.path_str());
        let cfg = sc.save_config(
            "pi",
            &fake_pi_exe(),
            &["--mode", "happy", "--step-delay-ms", "300"],
            None,
        );
        sc.start_runtime("pi", &cfg);
        task_id = sc.create_task("pi", &cfg, "EOF 中断任务");
        sc.wait_event("EOF 前任务正在运行", |event| {
            event["event"] == "task.phase"
                && event["task_id"] == task_id.as_str()
                && event["payload"]["phase"] == "editing"
        });

        assert!(sc.shutdown().success(), "stdin EOF 应正常关闭 Sidecar");
    }

    let mut sc2 = Sidecar::start_with_data_dir(&data_dir, &[]);
    sc2.hello();
    let status = sc2.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "interrupted");
    assert_eq!(sc2.ok("task.status", json!({}))["task"], serde_json::Value::Null);
    assert_eq!(
        sc2.ok("runtime.status", json!({}))["pi"]["state"],
        "not_probed",
        "EOF 后不得自动拉起运行时"
    );
    let snapshot = sc2.ok("task.snapshot", json!({"after_seq": 0}));
    assert_eq!(snapshot["task"], serde_json::Value::Null);
    assert_eq!(snapshot["session_messages"], json!([]));
    assert!(snapshot["events"].as_array().unwrap().iter().all(|event| {
        !matches!(
            event["event"].as_str(),
            Some("task.session_message" | "task.action_request" | "task.action_resolved")
        )
    }));
}

#[test]
fn opencode_interruption_keeps_workspace_and_does_not_replay_session_or_agent_write() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let data_tmp = tempfile::tempdir().expect("创建数据目录失败");
    let data_dir = data_tmp.path().join("OpenCode 中断 数据");
    let marker_tmp = tempfile::tempdir().expect("创建写入标记目录失败");
    let marker_file = marker_tmp.path().join("agent-write.log");
    let marker_arg = marker_file.to_string_lossy().to_string();

    let task_id;
    let original_file;
    {
        let mut sc = Sidecar::start_with_data_dir(&data_dir, &[]);
        sc.hello();
        sc.open_and_trust(&repo.path_str());
        let cfg = sc.save_config(
            "opencode",
            &fake_opencode_exe(),
            &[
                "--mode",
                "happy",
                "--write-marker-file",
                &marker_arg,
                "--require-credential-env",
                "OPENAI_API_KEY",
                "--require-isolated-state",
            ],
            Some(credential.reference()),
        );
        sc.start_runtime("opencode", &cfg);
        task_id = sc.create_task("opencode", &cfg, "OpenCode 中断且不重放");
        sc.wait_event("首轮 Agent 回复已进入活动会话", |event| {
            event["event"] == "task.session_message"
                && event["task_id"] == task_id.as_str()
                && event["payload"]["role"] == "agent"
        });
        let waiting = sc.wait_event("OpenCode 中断前等待开发者", |event| {
            event["event"] == "task.state"
                && event["task_id"] == task_id.as_str()
                && event["payload"]["state"] == "waiting_developer"
        });
        assert_eq!(waiting["payload"]["task"]["state"], "waiting_developer");
        original_file = std::fs::read_to_string(repo.root.join("hello_from_agent.txt"))
            .expect("OpenCode 首轮应产生工作区文件");
        assert_eq!(
            std::fs::read_to_string(&marker_file)
                .expect("应记录 Agent 写入")
                .lines()
                .count(),
            1
        );

        // 模拟 Sidecar 意外退出；不发 abort、不发新的消息，也不走正常清理。
        sc.kill();
    }

    let mut sc2 = Sidecar::start_with_data_dir(&data_dir, &[]);
    sc2.hello();
    let status = sc2.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "interrupted");
    assert_eq!(
        sc2.ok("runtime.status", json!({}))["opencode"]["state"],
        "not_probed",
        "重启后不得自动重连 OpenCode"
    );
    assert_eq!(sc2.ok("task.status", json!({}))["task"], serde_json::Value::Null);
    let snapshot = sc2.ok("task.snapshot", json!({"after_seq": 0}));
    assert_eq!(snapshot["task"], serde_json::Value::Null);
    assert_eq!(snapshot["session_messages"], json!([]));
    assert_eq!(snapshot["action_requests"], json!([]));
    assert!(snapshot["events"].as_array().unwrap().iter().all(|event| {
        !matches!(
            event["event"].as_str(),
            Some("task.session_message" | "task.action_request" | "task.action_resolved")
        )
    }));
    assert_eq!(
        std::fs::read_to_string(repo.root.join("hello_from_agent.txt"))
            .expect("中断后工作区文件应保留"),
        original_file,
        "重启后不得重复 Agent 写入"
    );
    assert_eq!(
        std::fs::read_to_string(&marker_file)
            .expect("写入标记应保留")
            .lines()
            .count(),
        1,
        "重启后不得重复 Agent 写入"
    );
    sc2.err(
        "review.get",
        json!({"task_id": task_id}),
        "EVIDENCE_NOT_FOUND",
    );
    assert!(sc2.shutdown().success());
}

#[test]
fn interruption_preserves_existing_review_evidence_and_workspace_changes() {
    let repo = TestRepo::new();
    let data_tmp = tempfile::tempdir().expect("创建数据目录失败");
    let data_dir = data_tmp.path().join("证据中断 数据");

    let task_id;
    let evidence;
    let file_before;
    {
        let mut sc = Sidecar::start_with_data_dir(&data_dir, &[]);
        sc.hello();
        sc.open_and_trust(&repo.path_str());
        let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);
        sc.start_runtime("pi", &cfg);
        task_id = sc.create_task("pi", &cfg, "中断前已有证据");
        let finished = sc.wait_task_finished(&task_id);
        let version = finished["evidence_version"]
            .as_u64()
            .expect("任务结束应有证据版本");
        evidence = sc.ok(
            "review.get",
            json!({"task_id": task_id, "version": version}),
        );
        file_before = std::fs::read_to_string(repo.root.join("hello_from_agent.txt"))
            .expect("Agent 产物应存在");
        // 证据已落库后直接模拟 Sidecar 崩溃，验证重启收口不会丢失已有事实。
        sc.kill();
    }

    let mut sc2 = Sidecar::start_with_data_dir(&data_dir, &[]);
    sc2.hello();
    let status = sc2.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "interrupted");
    let preserved = sc2.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(preserved["evidence_version"], evidence["evidence_version"]);
    assert_eq!(preserved["files"], evidence["files"]);
    assert_eq!(
        std::fs::read_to_string(repo.root.join("hello_from_agent.txt"))
            .expect("中断后 Agent 产物应保留"),
        file_before
    );
    assert!(sc2.shutdown().success());
}
