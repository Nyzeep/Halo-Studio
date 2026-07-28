//! DTO ↔ 领域类型映射集中地：协议 DTO（halo-protocol）、领域类型（halo-core /
//! halo-config / halo-runtime）与存储记录（halo-store）之间的互转全部收拢在此，
//! 避免散落到各 handler。

use serde_json::{json, Value};

use halo_protocol::methods::config::{LaunchConfig as LaunchConfigDto, ThinkingLevel as ThinkingLevelDto};
use halo_protocol::methods::history::{EvidenceFileSummary, EvidenceSummary};
use halo_protocol::methods::review::{
    Decision as DecisionDto, DecisionKind, FileChange, ReviewBundle, ReviewFile, ReviewOutcome,
    Verification as VerificationDto,
};
use halo_protocol::methods::runtime::{
    RuntimeState as RuntimeStateDto, RuntimeStateInfo,
};
use halo_protocol::methods::task::{
    CancelMode, TaskBaseline, TaskState as TaskStateDto, TaskStatus,
};
use halo_protocol::methods::{
    AgentKind as AgentKindDto, Attribution as AttributionDto, VerificationSource as VerificationSourceDto,
    VerificationStatus as VerificationStatusDto,
};

/// 当前时间的契约时间戳（UTC，YYYY-MM-DDThh:mm:ssZ，秒级）。
pub fn now_ts() -> String {
    let n = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        n.year(),
        u8::from(n.month()),
        n.day(),
        n.hour(),
        n.minute(),
        n.second()
    )
}

// ---------- Agent ----------

pub fn agent_dto_to_domain(a: AgentKindDto) -> halo_config::AgentKind {
    match a {
        AgentKindDto::Pi => halo_config::AgentKind::Pi,
        AgentKindDto::Opencode => halo_config::AgentKind::OpenCode,
    }
}

pub fn agent_domain_to_dto(a: halo_config::AgentKind) -> AgentKindDto {
    match a {
        halo_config::AgentKind::Pi => AgentKindDto::Pi,
        halo_config::AgentKind::OpenCode => AgentKindDto::Opencode,
    }
}

pub fn agent_str_to_dto(s: &str) -> AgentKindDto {
    match s {
        "opencode" => AgentKindDto::Opencode,
        _ => AgentKindDto::Pi,
    }
}

// ---------- ThinkingLevel ----------

pub fn thinking_dto_to_str(t: ThinkingLevelDto) -> &'static str {
    match t {
        ThinkingLevelDto::Off => "off",
        ThinkingLevelDto::Low => "low",
        ThinkingLevelDto::Medium => "medium",
        ThinkingLevelDto::High => "high",
    }
}

pub fn thinking_str_to_dto(s: &str) -> ThinkingLevelDto {
    match s {
        "off" => ThinkingLevelDto::Off,
        "low" => ThinkingLevelDto::Low,
        "high" => ThinkingLevelDto::High,
        _ => ThinkingLevelDto::Medium,
    }
}

// ---------- LaunchConfig ----------

pub fn config_record_to_dto(rec: &halo_store::LaunchConfigRecord) -> LaunchConfigDto {
    LaunchConfigDto {
        config_id: rec.config_id.clone(),
        name: rec.name.clone(),
        agent: agent_str_to_dto(&rec.agent),
        executable_path: rec.executable_path.clone(),
        model: rec.model.clone(),
        thinking_level: thinking_str_to_dto(&rec.thinking_level),
        credential_ref: rec.credential_ref.clone(),
        created_at: rec.created_at.clone(),
        updated_at: rec.updated_at.clone(),
    }
}

// ---------- 任务状态 ----------

pub fn task_state_core_to_dto(s: halo_core::TaskState) -> TaskStateDto {
    match s {
        halo_core::TaskState::Created => TaskStateDto::Created,
        halo_core::TaskState::Running => TaskStateDto::Running,
        halo_core::TaskState::AwaitingAction => TaskStateDto::AwaitingAction,
        halo_core::TaskState::Finishing => TaskStateDto::Finishing,
        halo_core::TaskState::ReviewReady => TaskStateDto::ReviewReady,
        halo_core::TaskState::Accepted => TaskStateDto::Accepted,
        halo_core::TaskState::Rejected => TaskStateDto::Rejected,
        halo_core::TaskState::Cancelled => TaskStateDto::Cancelled,
        halo_core::TaskState::Failed => TaskStateDto::Failed,
        halo_core::TaskState::Interrupted => TaskStateDto::Interrupted,
    }
}

