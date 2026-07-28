//! 场景 2：OpenCode 1.x 受管启动闭环。
//! 校验真实回环服务、每次启动新 Basic 认证、兼容性档案和独立运行时状态；
//! 本票不得经旧 `/task` 协议伪造任务完成。

mod support;

use std::time::Duration;

use serde_json::json;
use support::{
    contains_lower_hex_run, fake_opencode_exe, require_test_credential, wait_process_lock_held,
    wait_process_lock_released, Sidecar, TestRepo,
};

#[test]
fn opencode_1x_starts_with_fresh_basic_authentication_and_keeps_pi_unprobed() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let digest_dir = tempfile::tempdir().expect("创建摘要目录失败");
    let digest_file = digest_dir.path().join("password 摘要.txt");
    let digest_arg = digest_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[
        ("HALO_SHUTDOWN_GRACE_MS", "200"),
        ("HALO_CANCEL_GRACE_MS", "200"),
    ]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let config_id = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &[
            "--mode",
            "initial_busy_then_idle",
            "--password-digest-file",
            &digest_arg,
            "--require-credential-env",
            "OPENAI_API_KEY",
            "--require-isolated-state",
        ],
        Some(credential.reference()),
    );

    sc.start_runtime("opencode", &config_id);
    sc.wait_event("runtime.state ready", |event| {
        event["event"] == "runtime.state"
            && event["payload"]["agent"] == "opencode"
            && event["payload"]["state"] == "ready"
    });

    let status = sc.ok("runtime.status", json!({}));
    assert_eq!(status["opencode"]["state"], "ready");
    assert_eq!(status["opencode"]["version"], "1.18.5");
    assert_eq!(
        status["pi"]["state"], "not_probed",
        "Pi 状态必须独立如实呈现"
    );
    for agent in ["pi", "opencode"] {
        let info = status[agent].as_object().expect("runtime 状态应为对象");
        let mut keys: Vec<&str> = info.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["reason", "recovery_hint", "state", "version"]);
    }

    let created = sc.ok(
        "task.create",
        json!({
            "agent": "opencode", "config_id": config_id, "title": "真实首轮会话",
            "instructions": "在工作区写入 hello_from_agent.txt"
        }),
    );
    let task_id = created["task"]["task_id"]
        .as_str()
        .expect("task.create 必须返回任务标识")
        .to_string();
    assert_eq!(created["task"]["state"], "running");

    let user_message = sc.wait_event("首条用户会话消息", |event| {
        event["event"] == "task.session_message"
            && event["task_id"] == task_id
            && event["payload"]["role"] == "user"
    });
    assert_eq!(
        user_message["payload"]["text"],
        "在工作区写入 hello_from_agent.txt"
    );
    let agent_message = sc.wait_event("首条 Agent 会话回复", |event| {
        event["event"] == "task.session_message"
            && event["task_id"] == task_id
            && event["payload"]["role"] == "agent"
    });
    assert_eq!(
        agent_message["payload"]["text"],
        "fake-opencode 已完成首轮回复。"
    );
    assert!(!agent_message["payload"]
        .to_string()
        .contains("原始工具输出"));
    assert!(!agent_message["payload"]
        .to_string()
        .contains("不会作为活动会话回复"));

    let waiting = sc.wait_event("等待开发者状态", |event| {
        event["event"] == "task.state"
            && event["task_id"] == task_id
            && event["payload"]["state"] == "waiting_developer"
    });
    assert_eq!(waiting["payload"]["task"]["state"], "waiting_developer");

    let snapshot = sc.ok("task.snapshot", json!({"after_seq": 0}));
    assert_eq!(snapshot["task"]["state"], "waiting_developer");
    assert_eq!(
        snapshot["session_messages"],
        json!([
            {"role": "user", "text": "在工作区写入 hello_from_agent.txt", "truncated": false},
            {"role": "agent", "text": "fake-opencode 已完成首轮回复。", "truncated": false}
        ])
    );
    assert!(snapshot["task"].get("session_messages").is_none());
    let status = sc.ok("task.status", json!({"task_id": task_id}));
    assert_eq!(status["task"]["state"], "waiting_developer");
    assert!(status["task"].get("session_messages").is_none());
    let traces = sc.events.lock().unwrap();
    assert!(traces.iter().any(|event| {
        event["event"] == "trace.item"
            && event["task_id"] == task_id
            && event["payload"]["kind"] == "agent_note"
    }));
    assert!(traces.iter().any(|event| {
        event["event"] == "trace.item"
            && event["task_id"] == task_id
            && event["payload"]["kind"] == "file_hint"
    }));
    assert!(
        !traces
            .iter()
            .any(|event| event["event"] == "task.finished" && event["task_id"] == task_id),
        "首轮回复不得自动生成交付终态"
    );
    drop(traces);

    let cancelled = sc.ok("task.cancel", json!({"task_id": task_id}));
    assert_eq!(cancelled["accepted"], true);
    sc.wait_event("取消等待中的 OpenCode 任务", |event| {
        event["event"] == "task.cancelled" && event["task_id"] == task_id
    });

    let stopped = sc.ok("runtime.stop", json!({"agent": "opencode"}));
    assert_eq!(stopped["state"], "stopped");
    sc.start_runtime("opencode", &config_id);

    let digests: Vec<String> = std::fs::read_to_string(&digest_file)
        .expect("fake-opencode 应已写入认证摘要")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(digests.len(), 2, "两次启动应各记录一个密码摘要");
    assert_ne!(digests[0], digests[1], "每次启动必须生成新的 Basic 密码");
    for digest in &digests {
        assert_eq!(digest.len(), 64);
        assert!(digest
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
    }

    // 公开 IPC 不得暴露认证变量、端口或 Basic Authorization 内容。
    for line in sc.transcript_snapshot() {
        assert!(
            !line.contains("OPENCODE_SERVER_PASSWORD"),
            "IPC 泄漏认证变量名"
        );
        assert!(!line.contains("Authorization"), "IPC 泄漏认证头");
        assert!(!line.contains("\"port\""), "IPC 泄漏回环端口字段");
        assert!(
            !contains_lower_hex_run(&line, 64),
            "IPC 疑似泄漏本次 OpenCode 认证信息"
        );
    }

    assert!(sc.shutdown().success());
}

