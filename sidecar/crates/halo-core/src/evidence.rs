//! 交付证据：追加式版本日志。
//!
//! 交付证据版本是追加式的（CONTEXT.md）：重试、交接只能产生下一个版本，旧结果不可
//! 被覆盖。EvidenceLog 的内部 Vec 私有，公开 API 只有 append 与只读访问，类型上不
//! 存在修改旧版本的路径。

use crate::attribution::Attribution;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Finished,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEvidence {
    pub path: String,
    pub change: ChangeKind,
    pub diff: String,
    pub truncated: bool,
    /// 结束树中文件字节的 sha256；删除项、超大文件和旧证据没有该事实。
    #[serde(default)]
    pub end_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Passed,
    Failed,
    NotRun,
}

/// 验证结论来源：只能来自 Agent 原生运行时，或用户显式标记；Halo 不自行运行验证。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSource {
    Agent,
    UserMarked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    pub status: VerificationStatus,
    pub detail: String,
    pub source: VerificationSource,
}

impl Verification {
    pub fn from_agent(status: VerificationStatus, detail: impl Into<String>) -> Self {
        Verification {
            status,
            detail: detail.into(),
            source: VerificationSource::Agent,
        }
    }

    /// 用户显式标记只允许"未执行"这一种结论（ipc-protocol.md task.mark_verification）。
    pub fn user_marked_not_run(detail: impl Into<String>) -> Self {
        Verification {
            status: VerificationStatus::NotRun,
            detail: detail.into(),
            source: VerificationSource::UserMarked,
        }
    }
}

/// 待追加的证据草稿：与 EvidenceVersion 同构但没有版本号，版本号只能由日志分配。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceDraft {
    pub outcome: Outcome,
    pub attribution: Attribution,
    pub summary: String,
    pub files: Vec<FileEvidence>,
    pub verification: Verification,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceVersion {
    pub version: u32,
    pub outcome: Outcome,
    pub attribution: Attribution,
    pub summary: String,
    pub files: Vec<FileEvidence>,
    pub verification: Verification,
    pub created_at: String,
}

/// 追加式证据日志。版本号从 1 开始单调递增；不提供任何修改或删除旧版本的方法。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceLog(Vec<EvidenceVersion>);

impl EvidenceLog {
    pub fn new() -> Self {
        EvidenceLog(Vec::new())
    }

    pub fn append(&mut self, draft: EvidenceDraft) -> &EvidenceVersion {
        let version = self.0.len() as u32 + 1;
        self.0.push(EvidenceVersion {
            version,
            outcome: draft.outcome,
            attribution: draft.attribution,
            summary: draft.summary,
            files: draft.files,
            verification: draft.verification,
            created_at: draft.created_at,
        });
        // 刚 push 过，非空是构造保证
        &self.0[self.0.len() - 1]
    }

    pub fn latest(&self) -> Option<&EvidenceVersion> {
        self.0.last()
    }

    pub fn get(&self, version: u32) -> Option<&EvidenceVersion> {
        // 版本号即下标 + 1；仍按字段匹配以防调用方持有跨日志的版本号
        self.0.iter().find(|v| v.version == version)
    }

    pub fn versions(&self) -> &[EvidenceVersion] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(summary: &str, outcome: Outcome) -> EvidenceDraft {
        EvidenceDraft {
            outcome,
            attribution: Attribution::AgentOnly,
            summary: summary.to_string(),
            files: vec![FileEvidence {
                path: "src/auth.rs".to_string(),
                change: ChangeKind::Modified,
                diff: "--- a/src/auth.rs\n+++ b/src/auth.rs\n".to_string(),
                truncated: false,
                end_hash: Some("sha256:abc".to_string()),
            }],
            verification: Verification::from_agent(VerificationStatus::Passed, "cargo test 通过"),
            created_at: "2026-07-26T08:00:00Z".to_string(),
        }
    }

    #[test]
    fn append_assigns_incrementing_versions_from_one() {
        let mut log = EvidenceLog::new();
        assert!(log.is_empty());
        assert!(log.latest().is_none());

        let v1 = log.append(draft("第一次运行失败", Outcome::Failed)).version;
        let v2 = log.append(draft("重试成功", Outcome::Finished)).version;
        assert_eq!((v1, v2), (1, 2));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn append_never_overwrites_old_versions() {
        let mut log = EvidenceLog::new();
        log.append(draft("第一次运行失败", Outcome::Failed));
        let first_snapshot = log.get(1).cloned().unwrap();

        log.append(draft("重试成功", Outcome::Finished));
        log.append(draft("交接后复查", Outcome::Finished));

        // 追加后旧版本内容逐字段不变
        assert_eq!(log.get(1), Some(&first_snapshot));
        assert_eq!(log.get(1).unwrap().summary, "第一次运行失败");
        assert_eq!(log.get(1).unwrap().outcome, Outcome::Failed);
        // 只有最新版本可作为当前结论
        assert_eq!(log.latest().unwrap().version, 3);
        assert_eq!(log.latest().unwrap().summary, "交接后复查");
    }

    #[test]
    fn get_returns_none_for_missing_version() {
        let mut log = EvidenceLog::new();
        log.append(draft("唯一版本", Outcome::Finished));
        assert!(log.get(0).is_none());
        assert!(log.get(2).is_none());
        assert_eq!(log.get(1).unwrap().version, 1);
    }

    #[test]
    fn user_marked_verification_is_not_run_only() {
        let v = Verification::user_marked_not_run("用户标记：本次未运行测试");
        assert_eq!(v.status, VerificationStatus::NotRun);
        assert_eq!(v.source, VerificationSource::UserMarked);
    }

    #[test]
    fn enums_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&VerificationStatus::NotRun).unwrap(),
            "\"not_run\""
        );
        assert_eq!(
            serde_json::to_string(&VerificationSource::UserMarked).unwrap(),
            "\"user_marked\""
        );
        assert_eq!(serde_json::to_string(&Outcome::Finished).unwrap(), "\"finished\"");
        assert_eq!(serde_json::to_string(&ChangeKind::Renamed).unwrap(), "\"renamed\"");
    }

    #[test]
    fn file_evidence_end_hash_round_trips_and_old_payload_defaults_to_none() {
        let file = FileEvidence {
            path: "src/auth.rs".to_string(),
            change: ChangeKind::Modified,
            diff: "+line".to_string(),
            truncated: false,
            end_hash: Some("sha256:abc".to_string()),
        };
        let round_trip: FileEvidence = serde_json::from_str(&serde_json::to_string(&file).unwrap()).unwrap();
        assert_eq!(round_trip, file);

        let old: FileEvidence = serde_json::from_str(
            r#"{"path":"src/auth.rs","change":"modified","diff":"+line","truncated":false}"#,
        )
        .unwrap();
        assert_eq!(old.end_hash, None);
    }
}
