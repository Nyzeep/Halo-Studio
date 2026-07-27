//! halo-protocol 契约测试：封包 round-trip、错误码字符串稳定性、
//! 拒绝路径与复杂结构的 JSON 形状快照断言。

use serde_json::{json, Value};

use halo_protocol::methods::config::{
    CredentialCheckResult, LaunchConfig, LaunchConfigInput, ThinkingLevel,
};
use halo_protocol::methods::handoff::{
    HandoffCreateParams, HandoffPackage, HandoffPreviewParams, HandoffPreviewResult,
    HandoffVerification, SelectedChange,
};
use halo_protocol::methods::history::{EvidenceFileSummary, EvidenceSummary, HistoryListParams};
use halo_protocol::methods::review::{
    Decision, DecisionKind, DeliveryRejectParams, FileChange, ReviewBundle, ReviewGetParams,
    ReviewFile, ReviewOutcome, Verification,
};
use halo_protocol::methods::runtime::{RuntimeState, RuntimeStateInfo, RuntimeStatusResult};
use halo_protocol::methods::task::{
    CancelMode, CreateTaskResult, TaskBaseline, TaskSpec, TaskState, TaskStatus,
    TaskStatusParams,
};
use halo_protocol::methods::workspace::{
    TrustDecision, TrustState, TrustWorkspaceParams, WorkspaceStatus, WorkspaceStatusResult,
};
use halo_protocol::methods::{
    AgentKind, Attribution, HelloParams, HelloResult, VerificationSource, VerificationStatus,
};
use halo_protocol::{
    read_message, write_message, ErrorBody, ErrorCode, Event, Inbound, ProtocolError,
    RequestEnvelope, Response, MAX_LINE_BYTES, PROTOCOL_VERSION,
};

// ---------- 封包 round-trip 与形状 ----------

#[test]
fn request_envelope_roundtrip_and_shape() {
    let req = RequestEnvelope {
        v: PROTOCOL_VERSION,
        id: "r-11111111-2222-4333-8444-555555555555".to_string(),
        method: "task.create".to_string(),
        params: json!({"agent": "pi"}),
    };
    let value = serde_json::to_value(&req).unwrap();
    assert_eq!(
        value,
        json!({
            "v": 1,
            "kind": "request",
            "id": "r-11111111-2222-4333-8444-555555555555",
            "method": "task.create",
            "params": {"agent": "pi"}
        })
    );
    let back: RequestEnvelope = serde_json::from_value(value).unwrap();
    assert_eq!(back, req);
}

#[test]
fn response_ok_roundtrip_and_shape() {
    let resp = Response::success("r-1", json!({"closed": true}));
    let value = serde_json::to_value(&resp).unwrap();
    assert_eq!(
        value,
        json!({
            "v": 1,
            "kind": "response",
            "id": "r-1",
            "ok": true,
            "result": {"closed": true}
        })
    );
    let back: Response = serde_json::from_value(value).unwrap();
    assert_eq!(back, resp);
}

#[test]
fn response_error_roundtrip_and_shape() {
    let resp = Response::failure(
        "r-2",
        ErrorBody::with_details(
            ErrorCode::TaskAlreadyRunning,
            "已有任务在运行",
            json!({"task_id": "task-1"}),
        ),
    );
    let value = serde_json::to_value(&resp).unwrap();
    assert_eq!(
        value,
        json!({
            "v": 1,
            "kind": "response",
            "id": "r-2",
            "ok": false,
            "error": {
                "code": "TASK_ALREADY_RUNNING",
                "message": "已有任务在运行",
                "details": {"task_id": "task-1"}
            }
        })
    );
    let back: Response = serde_json::from_value(value).unwrap();
    assert_eq!(back, resp);
}

#[test]
fn error_body_details_omitted_when_null() {
    let body = ErrorBody::new(ErrorCode::Internal, "内部错误");
    let value = serde_json::to_value(&body).unwrap();
    assert_eq!(value, json!({"code": "INTERNAL", "message": "内部错误"}));
    let back: ErrorBody = serde_json::from_value(value).unwrap();
    assert_eq!(back, body);
}

