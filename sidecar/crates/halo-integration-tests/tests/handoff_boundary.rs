//! 场景 8：交接包边界。
//! 包 JSON 只含契约白名单字段：不含 instructions 之外的对话内容、不含原始日志
//! （trace 文本）、不含凭据引用之外的任何秘密字段；运行中任务交接 → TASK_STILL_RUNNING。

mod support;

use serde_json::{json, Value};
use support::{fake_pi_exe, Sidecar, TestRepo};

/// 断言 JSON 对象的键集合恰好等于（或属于）契约白名单。
fn assert_keys_exact(v: &Value, expect: &[&str], what: &str) {
    let obj = v.as_object().unwrap_or_else(|| panic!("{what} 应为对象：{v}"));
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut expect: Vec<&str> = expect.to_vec();
    expect.sort_unstable();
    assert_eq!(keys, expect, "{what} 的字段必须与契约完全一致");
}

#[test]
fn handoff_package_is_limited_context_only() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "交接边界任务");
    sc.wait_task_finished(&task_id);

    // preview：handoff_id/created_at 为 null，selected_files 省略 = 默认全部关联文件
    let preview = sc.ok("handoff.preview", json!({"task_id": task_id}));
    let package = &preview["package"];
    assert_keys_exact(
        package,
        &[
            "handoff_id",
            "task_id",
            "source_agent",
            "target_agent",
            "goal",
            "summary",
            "selected_changes",
            "verification",
            "created_at",
        ],
        "HandoffPackage",
    );
    assert_eq!(package["handoff_id"], Value::Null);
    assert_eq!(package["created_at"], Value::Null);
    assert_eq!(package["source_agent"], "pi");
    assert_eq!(package["goal"], "在工作区写入 hello_from_agent.txt", "goal 只来自任务目标");
    for change in package["selected_changes"].as_array().expect("应为数组") {
        assert_keys_exact(change, &["path", "diff"], "SelectedChange");
    }
    assert_keys_exact(&package["verification"], &["status", "detail"], "HandoffVerification");
    assert_eq!(package["verification"]["status"], "passed");

    // create：选定文件白名单 + 目标 Agent
    let created = sc.ok(
        "handoff.create",
        json!({
            "task_id": task_id,
            "target_agent": "opencode",
            "selected_files": ["hello_from_agent.txt"]
        }),
    );
    let package = created["package"].clone();
    assert!(created["handoff_id"].as_str().unwrap_or_default().starts_with("ho-"));
    assert_eq!(package["target_agent"], "opencode");
    assert!(package["created_at"].is_string());
    let changes = package["selected_changes"].as_array().expect("应为数组");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["path"], "hello_from_agent.txt");

    // 包 JSON 整体不得携带对话/原始日志/配置或凭据形态的内容
    let rendered = package.to_string();
    for forbidden in [
        "规划中",              // 运行轨迹 phase 文本（原始过程日志）
        "准备在工作区写入",    // agent_note 对话式输出
        "credential_ref",       // 启动配置字段
        "executable_path",      // 启动配置字段
        "env_overrides",        // 启动配置字段
        "HALO_PROVIDER_API_KEY",
        "instructions",         // 包内目标字段名固定为 goal
    ] {
        assert!(
            !rendered.contains(forbidden),
            "交接包不得携带“{forbidden}”：{rendered}"
        );
    }

    // 白名单外路径静默忽略，不成为夹带通道
    let sneaky = sc.ok(
        "handoff.preview",
        json!({"task_id": task_id, "selected_files": ["../../secrets.txt", "不存在.txt"]}),
    );
    assert_eq!(
        sneaky["package"]["selected_changes"].as_array().map(Vec::len),
        Some(0),
        "证据之外的路径不得进入交接包"
    );
}

#[test]
fn handoff_rejected_while_task_running() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config(
        "pi",
        &fake_pi_exe(),
        &["--mode", "happy", "--step-delay-ms", "200"],
        None,
    );
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, "运行中不可交接");
    sc.wait_event("task.phase planning", |e| {
        e["event"] == "task.phase" && e["task_id"] == task_id.as_str()
    });

    sc.err(
        "handoff.preview",
        json!({"task_id": task_id}),
        "TASK_STILL_RUNNING",
    );
    sc.err(
        "handoff.create",
        json!({"task_id": task_id, "target_agent": "opencode", "selected_files": []}),
        "TASK_STILL_RUNNING",
    );

    // 收尾：等待任务结束后交接立即可用
    sc.wait_task_finished(&task_id);
    let preview = sc.ok("handoff.preview", json!({"task_id": task_id}));
    assert_eq!(preview["package"]["task_id"], task_id.as_str());
}
