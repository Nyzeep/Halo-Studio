//! halo-store 公共 API 行为测试（tempfile 目录建库）。

use std::collections::BTreeMap;

use halo_store::{
    DecisionRecord, EvidenceDraft, FileChangeDraft, HandoffRecord, LaunchConfigRecord,
    SelectedChangeRecord, Store, StoreLimits, TaskRecord, TrustRecord,
};

fn task(id: &str, state: &str, created_at: &str) -> TaskRecord {
    TaskRecord {
        task_id: id.to_owned(),
        agent: "pi".to_owned(),
        title: format!("任务 {id}"),
        goal: format!("任务 {id} 的详细目标"),
        state: state.to_owned(),
        attribution: "agent_only".to_owned(),
        manual_edit_paths: vec![],
        baseline_head: Some("abc123".to_owned()),
        baseline_captured_at: "2026-07-26T08:00:00Z".to_owned(),
        created_at: created_at.to_owned(),
        ended_at: None,
        cancel_mode: None,
    }
}

fn draft(summary: &str, files: Vec<FileChangeDraft>) -> EvidenceDraft {
    EvidenceDraft {
        outcome: "finished".to_owned(),
        attribution: "agent_only".to_owned(),
        attribution_reasons: vec![],
        summary: summary.to_owned(),
        files,
        verification_status: "passed".to_owned(),
        verification_detail: "ok".to_owned(),
        verification_source: "agent".to_owned(),
        baseline_dirty_files: vec!["docs/x.md".to_owned()],
        created_at: "2026-07-26T08:05:00Z".to_owned(),
    }
}

fn file(path: &str, diff: &str) -> FileChangeDraft {
    FileChangeDraft {
        path: path.to_owned(),
        change: "modified".to_owned(),
        diff: diff.to_owned(),
        end_hash: None,
    }
}

#[test]
fn open_is_idempotent_and_keeps_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("halo.db");
    {
        let store = Store::open(&path, StoreLimits::default()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 3);
        store
            .put_task(&task("task-1", "running", "2026-07-26T08:00:00Z"))
            .unwrap();
        assert_eq!(store.append_evidence("task-1", &draft("s", vec![])).unwrap(), 1);
    }
    // 第二次 open：迁移不得重复执行，数据完整保留
    let store = Store::open(&path, StoreLimits::default()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 3);
    assert_eq!(store.get_task("task-1").unwrap().unwrap().state, "running");
    assert_eq!(store.latest_evidence("task-1").unwrap().unwrap().version, 1);
    drop(store);
    // 第三次仍幂等
    let store = Store::open(&path, StoreLimits::default()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 3);
}

#[test]
fn append_evidence_assigns_incrementing_versions_per_task() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();

    assert_eq!(store.append_evidence("task-1", &draft("v1", vec![])).unwrap(), 1);
    assert_eq!(store.append_evidence("task-1", &draft("v2", vec![])).unwrap(), 2);
    // 版本号按任务独立编号
    assert_eq!(store.append_evidence("task-2", &draft("其他任务", vec![])).unwrap(), 1);

    let all = store.list_evidence("task-1").unwrap();
    assert_eq!(all.iter().map(|e| e.version).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(all[0].summary, "v1");
    assert_eq!(all[1].summary, "v2");

    let latest = store.latest_evidence("task-1").unwrap().unwrap();
    assert_eq!(latest.version, 2);
    assert_eq!(latest.summary, "v2");
    // 旧版本原样保留，未被覆盖
    assert_eq!(store.list_evidence("task-1").unwrap()[0].summary, "v1");
}

#[test]
fn evidence_roundtrip_keeps_all_fields() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();

    let mut d = draft("完整字段", vec![file("src/a.rs", "-a\n+b\n")]);
    d.attribution = "mixed".to_owned();
    d.attribution_reasons = vec!["用户于 08:12 标记人工编辑".to_owned()];
    d.outcome = "failed".to_owned();
    d.verification_status = "failed".to_owned();
    d.verification_detail = "2 个用例失败".to_owned();
    d.verification_source = "agent".to_owned();
    d.files[0].end_hash = Some("sha256:abc".to_owned());

    store.append_evidence("task-9", &d).unwrap();
    let rec = store.latest_evidence("task-9").unwrap().unwrap();
    assert_eq!(rec.task_id, "task-9");
    assert_eq!(rec.outcome, "failed");
    assert_eq!(rec.attribution, "mixed");
    assert_eq!(rec.attribution_reasons, vec!["用户于 08:12 标记人工编辑"]);
    assert_eq!(rec.summary, "完整字段");
    assert!(!rec.summary_truncated);
    assert_eq!(rec.files.len(), 1);
    assert_eq!(rec.files[0].path, "src/a.rs");
    assert_eq!(rec.files[0].change, "modified");
    assert_eq!(rec.files[0].diff, "-a\n+b\n");
    assert!(!rec.files[0].truncated);
    assert_eq!(rec.files[0].end_hash.as_deref(), Some("sha256:abc"));
    assert_eq!(rec.verification_status, "failed");
    assert_eq!(rec.verification_detail, "2 个用例失败");
    assert_eq!(rec.verification_source, "agent");
    assert_eq!(rec.baseline_dirty_files, vec!["docs/x.md"]);
    assert!(!rec.truncated);
    assert_eq!(rec.created_at, "2026-07-26T08:05:00Z");
}