#[test]
fn opencode_stop_forces_process_exit_after_global_dispose_variants() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let locks = tempfile::tempdir().expect("创建锁目录失败");
    let mut sc = Sidecar::start(&[("HALO_SHUTDOWN_GRACE_MS", "200")]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());

    for mode in ["happy", "dispose_failure", "hang_on_dispose"] {
        let lock_file = locks.path().join(format!("{mode}.lock"));
        let lock_arg = lock_file.to_string_lossy().to_string();
        let dispose_marker = locks.path().join(format!("{mode}.dispose"));
        let dispose_marker_arg = dispose_marker.to_string_lossy().to_string();
        let config_id = sc.save_config(
            "opencode",
            &fake_opencode_exe(),
            &[
                "--mode",
                mode,
                "--lock-file",
                &lock_arg,
                "--dispose-marker-file",
                &dispose_marker_arg,
            ],
            Some(credential.reference()),
        );
        sc.start_runtime("opencode", &config_id);
        assert!(
            wait_process_lock_held(&lock_file, Duration::from_secs(5)),
            "{mode} OpenCode 进程应持有锁文件"
        );

        let stopped = sc.ok("runtime.stop", json!({"agent": "opencode"}));
        assert_eq!(stopped["state"], "stopped");
        assert_eq!(
            std::fs::read_to_string(&dispose_marker).as_deref(),
            Ok("global_dispose"),
            "{mode} 停止必须调用 OpenCode 的 /global/dispose"
        );
        assert!(
            wait_process_lock_released(&lock_file, Duration::from_secs(5)),
            "{mode} dispose 后 Sidecar 必须强制回收 OpenCode 进程"
        );
    }

    assert!(sc.shutdown().success());
}