#[test]
fn event_roundtrip_and_shape() {
    let ev = Event {
        v: PROTOCOL_VERSION,
        seq: 42,
        ts: "2026-07-26T08:00:00Z".to_string(),
        task_id: Some("task-1".to_string()),
        event: "task.phase".to_string(),
        payload: json!({"phase": "planning", "detail": "…"}),
    };
    let value = serde_json::to_value(&ev).unwrap();
    assert_eq!(
        value,
        json!({
            "v": 1,
            "kind": "event",
            "seq": 42,
            "ts": "2026-07-26T08:00:00Z",
            "task_id": "task-1",
            "event": "task.phase",
            "payload": {"phase": "planning", "detail": "…"}
        })
    );
    let back: Event = serde_json::from_value(value).unwrap();
    assert_eq!(back, ev);
}

#[test]
fn event_task_id_serializes_as_explicit_null() {
    let ev = Event {
        v: PROTOCOL_VERSION,
        seq: 1,
        ts: "2026-07-26T08:00:00Z".to_string(),
        task_id: None,
        event: "sidecar.state".to_string(),
        payload: json!({"state": "ready", "protocol_version": 1}),
    };
    let value = serde_json::to_value(&ev).unwrap();
    assert_eq!(value.get("task_id"), Some(&Value::Null));
}

#[test]
fn envelope_kind_mismatch_rejected_by_serde() {
    let wrong = json!({
        "v": 1, "kind": "response", "id": "r-1", "method": "task.status", "params": {}
    });
    assert!(serde_json::from_value::<RequestEnvelope>(wrong).is_err());

    let wrong = json!({"v": 1, "kind": "request", "id": "r-1", "ok": true});
    assert!(serde_json::from_value::<Response>(wrong).is_err());

    let wrong = json!({
        "v": 1, "kind": "request", "seq": 1, "ts": "2026-07-26T08:00:00Z",
        "task_id": null, "event": "sidecar.state", "payload": {}
    });
    assert!(serde_json::from_value::<Event>(wrong).is_err());
}

// ---------- 封包 IO ----------

#[test]
fn write_then_read_roundtrip() {
    let req = RequestEnvelope {
        v: PROTOCOL_VERSION,
        id: "r-1".to_string(),
        method: "workspace.open".to_string(),
        params: json!({"path": "D:\\repo with space\\子目录"}),
    };
    let mut buf: Vec<u8> = Vec::new();
    write_message(&mut buf, &req).unwrap();

    // 单行输出：只有末尾一个换行符
    assert_eq!(buf.last(), Some(&b'\n'));
    assert_eq!(buf.iter().filter(|b| **b == b'\n').count(), 1);

    let line = std::str::from_utf8(&buf).unwrap();
    let Inbound::Request(back) = read_message(line).unwrap();
    assert_eq!(back, req);
}

#[test]
fn write_message_rejects_oversized_line() {
    let req = RequestEnvelope {
        v: PROTOCOL_VERSION,
        id: "r-1".to_string(),
        method: "task.create".to_string(),
        params: json!({"base_diff": "x".repeat(MAX_LINE_BYTES)}),
    };
    let mut buf: Vec<u8> = Vec::new();
    let err = write_message(&mut buf, &req).unwrap_err();
    assert!(matches!(err, ProtocolError::LineTooLong { actual } if actual > MAX_LINE_BYTES));
    assert!(buf.is_empty(), "超限时不得写出任何字节");
}

#[test]
fn read_message_rejects_oversized_line() {
    let line = format!(
        r#"{{"v":1,"kind":"request","id":"r-1","method":"task.create","params":{{"notes":"{}"}}}}"#,
        "y".repeat(MAX_LINE_BYTES)
    );
    let err = read_message(&line).unwrap_err();
    assert!(matches!(err, ProtocolError::LineTooLong { .. }));
}

#[test]
fn read_message_rejects_bad_json() {
    let err = read_message("{这不是 JSON").unwrap_err();
    assert!(matches!(err, ProtocolError::Parse { .. }));

    let err = read_message("").unwrap_err();
    assert!(matches!(err, ProtocolError::Parse { .. }));
}