#[test]
fn oversized_text_is_truncated_and_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let limits = StoreLimits {
        summary_max_bytes: 8,
        file_diff_max_bytes: 10,
        version_total_max_bytes: 16,
        trace_text_max_bytes: 6,
    };
    let store = Store::open(&dir.path().join("halo.db"), limits).unwrap();

    let mut d = draft(
        "0123456789ABCDEF",
        vec![file("a.rs", &"d".repeat(30)), file("b.rs", &"e".repeat(30))],
    );
    d.verification_detail = "详细验证输出很长".to_owned();
    store.append_evidence("task-1", &d).unwrap();

    let rec = store.latest_evidence("task-1").unwrap().unwrap();
    // summary 按 summary_max 截断并标记
    assert_eq!(rec.summary, "01234567");
    assert!(rec.summary_truncated);
    // 单文件 diff 按 file_diff_max 截断
    assert_eq!(rec.files[0].diff.len(), 10);
    assert!(rec.files[0].truncated);
    // 版本总量预算：第二个文件只剩 16-10=6 字节
    assert_eq!(rec.files[1].diff.len(), 6);
    assert!(rec.files[1].truncated);
    // verification_detail 按 trace 上限截断（UTF-8 边界内）
    assert!(rec.verification_detail.len() <= 6);
    // 版本级 truncated 汇总标记
    assert!(rec.truncated);
}

#[test]
fn truncation_respects_utf8_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let limits = StoreLimits {
        summary_max_bytes: 8,
        ..StoreLimits::default()
    };
    let store = Store::open(&dir.path().join("halo.db"), limits).unwrap();
    store
        .append_evidence("task-1", &draft("你好世界", vec![]))
        .unwrap();
    let rec = store.latest_evidence("task-1").unwrap().unwrap();
    // 每个汉字 3 字节，上限 8 落在字符中间，回退到 6 字节边界
    assert_eq!(rec.summary, "你好");
    assert!(rec.summary_truncated);
}

#[test]
fn within_limit_text_is_not_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();
    store
        .append_evidence("task-1", &draft("正常摘要", vec![file("a.rs", "-x\n+y\n")]))
        .unwrap();
    let rec = store.latest_evidence("task-1").unwrap().unwrap();
    assert!(!rec.summary_truncated);
    assert!(!rec.files[0].truncated);
    assert!(!rec.truncated);
}

#[test]
fn default_limits_match_contract() {
    let limits = StoreLimits::default();
    assert_eq!(limits.summary_max_bytes, 16 * 1024);
    assert_eq!(limits.file_diff_max_bytes, 256 * 1024);
    assert_eq!(limits.version_total_max_bytes, 4 * 1024 * 1024);
    assert_eq!(limits.trace_text_max_bytes, 4 * 1024);
}

#[test]
fn mark_non_terminal_interrupted_only_touches_non_terminal_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();

    let non_terminal = ["created", "running", "awaiting_action", "finishing", "review_ready"];
    let terminal = ["accepted", "rejected", "cancelled", "failed", "interrupted"];
    for (i, state) in non_terminal.iter().chain(terminal.iter()).enumerate() {
        store
            .put_task(&task(&format!("task-{i:02}"), state, "2026-07-26T08:00:00Z"))
            .unwrap();
    }

    let mut affected = store.mark_non_terminal_interrupted().unwrap();
    affected.sort();
    assert_eq!(
        affected,
        vec!["task-00", "task-01", "task-02", "task-03", "task-04"]
    );

    // 非终态任务全部置 interrupted，并补记 ended_at
    for id in &affected {
        let t = store.get_task(id).unwrap().unwrap();
        assert_eq!(t.state, "interrupted");
        assert!(t.ended_at.is_some(), "{id} 应补记 ended_at");
    }
    // 终态任务保持原状
    for (i, state) in terminal.iter().enumerate() {
        let t = store.get_task(&format!("task-{:02}", i + 5)).unwrap().unwrap();
        assert_eq!(&t.state, state);
        assert!(t.ended_at.is_none());
    }
    // 再次调用：已无非终态任务
    assert!(store.mark_non_terminal_interrupted().unwrap().is_empty());
}