pub fn task_state_from_str(s: &str) -> halo_core::TaskState {
    match s {
        "created" => halo_core::TaskState::Created,
        "running" => halo_core::TaskState::Running,
        "awaiting_action" => halo_core::TaskState::AwaitingAction,
        "finishing" => halo_core::TaskState::Finishing,
        "review_ready" => halo_core::TaskState::ReviewReady,
        "accepted" => halo_core::TaskState::Accepted,
        "rejected" => halo_core::TaskState::Rejected,
        "cancelled" => halo_core::TaskState::Cancelled,
        "failed" => halo_core::TaskState::Failed,
        _ => halo_core::TaskState::Interrupted,
    }
}

// ---------- 归因 ----------

pub fn attribution_core_to_dto(a: &halo_core::Attribution) -> AttributionDto {
    match a {
        halo_core::Attribution::AgentOnly => AttributionDto::AgentOnly,
        halo_core::Attribution::Mixed { .. } => AttributionDto::Mixed,
    }
}

pub fn attribution_core_to_str(a: &halo_core::Attribution) -> &'static str {
    match a {
        halo_core::Attribution::AgentOnly => "agent_only",
        halo_core::Attribution::Mixed { .. } => "mixed",
    }
}

pub fn attribution_str_to_dto(s: &str) -> AttributionDto {
    if s == "mixed" {
        AttributionDto::Mixed
    } else {
        AttributionDto::AgentOnly
    }
}

pub fn attribution_reasons(a: &halo_core::Attribution) -> Vec<String> {
    match a {
        halo_core::Attribution::AgentOnly => Vec::new(),
        halo_core::Attribution::Mixed { reasons } => reasons.clone(),
    }
}

// ---------- 验证结论 ----------

pub fn verification_status_str_to_dto(s: &str) -> VerificationStatusDto {
    match s {
        "passed" => VerificationStatusDto::Passed,
        "failed" => VerificationStatusDto::Failed,
        _ => VerificationStatusDto::NotRun,
    }
}

pub fn verification_source_str_to_dto(s: &str) -> VerificationSourceDto {
    if s == "user_marked" {
        VerificationSourceDto::UserMarked
    } else {
        VerificationSourceDto::Agent
    }
}

pub fn verification_status_core_to_str(s: halo_core::VerificationStatus) -> &'static str {
    match s {
        halo_core::VerificationStatus::Passed => "passed",
        halo_core::VerificationStatus::Failed => "failed",
        halo_core::VerificationStatus::NotRun => "not_run",
    }
}

pub fn verification_source_core_to_str(s: halo_core::VerificationSource) -> &'static str {
    match s {
        halo_core::VerificationSource::Agent => "agent",
        halo_core::VerificationSource::UserMarked => "user_marked",
    }
}

// ---------- 结局与文件变更 ----------

pub fn outcome_str_to_dto(s: &str) -> ReviewOutcome {
    match s {
        "finished" => ReviewOutcome::Finished,
        "cancelled" => ReviewOutcome::Cancelled,
        "interrupted" => ReviewOutcome::Interrupted,
        _ => ReviewOutcome::Failed,
    }
}

pub fn change_str_to_dto(s: &str) -> FileChange {
    match s {
        "added" => FileChange::Added,
        "deleted" => FileChange::Deleted,
        "renamed" => FileChange::Renamed,
        _ => FileChange::Modified,
    }
}

// ---------- TaskStatus ----------

/// 从存储记录组装 TaskStatus DTO；latest_evidence_version 由调用方查证据表得出。
pub fn task_record_to_status(rec: &halo_store::TaskRecord, latest_evidence_version: u32) -> TaskStatus {
    TaskStatus {
        task_id: rec.task_id.clone(),
        agent: agent_str_to_dto(&rec.agent),
        title: rec.title.clone(),
        state: task_state_core_to_dto(task_state_from_str(&rec.state)),
        attribution: attribution_str_to_dto(&rec.attribution),
        baseline: TaskBaseline {
            head: rec.baseline_head.clone(),
            captured_at: rec.baseline_captured_at.clone(),
        },
        created_at: rec.created_at.clone(),
        ended_at: rec.ended_at.clone(),
        cancel_mode: rec.cancel_mode.as_deref().map(cancel_mode_from_str),
        latest_evidence_version,
    }
}

pub fn cancel_mode_from_str(s: &str) -> CancelMode {
    if s == "forced" {
        CancelMode::Forced
    } else {
        CancelMode::Native
    }
}

// ---------- 证据 ----------