#[test]
fn opencode_missing_busy_event_uses_status_snapshot_to_complete_the_round() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[("HALO_SHUTDOWN_GRACE_MS", "200")]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let config_id = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &[
            "--mode",
            "missing_busy_eof",
            "--require-credential-env",
            "OPENAI_API_KEY",
            "--require-isolated-state",
        ],
        Some(credential.reference()),
    );
    sc.start_runtime("opencode", &config_id);
    sc.wait_event("runtime.state ready", |event| {
        event["event"] == "runtime.state"
            && event["payload"]["agent"] == "opencode"
            && event["payload"]["state"] == "ready"
    });

    let created = sc.ok(
        "task.create",
        json!({
            "agent": "opencode", "config_id": config_id, "title": "缺失 busy 的会话",
            "instructions": "验证受管状态快照回退"
        }),
    );
    let task_id = created["task"]["task_id"]
        .as_str()
        .expect("task.create 必须返回任务标识")
        .to_string();
    let agent_message = sc.wait_event("缺失 busy 后仍取得首轮回复", |event| {
        event["event"] == "task.session_message"
            && event["task_id"] == task_id
            && event["payload"]["role"] == "agent"
    });
    assert_eq!(
        agent_message["payload"]["text"],
        "fake-opencode 已完成首轮回复。"
    );
    let waiting = sc.wait_event("缺失 busy 后进入等待开发者", |event| {
        event["event"] == "task.state"
            && event["task_id"] == task_id
            && event["payload"]["state"] == "waiting_developer"
    });
    assert_eq!(waiting["payload"]["task"]["state"], "waiting_developer");
    assert!(
        !sc.events
            .lock()
            .unwrap()
            .iter()
            .any(|event| { event["event"] == "task.finished" && event["task_id"] == task_id }),
        "状态快照回退不得把首轮自动送进交付终态"
    );
    let cancelled = sc.ok("task.cancel", json!({"task_id": task_id}));
    assert_eq!(cancelled["accepted"], true);
    sc.wait_event("取消回退后的等待任务", |event| {
        event["event"] == "task.cancelled" && event["task_id"] == task_id
    });
    assert!(sc.shutdown().success());
}

#[test]
fn opencode_fast_initial_round_reaches_waiting_developer_after_sse_closes() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[("HALO_SHUTDOWN_GRACE_MS", "200")]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let config_id = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &[
            "--mode",
            "fast_initial_round",
            "--require-credential-env",
            "OPENAI_API_KEY",
            "--require-isolated-state",
        ],
        Some(credential.reference()),
    );
    sc.start_runtime("opencode", &config_id);
    sc.wait_event("runtime.state ready", |event| {
        event["event"] == "runtime.state"
            && event["payload"]["agent"] == "opencode"
            && event["payload"]["state"] == "ready"
    });

    let created = sc.ok(
        "task.create",
        json!({
            "agent": "opencode", "config_id": config_id, "title": "极速完成的首轮会话",
            "instructions": "验证首轮极速完成后仍等待开发者"
        }),
    );
    let task_id = created["task"]["task_id"]
        .as_str()
        .expect("task.create 必须返回任务标识")
        .to_string();
    let agent_message = sc.wait_event("极速完成后仍取得首轮回复", |event| {
        event["event"] == "task.session_message"
            && event["task_id"] == task_id
            && event["payload"]["role"] == "agent"
    });
    assert_eq!(
        agent_message["payload"]["text"],
        "fake-opencode 已完成首轮回复。"
    );
    let waiting = sc.wait_event("极速完成后进入等待开发者", |event| {
        event["event"] == "task.state"
            && event["task_id"] == task_id
            && event["payload"]["state"] == "waiting_developer"
    });
    assert_eq!(waiting["payload"]["task"]["state"], "waiting_developer");

    let events = sc.events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| {
                event["event"] == "task.session_message"
                    && event["task_id"] == task_id
                    && event["payload"]["role"] == "agent"
            })
            .count(),
        1,
        "重复 idle 与 EOF 不得重复追加 Agent 回复"
    );
    assert!(
        !events
            .iter()
            .any(|event| event["event"] == "task.finished" && event["task_id"] == task_id),
        "极速首轮也不得自动产生交付终态"
    );
    drop(events);

    let cancelled = sc.ok("task.cancel", json!({"task_id": task_id}));
    assert_eq!(cancelled["accepted"], true);
    sc.wait_event("取消极速完成后的等待任务", |event| {
        event["event"] == "task.cancelled" && event["task_id"] == task_id
    });
    assert!(sc.shutdown().success());
}