#[test]
fn decisions_roundtrip_latest_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();

    let d1 = DecisionRecord {
        kind: "accepted".to_owned(),
        task_id: "task-1".to_owned(),
        evidence_version: 1,
        decided_at: "2026-07-26T09:00:00Z".to_owned(),
        reason: None,
        reason_truncated: false,
    };
    let d2 = DecisionRecord {
        kind: "rejected".to_owned(),
        task_id: "task-2".to_owned(),
        evidence_version: 3,
        decided_at: "2026-07-26T09:10:00Z".to_owned(),
        reason: Some("验证未通过，需要修复用例".to_owned()),
        reason_truncated: false,
    };
    store.put_decision(&d1).unwrap();
    store.put_decision(&d2).unwrap();

    let listed = store.list_decisions(10).unwrap();
    assert_eq!(listed, vec![d2.clone(), d1.clone()]);
    // limit 生效
    assert_eq!(store.list_decisions(1).unwrap(), vec![d2]);
}

#[test]
fn decision_reason_is_capped_with_flag() {
    let dir = tempfile::tempdir().unwrap();
    let limits = StoreLimits {
        trace_text_max_bytes: 4,
        ..StoreLimits::default()
    };
    let store = Store::open(&dir.path().join("halo.db"), limits).unwrap();
    store
        .put_decision(&DecisionRecord {
            kind: "rejected".to_owned(),
            task_id: "task-1".to_owned(),
            evidence_version: 1,
            decided_at: "2026-07-26T09:00:00Z".to_owned(),
            reason: Some("abcdefgh".to_owned()),
            reason_truncated: false,
        })
        .unwrap();
    let listed = store.list_decisions(1).unwrap();
    assert_eq!(listed[0].reason.as_deref(), Some("abcd"));
    assert!(listed[0].reason_truncated);
}

#[test]
fn handoffs_roundtrip_and_upsert() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();

    let h = HandoffRecord {
        handoff_id: "ho-1".to_owned(),
        task_id: "task-1".to_owned(),
        source_agent: "pi".to_owned(),
        target_agent: Some("opencode".to_owned()),
        goal: "修复登录超时".to_owned(),
        goal_truncated: false,
        summary: "已定位到会话过期逻辑".to_owned(),
        summary_truncated: false,
        selected_changes: vec![SelectedChangeRecord {
            path: "src/auth.rs".to_owned(),
            diff: "-old\n+new\n".to_owned(),
            truncated: false,
        }],
        verification_status: "passed".to_owned(),
        verification_detail: "全部用例通过".to_owned(),
        truncated: false,
        created_at: "2026-07-26T10:00:00Z".to_owned(),
    };
    store.put_handoff(&h).unwrap();
    assert_eq!(store.get_handoff("ho-1").unwrap().unwrap(), h);
    assert!(store.get_handoff("ho-missing").unwrap().is_none());

    // 同 id 覆盖写（交接包非追加式记录）
    let mut h2 = h.clone();
    h2.target_agent = None;
    h2.summary = "更新后的摘要".to_owned();
    store.put_handoff(&h2).unwrap();
    assert_eq!(store.get_handoff("ho-1").unwrap().unwrap(), h2);
}