pub fn evidence_record_to_bundle(
    rec: &halo_store::EvidenceRecord,
    manual_edit_paths: &[String],
    is_latest: bool,
) -> ReviewBundle {
    ReviewBundle {
        task_id: rec.task_id.clone(),
        evidence_version: rec.version,
        is_latest,
        outcome: outcome_str_to_dto(&rec.outcome),
        attribution: attribution_str_to_dto(&rec.attribution),
        attribution_reasons: rec.attribution_reasons.clone(),
        manual_edit_paths: manual_edit_paths.to_vec(),
        summary: rec.summary.clone(),
        files: rec
            .files
            .iter()
            .map(|f| ReviewFile {
                path: f.path.clone(),
                change: change_str_to_dto(&f.change),
                diff: f.diff.clone(),
                truncated: f.truncated,
                end_hash: f.end_hash.clone(),
            })
            .collect(),
        verification: VerificationDto {
            status: verification_status_str_to_dto(&rec.verification_status),
            detail: rec.verification_detail.clone(),
            source: verification_source_str_to_dto(&rec.verification_source),
        },
        baseline_dirty_files: rec.baseline_dirty_files.clone(),
    }
}

pub fn evidence_record_to_summary(rec: &halo_store::EvidenceRecord, is_latest: bool) -> EvidenceSummary {
    EvidenceSummary {
        task_id: rec.task_id.clone(),
        evidence_version: rec.version,
        is_latest,
        outcome: outcome_str_to_dto(&rec.outcome),
        attribution: attribution_str_to_dto(&rec.attribution),
        attribution_reasons: rec.attribution_reasons.clone(),
        summary: rec.summary.clone(),
        files: rec
            .files
            .iter()
            .map(|f| EvidenceFileSummary {
                path: f.path.clone(),
                change: change_str_to_dto(&f.change),
                truncated: f.truncated,
            })
            .collect(),
        verification: VerificationDto {
            status: verification_status_str_to_dto(&rec.verification_status),
            detail: rec.verification_detail.clone(),
            source: verification_source_str_to_dto(&rec.verification_source),
        },
        baseline_dirty_files: rec.baseline_dirty_files.clone(),
    }
}

// ---------- 决定 ----------

pub fn decision_record_to_dto(rec: &halo_store::DecisionRecord) -> DecisionDto {
    DecisionDto {
        kind: if rec.kind == "rejected" {
            DecisionKind::Rejected
        } else {
            DecisionKind::Accepted
        },
        task_id: rec.task_id.clone(),
        evidence_version: rec.evidence_version,
        decided_at: rec.decided_at.clone(),
        reason: rec.reason.clone(),
    }
}

// ---------- 运行时状态 ----------

pub fn runtime_state_to_info(state: &halo_runtime::RuntimeState, version: Option<String>) -> RuntimeStateInfo {
    let (dto, reason, hint) = match state {
        halo_runtime::RuntimeState::NotProbed => (RuntimeStateDto::NotProbed, None, None),
        halo_runtime::RuntimeState::Probing => (RuntimeStateDto::Probing, None, None),
        halo_runtime::RuntimeState::Starting => (RuntimeStateDto::Starting, None, None),
        halo_runtime::RuntimeState::Ready => (RuntimeStateDto::Ready, None, None),
        halo_runtime::RuntimeState::Failed { reason, recovery_hint } => (
            RuntimeStateDto::Failed,
            Some(reason.clone()),
            Some(recovery_hint.clone()),
        ),
        halo_runtime::RuntimeState::Stopping => (RuntimeStateDto::Stopping, None, None),
        halo_runtime::RuntimeState::Stopped => (RuntimeStateDto::Stopped, None, None),
    };
    RuntimeStateInfo {
        state: dto,
        reason,
        recovery_hint: hint,
        version,
    }
}

/// runtime.state 事件 payload：{"agent": …} ∪ RuntimeStateInfo。
pub fn runtime_state_payload(
    agent: halo_config::AgentKind,
    state: &halo_runtime::RuntimeState,
    version: Option<String>,
) -> Value {
    let info = runtime_state_to_info(state, version);
    let mut payload = match serde_json::to_value(&info) {
        Ok(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    payload.insert("agent".to_string(), json!(agent.as_str()));
    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_ts_matches_contract_shape() {
        let ts = now_ts();
        // YYYY-MM-DDThh:mm:ssZ，长度 20，无小数秒
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[10..11], "T");
        assert!(!ts.contains('.'));
    }

    #[test]
    fn runtime_state_payload_contains_agent_and_info() {
        let p = runtime_state_payload(
            halo_config::AgentKind::Pi,
            &halo_runtime::RuntimeState::Failed {
                reason: "启动失败".to_string(),
                recovery_hint: "请重试".to_string(),
            },
            Some("1.4.0".to_string()),
        );
        assert_eq!(p["agent"], "pi");
        assert_eq!(p["state"], "failed");
        assert_eq!(p["reason"], "启动失败");
        assert_eq!(p["version"], "1.4.0");
    }

    #[test]
    fn task_state_roundtrip_str() {
        for s in halo_core::TaskState::ALL {
            assert_eq!(task_state_from_str(s.as_str()), s);
        }
    }
}