#[test]
fn read_message_rejects_wrong_version() {
    let err = read_message(r#"{"v":2,"kind":"request","id":"r-1","method":"task.status","params":{}}"#)
        .unwrap_err();
    assert!(matches!(err, ProtocolError::UnsupportedVersion { found: 2 }));

    let err = read_message(r#"{"v":0,"kind":"request","id":"r-1","method":"task.status","params":{}}"#)
        .unwrap_err();
    assert!(matches!(err, ProtocolError::UnsupportedVersion { found: 0 }));
}

#[test]
fn read_message_rejects_missing_or_non_integer_version() {
    let err = read_message(r#"{"kind":"request","id":"r-1","method":"task.status","params":{}}"#)
        .unwrap_err();
    assert!(matches!(err, ProtocolError::Parse { .. }));

    let err = read_message(r#"{"v":"1","kind":"request","id":"r-1","method":"task.status","params":{}}"#)
        .unwrap_err();
    assert!(matches!(err, ProtocolError::Parse { .. }));
}

#[test]
fn read_message_rejects_non_request_kind() {
    let err = read_message(r#"{"v":1,"kind":"response","id":"r-1","ok":true,"result":{}}"#)
        .unwrap_err();
    assert!(matches!(err, ProtocolError::UnexpectedKind { ref found } if found == "response"));

    let err = read_message(
        r#"{"v":1,"kind":"event","seq":1,"ts":"2026-07-26T08:00:00Z","task_id":null,"event":"sidecar.state","payload":{}}"#,
    )
    .unwrap_err();
    assert!(matches!(err, ProtocolError::UnexpectedKind { ref found } if found == "event"));

    let err = read_message(r#"{"v":1,"id":"r-1","method":"task.status","params":{}}"#).unwrap_err();
    assert!(matches!(err, ProtocolError::Parse { .. }));
}

#[test]
fn read_message_tolerates_trailing_newline() {
    let line = "{\"v\":1,\"kind\":\"request\",\"id\":\"r-1\",\"method\":\"workspace.status\",\"params\":{}}\r\n";
    let Inbound::Request(req) = read_message(line).unwrap();
    assert_eq!(req.method, "workspace.status");
}

// ---------- 错误码字符串稳定性 ----------

#[test]
fn error_code_strings_are_stable() {
    // 与 protocol/v1/envelope.schema.json 的枚举一字不差（全部 31 个）。
    let pairs: &[(ErrorCode, &str)] = &[
        (ErrorCode::HelloRequired, "HELLO_REQUIRED"),
        (ErrorCode::ProtocolVersionUnsupported, "PROTOCOL_VERSION_UNSUPPORTED"),
        (ErrorCode::MethodNotFound, "METHOD_NOT_FOUND"),
        (ErrorCode::InvalidParams, "INVALID_PARAMS"),
        (ErrorCode::Internal, "INTERNAL"),
        (ErrorCode::WorkspacePathInvalid, "WORKSPACE_PATH_INVALID"),
        (ErrorCode::WorkspaceNotReadable, "WORKSPACE_NOT_READABLE"),
        (ErrorCode::WorkspaceNotGit, "WORKSPACE_NOT_GIT"),
        (ErrorCode::WorkspaceNotTrusted, "WORKSPACE_NOT_TRUSTED"),
        (ErrorCode::WorkspaceNotActive, "WORKSPACE_NOT_ACTIVE"),
        (ErrorCode::WorkspaceIdentityChanged, "WORKSPACE_IDENTITY_CHANGED"),
        (ErrorCode::CredentialStoreUnavailable, "CREDENTIAL_STORE_UNAVAILABLE"),
        (ErrorCode::CredentialNotFound, "CREDENTIAL_NOT_FOUND"),
        (ErrorCode::EnvNotWhitelisted, "ENV_NOT_WHITELISTED"),
        (ErrorCode::ConfigNotFound, "CONFIG_NOT_FOUND"),
        (ErrorCode::ConfigConflict, "CONFIG_CONFLICT"),
        (ErrorCode::RuntimeNotReady, "RUNTIME_NOT_READY"),
        (ErrorCode::RuntimeProbeFailed, "RUNTIME_PROBE_FAILED"),
        (ErrorCode::RuntimeVersionMismatch, "RUNTIME_VERSION_MISMATCH"),
        (ErrorCode::RuntimeAlreadyRunning, "RUNTIME_ALREADY_RUNNING"),
        (ErrorCode::TaskAlreadyRunning, "TASK_ALREADY_RUNNING"),
        (ErrorCode::TaskRunning, "TASK_RUNNING"),
        (ErrorCode::TaskNotFound, "TASK_NOT_FOUND"),
        (ErrorCode::TaskStillRunning, "TASK_STILL_RUNNING"),
        (ErrorCode::TaskNotReviewable, "TASK_NOT_REVIEWABLE"),
        (ErrorCode::EvidenceNotFound, "EVIDENCE_NOT_FOUND"),
        (ErrorCode::EvidenceNotLatest, "EVIDENCE_NOT_LATEST"),
        (ErrorCode::EventGap, "EVENT_GAP"),
        (ErrorCode::HandoffNotFound, "HANDOFF_NOT_FOUND"),
        (ErrorCode::LineTooLong, "LINE_TOO_LONG"),
        (ErrorCode::ParseError, "PARSE_ERROR"),
    ];
    assert_eq!(pairs.len(), 31);
    for (code, expected) in pairs {
        assert_eq!(serde_json::to_value(code).unwrap(), json!(expected));
        let back: ErrorCode = serde_json::from_value(json!(expected)).unwrap();
        assert_eq!(back, *code);
    }
}

// ---------- 共享枚举取值稳定性 ----------

#[test]
fn method_enum_values_are_lowercase_snake() {
    let cases: &[(Value, &str)] = &[
        (serde_json::to_value(AgentKind::Pi).unwrap(), "pi"),
        (serde_json::to_value(AgentKind::Opencode).unwrap(), "opencode"),
        (serde_json::to_value(Attribution::AgentOnly).unwrap(), "agent_only"),
        (serde_json::to_value(Attribution::Mixed).unwrap(), "mixed"),
        (serde_json::to_value(VerificationStatus::NotRun).unwrap(), "not_run"),
        (serde_json::to_value(VerificationSource::UserMarked).unwrap(), "user_marked"),
        (serde_json::to_value(TrustDecision::Revoke).unwrap(), "revoke"),
        (serde_json::to_value(TrustState::Untrusted).unwrap(), "untrusted"),
        (serde_json::to_value(ThinkingLevel::Medium).unwrap(), "medium"),
        (serde_json::to_value(RuntimeState::NotProbed).unwrap(), "not_probed"),
        (serde_json::to_value(TaskState::AwaitingAction).unwrap(), "awaiting_action"),
        (serde_json::to_value(TaskState::ReviewReady).unwrap(), "review_ready"),
        (serde_json::to_value(CancelMode::Forced).unwrap(), "forced"),
        (serde_json::to_value(ReviewOutcome::Interrupted).unwrap(), "interrupted"),
        (serde_json::to_value(FileChange::Renamed).unwrap(), "renamed"),
        (serde_json::to_value(DecisionKind::Accepted).unwrap(), "accepted"),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual, &json!(expected));
    }
}

// ---------- sidecar.hello ----------

#[test]
fn hello_roundtrip_and_shape() {
    let params = HelloParams {
        app_protocol_versions: vec![1],
        app_version: "0.1.0".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&params).unwrap(),
        json!({"app_protocol_versions": [1], "app_version": "0.1.0"})
    );

    let result = HelloResult {
        protocol_version: 1,
        sidecar_version: "0.1.0".to_string(),
        capabilities: vec![
            "workspace", "config", "pi", "opencode", "task", "review", "handoff", "history",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value,
        json!({
            "protocol_version": 1,
            "sidecar_version": "0.1.0",
            "capabilities": ["workspace","config","pi","opencode","task","review","handoff","history"]
        })
    );
    let back: HelloResult = serde_json::from_value(value).unwrap();
    assert_eq!(back, result);
}

// ---------- workspace ----------

#[test]
fn workspace_status_result_untagged_both_ways() {
    let active: WorkspaceStatusResult = serde_json::from_value(json!({
        "active": true,
        "workspace_id": "ws-1",
        "real_path": "D:\\repo",
        "git_root": "D:\\repo",
        "root_commit": "abc123",
        "trust": "trusted",
        "identity_changed": false
    }))
    .unwrap();
    assert!(matches!(active, WorkspaceStatusResult::Active(ref ws) if ws.trust == TrustState::Trusted));

    let inactive: WorkspaceStatusResult = serde_json::from_value(json!({"active": false})).unwrap();
    assert!(matches!(inactive, WorkspaceStatusResult::Inactive(ref w) if !w.active));
}

#[test]
fn workspace_types_roundtrip() {
    let params = TrustWorkspaceParams {
        workspace_id: "ws-1".to_string(),
        decision: TrustDecision::Trust,
    };
    let value = serde_json::to_value(&params).unwrap();
    assert_eq!(value, json!({"workspace_id": "ws-1", "decision": "trust"}));

    let status = WorkspaceStatus {
        active: true,
        workspace_id: "ws-1".to_string(),
        real_path: "D:\\空格 目录\\repo".to_string(),
        git_root: "D:\\空格 目录\\repo".to_string(),
        root_commit: None,
        trust: TrustState::Untrusted,
        identity_changed: true,
    };
    let value = serde_json::to_value(&status).unwrap();
    assert_eq!(value.get("root_commit"), Some(&Value::Null));
    let back: WorkspaceStatus = serde_json::from_value(value).unwrap();
    assert_eq!(back, status);
}

// ---------- config ----------

#[test]
fn launch_config_shape_snapshot() {
    let input = LaunchConfigInput {
        name: "Pi + GPT".to_string(),
        agent: AgentKind::Pi,
        executable_path: "C:\\tools\\pi\\pi.exe".to_string(),
        model: "gpt-5".to_string(),
        thinking_level: ThinkingLevel::High,
        credential_ref: Some("halo/pi/openai".to_string()),
        extra_args: vec![],
        env_overrides: Default::default(),
    };
    assert_eq!(
        serde_json::to_value(&input).unwrap(),
        json!({
            "name": "Pi + GPT",
            "agent": "pi",
            "executable_path": "C:\\tools\\pi\\pi.exe",
            "model": "gpt-5",
            "thinking_level": "high",
            "credential_ref": "halo/pi/openai",
            "extra_args": [],
            "env_overrides": {}
        })
    );

    let config = LaunchConfig {
        config_id: "cfg-1".to_string(),
        name: input.name.clone(),
        agent: input.agent,
        executable_path: input.executable_path.clone(),
        model: input.model.clone(),
        thinking_level: input.thinking_level,
        credential_ref: input.credential_ref.clone(),
        extra_args: input.extra_args.clone(),
        env_overrides: input.env_overrides.clone(),
        created_at: "2026-07-26T08:00:00Z".to_string(),
        updated_at: "2026-07-26T08:00:00Z".to_string(),
    };
    let value = serde_json::to_value(&config).unwrap();
    assert_eq!(value.get("config_id"), Some(&json!("cfg-1")));
    let back: LaunchConfig = serde_json::from_value(value).unwrap();
    assert_eq!(back, config);

    let check = CredentialCheckResult {
        exists: true,
        store_available: true,
    };
    assert_eq!(
        serde_json::to_value(&check).unwrap(),
        json!({"exists": true, "store_available": true})
    );
}

// ---------- runtime ----------

#[test]
fn runtime_status_shape_snapshot() {
    let result = RuntimeStatusResult {
        pi: RuntimeStateInfo {
            state: RuntimeState::Ready,
            reason: None,
            recovery_hint: None,
            version: Some("1.4.0".to_string()),
        },
        opencode: RuntimeStateInfo {
            state: RuntimeState::Failed,
            reason: Some("版本不匹配".to_string()),
            recovery_hint: Some("请安装锁定版本 0.4.2".to_string()),
            version: Some("0.5.0".to_string()),
        },
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value,
        json!({
            "pi": {"state": "ready", "reason": null, "recovery_hint": null, "version": "1.4.0"},
            "opencode": {
                "state": "failed",
                "reason": "版本不匹配",
                "recovery_hint": "请安装锁定版本 0.4.2",
                "version": "0.5.0"
            }
        })
    );
    let back: RuntimeStatusResult = serde_json::from_value(value).unwrap();
    assert_eq!(back, result);
}

// ---------- task.create 形状快照 ----------

#[test]
fn task_create_params_shape_snapshot() {
    let spec = TaskSpec {
        agent: AgentKind::Pi,
        config_id: "cfg-1".to_string(),
        title: "修复登录超时".to_string(),
        instructions: "排查 401 并修复".to_string(),
        files: vec!["src/auth.rs".to_string()],
        base_diff: None,
        notes: Some("优先最小改动".to_string()),
        handoff_id: None,
    };
    let value = serde_json::to_value(&spec).unwrap();
    assert_eq!(
        value,
        json!({
            "agent": "pi",
            "config_id": "cfg-1",
            "title": "修复登录超时",
            "instructions": "排查 401 并修复",
            "files": ["src/auth.rs"],
            "base_diff": null,
            "notes": "优先最小改动",
            "handoff_id": null
        })
    );
    let back: TaskSpec = serde_json::from_value(value).unwrap();
    assert_eq!(back, spec);

    // 可选字段允许整体省略
    let minimal: TaskSpec = serde_json::from_value(json!({
        "agent": "opencode",
        "config_id": "cfg-2",
        "title": "t",
        "instructions": "i"
    }))
    .unwrap();
    assert!(minimal.files.is_empty());
    assert_eq!(minimal.base_diff, None);
}

#[test]
fn task_create_result_shape_snapshot() {
    let result = CreateTaskResult {
        task: TaskStatus {
            task_id: "task-1".to_string(),
            agent: AgentKind::Pi,
            title: "修复登录超时".to_string(),
            state: TaskState::Created,
            attribution: Attribution::AgentOnly,
            baseline: TaskBaseline {
                head: Some("abc123".to_string()),
                captured_at: "2026-07-26T08:00:00Z".to_string(),
            },
            created_at: "2026-07-26T08:00:00Z".to_string(),
            ended_at: None,
            cancel_mode: None,
            latest_evidence_version: 0,
        },
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value,
        json!({
            "task": {
                "task_id": "task-1",
                "agent": "pi",
                "title": "修复登录超时",
                "state": "created",
                "attribution": "agent_only",
                "baseline": {"head": "abc123", "captured_at": "2026-07-26T08:00:00Z"},
                "created_at": "2026-07-26T08:00:00Z",
                "ended_at": null,
                "cancel_mode": null,
                "latest_evidence_version": 0
            }
        })
    );
    let back: CreateTaskResult = serde_json::from_value(value).unwrap();
    assert_eq!(back, result);
}

#[test]
fn task_status_params_allows_empty_object() {
    let params = TaskStatusParams { task_id: None };
    assert_eq!(serde_json::to_value(&params).unwrap(), json!({}));

    let back: TaskStatusParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(back.task_id, None);

    let back: TaskStatusParams = serde_json::from_value(json!({"task_id": "task-1"})).unwrap();
    assert_eq!(back.task_id.as_deref(), Some("task-1"));
}

// ---------- review.get 形状快照 ----------

#[test]
fn review_get_params_version_optional() {
    let latest = ReviewGetParams {
        task_id: "task-1".to_string(),
        version: None,
    };
    assert_eq!(
        serde_json::to_value(&latest).unwrap(),
        json!({"task_id": "task-1"})
    );

    let pinned = ReviewGetParams {
        task_id: "task-1".to_string(),
        version: Some(2),
    };
    assert_eq!(
        serde_json::to_value(&pinned).unwrap(),
        json!({"task_id": "task-1", "version": 2})
    );
}

#[test]
fn review_bundle_shape_snapshot() {
    let bundle = ReviewBundle {
        task_id: "task-1".to_string(),
        evidence_version: 2,
        is_latest: true,
        outcome: ReviewOutcome::Finished,
        attribution: Attribution::Mixed,
        attribution_reasons: vec!["用户于 08:12 标记人工编辑".to_string()],
        summary: "修复了登录超时".to_string(),
        files: vec![ReviewFile {
            path: "src/auth.rs".to_string(),
            change: FileChange::Modified,
            diff: "@@ -1 +1 @@".to_string(),
            truncated: false,
        }],
        verification: Verification {
            status: VerificationStatus::Passed,
            detail: "cargo test 全绿".to_string(),
            source: VerificationSource::Agent,
        },
        baseline_dirty_files: vec!["docs/x.md".to_string()],
    };
    let value = serde_json::to_value(&bundle).unwrap();
    assert_eq!(
        value,
        json!({
            "task_id": "task-1",
            "evidence_version": 2,
            "is_latest": true,
            "outcome": "finished",
            "attribution": "mixed",
            "attribution_reasons": ["用户于 08:12 标记人工编辑"],
            "summary": "修复了登录超时",
            "files": [{
                "path": "src/auth.rs",
                "change": "modified",
                "diff": "@@ -1 +1 @@",
                "truncated": false
            }],
            "verification": {"status": "passed", "detail": "cargo test 全绿", "source": "agent"},
            "baseline_dirty_files": ["docs/x.md"]
        })
    );
    let back: ReviewBundle = serde_json::from_value(value).unwrap();
    assert_eq!(back, bundle);
}

#[test]
fn decision_roundtrip_and_reject_reason_optional() {
    let decision = Decision {
        kind: DecisionKind::Rejected,
        task_id: "task-1".to_string(),
        evidence_version: 2,
        decided_at: "2026-07-26T09:00:00Z".to_string(),
        reason: Some("验证失败".to_string()),
    };
    let value = serde_json::to_value(&decision).unwrap();
    assert_eq!(
        value,
        json!({
            "kind": "rejected",
            "task_id": "task-1",
            "evidence_version": 2,
            "decided_at": "2026-07-26T09:00:00Z",
            "reason": "验证失败"
        })
    );
    let back: Decision = serde_json::from_value(value).unwrap();
    assert_eq!(back, decision);

    let params: DeliveryRejectParams = serde_json::from_value(json!({
        "task_id": "task-1",
        "evidence_version": 2
    }))
    .unwrap();
    assert_eq!(params.reason, None);
}

// ---------- handoff.preview 形状快照 ----------

#[test]
fn handoff_preview_params_selected_files_null_means_all() {
    let params = HandoffPreviewParams {
        task_id: "task-1".to_string(),
        selected_files: None,
    };
    assert_eq!(
        serde_json::to_value(&params).unwrap(),
        json!({"task_id": "task-1", "selected_files": null})
    );

    let params = HandoffPreviewParams {
        task_id: "task-1".to_string(),
        selected_files: Some(vec!["a.rs".to_string()]),
    };
    assert_eq!(
        serde_json::to_value(&params).unwrap(),
        json!({"task_id": "task-1", "selected_files": ["a.rs"]})
    );
}

#[test]
fn handoff_preview_result_shape_snapshot() {
    let result = HandoffPreviewResult {
        package: HandoffPackage {
            handoff_id: None,
            task_id: "task-1".to_string(),
            source_agent: AgentKind::Pi,
            target_agent: Some(AgentKind::Opencode),
            goal: "修复登录超时".to_string(),
            summary: "已修复 401 逻辑".to_string(),
            selected_changes: vec![SelectedChange {
                path: "src/auth.rs".to_string(),
                diff: "@@ -1 +1 @@".to_string(),
            }],
            verification: HandoffVerification {
                status: VerificationStatus::Failed,
                detail: "1 个用例失败".to_string(),
            },
            created_at: None,
        },
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value,
        json!({
            "package": {
                "handoff_id": null,
                "task_id": "task-1",
                "source_agent": "pi",
                "target_agent": "opencode",
                "goal": "修复登录超时",
                "summary": "已修复 401 逻辑",
                "selected_changes": [{"path": "src/auth.rs", "diff": "@@ -1 +1 @@"}],
                "verification": {"status": "failed", "detail": "1 个用例失败"},
                "created_at": null
            }
        })
    );
    let back: HandoffPreviewResult = serde_json::from_value(value).unwrap();
    assert_eq!(back, result);
}

#[test]
fn handoff_create_params_roundtrip() {
    let params = HandoffCreateParams {
        task_id: "task-1".to_string(),
        target_agent: AgentKind::Opencode,
        selected_files: vec!["a.rs".to_string()],
    };
    let value = serde_json::to_value(&params).unwrap();
    assert_eq!(
        value,
        json!({"task_id": "task-1", "target_agent": "opencode", "selected_files": ["a.rs"]})
    );
    let back: HandoffCreateParams = serde_json::from_value(value).unwrap();
    assert_eq!(back, params);
}

// ---------- history ----------

#[test]
fn history_types_roundtrip() {
    let params = HistoryListParams { limit: 50 };
    assert_eq!(serde_json::to_value(&params).unwrap(), json!({"limit": 50}));

    let summary = EvidenceSummary {
        task_id: "task-1".to_string(),
        evidence_version: 1,
        is_latest: true,
        outcome: ReviewOutcome::Cancelled,
        attribution: Attribution::AgentOnly,
        attribution_reasons: vec![],
        summary: "任务被取消".to_string(),
        files: vec![EvidenceFileSummary {
            path: "src/auth.rs".to_string(),
            change: FileChange::Added,
            truncated: true,
        }],
        verification: Verification {
            status: VerificationStatus::NotRun,
            detail: "用户标记未执行".to_string(),
            source: VerificationSource::UserMarked,
        },
        baseline_dirty_files: vec![],
    };
    let value = serde_json::to_value(&summary).unwrap();
    // 摘要形式不含逐文件 diff 正文
    assert_eq!(
        value.get("files"),
        Some(&json!([{"path": "src/auth.rs", "change": "added", "truncated": true}]))
    );
    let back: EvidenceSummary = serde_json::from_value(value).unwrap();
    assert_eq!(back, summary);
}