#[test]
fn opencode_permission_requires_an_exact_one_time_decision_before_resuming() {
    let credential = require_test_credential();
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[
        ("HALO_SHUTDOWN_GRACE_MS", "200"),
        ("HALO_CANCEL_GRACE_MS", "200"),
    ]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let config_id = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &[
            "--mode",
            "permission_once",
            "--require-credential-env",
            "OPENAI_API_KEY",
            "--require-isolated-state",
        ],
        Some(credential.reference()),
    );
    sc.start_runtime("opencode", &config_id);
    sc.wait_event("runtime.state ready", |event| {
        event["event"] == "runtime.state"
            && event["payload"]["agent"] == "opencode"
            && event["payload"]["state"] == "ready"
    });

    let created = sc.ok(
        "task.create",
        json!({
            "agent": "opencode", "config_id": config_id, "title": "一次性权限",
            "instructions": "请求一次写入权限后继续"
        }),
    );
    let task_id = created["task"]["task_id"]
        .as_str()
        .expect("task.create 必须返回任务标识")
        .to_string();
    let action = sc.wait_event("OpenCode 权限请求", |event| {
        event["event"] == "task.action_request"
            && event["task_id"] == task_id
            && event["payload"]["kind"] == "permission"
    });
    let request_id = action["payload"]["request_id"]
        .as_str()
        .expect("权限请求必须有可决议的标识")
        .to_string();
    assert_eq!(action["payload"]["decision_sent"], false);

    let awaiting = sc.wait_event("awaiting_action", |event| {
        event["event"] == "task.state"
            && event["task_id"] == task_id
            && event["payload"]["state"] == "awaiting_action"
    });
    assert_eq!(awaiting["payload"]["task"]["state"], "awaiting_action");
    let snapshot = sc.ok("task.snapshot", json!({"after_seq": 0}));
    assert_eq!(
        snapshot["action_requests"],
        json!([{
            "request_id": request_id,
            "kind": "permission",
            "prompt": "OpenCode 请求本次 edit 权限：src/fake.rs",
            "decision_sent": false
        }])
    );

    sc.err(
        "task.resolve_action",
        json!({
            "task_id": task_id,
            "request_id": "per_other",
            "decision": "allow_once",
            "answer": null
        }),
        "ACTION_REQUEST_NOT_FOUND",
    );

    let accepted = sc.ok(
        "task.resolve_action",
        json!({
            "task_id": task_id,
            "request_id": request_id,
            "decision": "allow_once",
            "answer": null
        }),
    );
    assert_eq!(accepted, json!({"accepted": true}));
    sc.err(
        "task.resolve_action",
        json!({
            "task_id": task_id,
            "request_id": request_id,
            "decision": "allow_once",
            "answer": null
        }),
        "ACTION_REQUEST_ALREADY_RESOLVED",
    );

    let resolved = sc.wait_event("精确权限回执", |event| {
        event["event"] == "task.action_resolved" && event["task_id"] == task_id
    });
    assert_eq!(resolved["payload"], json!({"request_id": request_id}));

    let action_seq = action["seq"].as_u64().expect("事件应有 seq");
    sc.wait_event("真实权限反馈后的 running", |event| {
        event["event"] == "task.state"
            && event["task_id"] == task_id
            && event["payload"]["state"] == "running"
            && event["seq"].as_u64().is_some_and(|seq| seq > action_seq)
    });
    sc.wait_event("权限后首轮 Agent 回复", |event| {
        event["event"] == "task.session_message"
            && event["task_id"] == task_id
            && event["payload"]["role"] == "agent"
    });
    sc.wait_event("权限后等待开发者", |event| {
        event["event"] == "task.state"
            && event["task_id"] == task_id
            && event["payload"]["state"] == "waiting_developer"
    });

    let final_snapshot = sc.ok("task.snapshot", json!({"after_seq": 0}));
    assert_eq!(final_snapshot["action_requests"], json!([]));
    for line in sc.transcript_snapshot() {
        assert!(!line.contains("\"always\""), "不得暴露永久放行决议");
        assert!(!line.contains("ses_fake_"), "远程会话标识不得进入 IPC");
    }

    let cancelled = sc.ok("task.cancel", json!({"task_id": task_id}));
    assert_eq!(cancelled["accepted"], true);
    sc.wait_event("取消等待中的任务", |event| {
        event["event"] == "task.cancelled" && event["task_id"] == task_id
    });
    assert!(sc.shutdown().success());
}