#[test]
fn handoff_oversized_content_is_capped_and_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let limits = StoreLimits {
        summary_max_bytes: 8,
        file_diff_max_bytes: 10,
        version_total_max_bytes: 16,
        trace_text_max_bytes: 6,
    };
    let store = Store::open(&dir.path().join("halo.db"), limits).unwrap();

    let h = HandoffRecord {
        handoff_id: "ho-big".to_owned(),
        task_id: "task-1".to_owned(),
        source_agent: "pi".to_owned(),
        target_agent: Some("opencode".to_owned()),
        goal: "G".repeat(20),
        goal_truncated: false,
        summary: "S".repeat(20),
        summary_truncated: false,
        selected_changes: vec![
            SelectedChangeRecord {
                path: "a.rs".to_owned(),
                diff: "d".repeat(30),
                truncated: false,
            },
            SelectedChangeRecord {
                path: "b.rs".to_owned(),
                diff: "e".repeat(30),
                truncated: false,
            },
        ],
        verification_status: "passed".to_owned(),
        verification_detail: "detail-long".to_owned(),
        truncated: false,
        created_at: "2026-07-26T10:00:00Z".to_owned(),
    };
    store.put_handoff(&h).unwrap();

    let rec = store.get_handoff("ho-big").unwrap().unwrap();
    assert_eq!(rec.goal.len(), 8);
    assert!(rec.goal_truncated);
    assert_eq!(rec.summary.len(), 8);
    assert!(rec.summary_truncated);
    assert_eq!(rec.selected_changes[0].diff.len(), 10);
    assert!(rec.selected_changes[0].truncated);
    // 总量预算：第二个变更只剩 6 字节
    assert_eq!(rec.selected_changes[1].diff.len(), 6);
    assert!(rec.selected_changes[1].truncated);
    assert!(rec.verification_detail.len() <= 6);
    assert!(rec.truncated);
}

#[test]
fn trust_roundtrip_update_and_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();
    // 含空格与中文的 Windows 路径必须原样往返
    let real_path = r"D:\Halo Studio ultra\示例 仓库";

    assert!(store.get_trust(real_path).unwrap().is_none());

    let rec = TrustRecord {
        real_path: real_path.to_owned(),
        root_commit: Some("deadbeef".to_owned()),
        trusted: true,
        decided_at: "2026-07-26T08:00:00Z".to_owned(),
    };
    store.put_trust(&rec).unwrap();
    assert_eq!(store.get_trust(real_path).unwrap().unwrap(), rec);

    // 同键更新（例如目录重建后 root_commit 变化并降级）
    let updated = TrustRecord {
        root_commit: Some("cafebabe".to_owned()),
        trusted: false,
        decided_at: "2026-07-26T09:00:00Z".to_owned(),
        ..rec.clone()
    };
    store.put_trust(&updated).unwrap();
    assert_eq!(store.get_trust(real_path).unwrap().unwrap(), updated);

    store.revoke_trust(real_path).unwrap();
    assert!(store.get_trust(real_path).unwrap().is_none());
    // 重复撤销为无害空操作
    store.revoke_trust(real_path).unwrap();
}

#[test]
fn launch_configs_roundtrip_update_delete() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();

    let mut env = BTreeMap::new();
    env.insert("PATH".to_owned(), r"C:\tools".to_owned());
    let cfg = LaunchConfigRecord {
        config_id: "cfg-1".to_owned(),
        name: "Pi + GPT".to_owned(),
        agent: "pi".to_owned(),
        executable_path: r"C:\tools\pi\pi.exe".to_owned(),
        model: "gpt-5".to_owned(),
        thinking_level: "medium".to_owned(),
        // 只存凭据引用名，绝无明文
        credential_ref: Some("halo/pi/openai".to_owned()),
        extra_args: vec!["--verbose".to_owned()],
        env_overrides: env,
        created_at: "2026-07-26T08:00:00Z".to_owned(),
        updated_at: "2026-07-26T08:00:00Z".to_owned(),
    };
    store.put_config(&cfg).unwrap();
    assert_eq!(store.list_configs().unwrap(), vec![cfg.clone()]);

    let mut cfg2 = cfg.clone();
    cfg2.model = "gpt-5-mini".to_owned();
    cfg2.updated_at = "2026-07-26T09:00:00Z".to_owned();
    store.put_config(&cfg2).unwrap();
    let listed = store.list_configs().unwrap();
    assert_eq!(listed.len(), 1, "同 config_id 应为更新而非新增");
    assert_eq!(listed[0], cfg2);

    assert!(store.delete_config("cfg-1").unwrap());
    assert!(!store.delete_config("cfg-1").unwrap());
    assert!(store.list_configs().unwrap().is_empty());
}

#[test]
fn tasks_list_ordering_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();
    store
        .put_task(&task("task-a", "accepted", "2026-07-26T08:00:00Z"))
        .unwrap();
    store
        .put_task(&task("task-b", "running", "2026-07-26T09:00:00Z"))
        .unwrap();
    store
        .put_task(&task("task-c", "failed", "2026-07-26T10:00:00Z"))
        .unwrap();

    let listed = store.list_tasks(2).unwrap();
    assert_eq!(
        listed.iter().map(|t| t.task_id.as_str()).collect::<Vec<_>>(),
        vec!["task-c", "task-b"]
    );
    assert!(store.get_task("task-missing").unwrap().is_none());
}
