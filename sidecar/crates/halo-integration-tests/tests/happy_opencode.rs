//! 场景 2：OpenCode 完整链路 happy path。
//! 校验回环服务链路、每次启动新认证信息（经 fake 落盘的 SHA-256 摘要对比两次启动
//! 的 token 不同），并断言公开 IPC 状态里无端口、无 token。

mod support;

use serde_json::{json, Value};
use support::{contains_lower_hex_run, fake_opencode_exe, Sidecar, TestRepo};

#[test]
fn opencode_full_chain_with_fresh_token_per_start() {
    let repo = TestRepo::new();
    let digest_dir = tempfile::tempdir().expect("创建摘要目录失败");
    let digest_file = digest_dir.path().join("token 摘要.txt");
    let digest_arg = digest_file.to_string_lossy().to_string();

    let mut sc = Sidecar::start(&[]);
    sc.hello();
    sc.open_and_trust(&repo.path_str());

    let cfg = sc.save_config(
        "opencode",
        &fake_opencode_exe(),
        &["--token-digest-file", &digest_arg],
        None,
    );

    // 第一次启动：健康检查 + 精确版本握手通过后 ready
    sc.start_runtime("opencode", &cfg);
    sc.wait_event("runtime.state ready", |e| {
        e["event"] == "runtime.state"
            && e["payload"]["agent"] == "opencode"
            && e["payload"]["state"] == "ready"
    });

    // 完整任务链路
    let task_id = sc.create_task("opencode", &cfg, "OpenCode 写入问候文件");
    let finished = sc.wait_task_finished(&task_id);
    assert_eq!(finished["outcome"], "finished");
    assert_eq!(finished["evidence_version"], 1);

    let phases: Vec<String> = sc
        .events_snapshot()
        .iter()
        .filter(|e| e["event"] == "task.phase" && e["task_id"] == task_id.as_str())
        .filter_map(|e| e["payload"]["phase"].as_str().map(str::to_string))
        .collect();
    assert_eq!(phases, ["planning", "editing", "verifying"], "{phases:?}");

    let bundle = sc.ok("review.get", json!({"task_id": task_id}));
    assert_eq!(bundle["outcome"], "finished");
    assert!(
        bundle["files"]
            .as_array()
            .expect("files 应为数组")
            .iter()
            .any(|f| f["path"] == "hello_from_agent.txt" && f["change"] == "added"),
        "OpenCode 关联变更应包含真实写入的文件"
    );
    assert_eq!(bundle["verification"]["status"], "passed");

    // runtime.status：每个受管应用独立状态，且公开形状里没有端口/token 字段
    let status = sc.ok("runtime.status", json!({}));
    for agent in ["pi", "opencode"] {
        let info = status[agent].as_object().expect("runtime 状态应为对象");
        let mut keys: Vec<&str> = info.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["reason", "recovery_hint", "state", "version"],
            "RuntimeStateInfo 公开形状只允许契约字段：{agent}"
        );
    }
    assert_eq!(status["opencode"]["state"], "ready");
    assert_eq!(status["opencode"]["version"], "0.4.2");
    assert_eq!(status["pi"]["state"], "not_probed", "Pi 状态不得被合并为全局在线");

    // 第二次启动：先停止，再启动，fake 会把新 token 的摘要追加到同一文件
    let stopped = sc.ok("runtime.stop", json!({"agent": "opencode"}));
    assert_eq!(stopped["state"], "stopped");
    sc.start_runtime("opencode", &cfg);

    let digests: Vec<String> = std::fs::read_to_string(&digest_file)
        .expect("fake-opencode 应已写入 token 摘要文件")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(digests.len(), 2, "两次启动应各写入一行摘要：{digests:?}");
    for d in &digests {
        assert_eq!(d.len(), 64, "SHA-256 摘要应为 64 个十六进制字符：{d}");
        assert!(d.chars().all(|c| c.is_ascii_hexdigit()), "{d}");
    }
    assert_ne!(digests[0], digests[1], "每次启动必须生成全新认证信息");

    // 公开 IPC 里无 token（64 位小写十六进制串）也无端口/token 字段泄漏
    for line in sc.transcript_snapshot() {
        assert!(
            !contains_lower_hex_run(&line, 64),
            "IPC 行疑似泄漏 token：{line}"
        );
        assert!(!line.contains("HALO_OC_TOKEN"), "IPC 行不得出现 token 变量名：{line}");
        let v: Value = match serde_json::from_str(line.trim_start_matches(['<', '>', ' '])) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let rendered = v.to_string();
        assert!(!rendered.contains("\"port\""), "IPC 消息不得携带端口字段：{line}");
        assert!(!rendered.contains("\"token\""), "IPC 消息不得携带 token 字段：{line}");
    }

    let status = sc.shutdown();
    assert!(status.success());
}
