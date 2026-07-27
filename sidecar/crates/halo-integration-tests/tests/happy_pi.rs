//! 场景 1：Pi 完整链路 happy path（真实 halo-sidecar + fake-pi 子进程）。
//! hello → workspace.open/trust → config.save → runtime.start → task.create →
//! 事件序列（seq 单调、phase 顺序、trace.item）→ task.finished → review.get →
//! delivery.accept → history.list。
//! 同时覆盖场景 10 的“含空格与中文字符的工作区路径全链路可用”。

mod support;

use serde_json::{json, Value};
use support::{fake_pi_exe, Sidecar, TestRepo};

#[test]
fn pi_full_delivery_chain_happy() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);

    // 启动后首条事件：sidecar.state ready，seq 从 1 开始
    let first = sc.wait_event("sidecar.state", |e| e["event"] == "sidecar.state");
    assert_eq!(first["seq"], 1);
    assert_eq!(first["payload"]["state"], "ready");
    assert_eq!(first["payload"]["protocol_version"], 1);

    let hello = sc.hello();
    let caps: Vec<&str> = hello["capabilities"]
        .as_array()
        .expect("capabilities 应为数组")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for cap in ["workspace", "config", "pi", "opencode", "task", "review", "handoff", "history"] {
        assert!(caps.contains(&cap), "缺少能力 {cap}：{caps:?}");
    }

    // 含空格与中文的工作区路径全链路可用
    let ws = sc.ok("workspace.open", json!({"path": repo.path_str()}));
    assert_eq!(ws["active"], true);
    assert_eq!(ws["trust"], "untrusted", "全新路径必须先确认信任");
    assert_eq!(ws["identity_changed"], false);
    let real_path = ws["real_path"].as_str().expect("缺少 real_path");
    assert!(real_path.contains("集成 工作区"), "real_path={real_path}");
    assert!(ws["root_commit"].is_string(), "应有首个提交锚点");
    let ws_id = ws["workspace_id"].as_str().expect("缺少 workspace_id").to_string();
    let trusted = sc.ok(
        "workspace.trust",
        json!({"workspace_id": ws_id, "decision": "trust"}),
    );
    assert_eq!(trusted["trust"], "trusted");
    sc.wait_event("workspace.changed(trusted)", |e| {
        e["event"] == "workspace.changed" && e["payload"]["trust"] == "trusted"
    });

    let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);

    // runtime.probe：真实版本探测
    let probe = sc.ok("runtime.probe", json!({"agent": "pi", "config_id": cfg}));
    assert_eq!(probe["version"], "1.4.0");
    assert_eq!(probe["supported"], true);

    sc.start_runtime("pi", &cfg);
    sc.wait_event("runtime.state ready", |e| {
        e["event"] == "runtime.state"
            && e["payload"]["agent"] == "pi"
            && e["payload"]["state"] == "ready"
    });

    let task_id = sc.create_task("pi", &cfg, "写入问候文件");
    let finished = sc.wait_task_finished(&task_id);
    assert_eq!(finished["outcome"], "finished");
    assert_eq!(finished["evidence_version"], 1);

    // ---- 事件序列断言 ----
    let events = sc.events_snapshot();

    // seq 全局单调递增
    let mut prev = 0u64;
    for e in &events {
        let seq = e["seq"].as_u64().expect("事件必须有 seq");
        assert!(seq > prev, "seq 必须严格递增：{prev} -> {seq}");
        prev = seq;
    }

    // phase 顺序：planning → editing → verifying
    let phases: Vec<String> = events
        .iter()
        .filter(|e| e["event"] == "task.phase" && e["task_id"] == task_id.as_str())
        .filter_map(|e| e["payload"]["phase"].as_str().map(str::to_string))
        .collect();
    assert_eq!(phases, ["planning", "editing", "verifying"], "{phases:?}");

    // trace.item：至少含 phase / agent_note / file_hint / verification 四类
    let trace_kinds: Vec<String> = events
        .iter()
        .filter(|e| e["event"] == "trace.item" && e["task_id"] == task_id.as_str())
        .filter_map(|e| e["payload"]["kind"].as_str().map(str::to_string))
        .collect();
    for kind in ["phase", "agent_note", "file_hint", "verification"] {
        assert!(trace_kinds.iter().any(|k| k == kind), "缺少 trace kind {kind}：{trace_kinds:?}");
    }

    // 任务状态机迁移顺序
    let states: Vec<String> = events
        .iter()
        .filter(|e| e["event"] == "task.state" && e["task_id"] == task_id.as_str())
        .filter_map(|e| e["payload"]["state"].as_str().map(str::to_string))
        .collect();
    assert_eq!(
        states,
        ["created", "running", "finishing", "review_ready"],
        "{states:?}"
    );

    // Agent 原生验证结论事件
    let verification = sc.wait_event("task.verification", |e| {
        e["event"] == "task.verification" && e["task_id"] == task_id.as_str()
    });
    assert_eq!(verification["payload"]["status"], "passed");
    assert_eq!(verification["payload"]["source"], "agent");

    // ---- 审查 ----
    let bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(bundle["evidence_version"], 1);
    assert_eq!(bundle["is_latest"], true);
    assert_eq!(bundle["outcome"], "finished");
    assert_eq!(bundle["attribution"], "agent_only");
    assert_eq!(bundle["verification"]["status"], "passed");
    assert_eq!(bundle["verification"]["source"], "agent");

    let files = bundle["files"].as_array().expect("files 应为数组");
    let hello_file = files
        .iter()
        .find(|f| f["path"] == "hello_from_agent.txt")
        .unwrap_or_else(|| panic!("关联变更应包含 hello_from_agent.txt：{files:?}"));
    assert_eq!(hello_file["change"], "added");
    assert!(
        hello_file["diff"].as_str().unwrap_or("").contains("hello from agent"),
        "diff 应包含 Agent 真实写入的内容"
    );

    // 基线脏文件单列，不归因 Agent
    let dirty: Vec<&str> = bundle["baseline_dirty_files"]
        .as_array()
        .expect("baseline_dirty_files 应为数组")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(dirty.contains(&"tracked_dirty.txt"), "{dirty:?}");
    assert!(dirty.contains(&"untracked_dirty.txt"), "{dirty:?}");
    for pre in ["tracked_dirty.txt", "untracked_dirty.txt"] {
        assert!(
            !files.iter().any(|f| f["path"] == pre),
            "基线前修改 {pre} 不得进入关联变更：{files:?}"
        );
    }

    // ---- 接受交付 ----
    let decision = sc.ok(
        "delivery.accept",
        json!({"task_id": task_id, "evidence_version": 1}),
    );
    assert_eq!(decision["decision"]["kind"], "accepted");
    assert_eq!(decision["decision"]["evidence_version"], 1);
    sc.wait_event("task.state accepted", |e| {
        e["event"] == "task.state"
            && e["task_id"] == task_id.as_str()
            && e["payload"]["state"] == "accepted"
    });

    // ---- 历史可见结论 ----
    let history = sc.ok("history.list", json!({"limit": 50}));
    let tasks = history["tasks"].as_array().expect("tasks 应为数组");
    let task_entry = tasks
        .iter()
        .find(|t| t["task_id"] == task_id.as_str())
        .expect("历史应包含该任务");
    assert_eq!(task_entry["state"], "accepted");
    assert_eq!(task_entry["latest_evidence_version"], 1);
    let decisions = history["decisions"].as_array().expect("decisions 应为数组");
    assert!(
        decisions
            .iter()
            .any(|d| d["task_id"] == task_id.as_str() && d["kind"] == "accepted"),
        "历史应包含接受结论：{decisions:?}"
    );

    let status = sc.shutdown();
    assert!(status.success(), "sidecar 应正常退出");
}
