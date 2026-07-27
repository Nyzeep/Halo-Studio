//! 交接包草稿。
//!
//! 交接包是有限上下文（CONTEXT.md）：只包含任务目标、主 Agent 摘要、选定文件变更
//! 和验证结果。HandoffDraft 类型上没有对话、原始工具日志、凭据或配置文件字段，
//! build_handoff 只从证据白名单字段取值并再过一次脱敏，构造上排除泄漏路径。

use crate::evidence::{EvidenceVersion, Verification};
use crate::text::sanitize;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedChange {
    pub path: String,
    pub diff: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffDraft {
    pub goal: String,
    pub summary: String,
    pub selected_changes: Vec<SelectedChange>,
    pub verification: Verification,
}

/// 从一个证据版本构建交接包草稿。
/// selected 为 None 时默认携带全部关联文件；为 Some 时只携带白名单内路径，
/// 未知路径静默忽略（不成为夹带额外内容的通道）。
pub fn build_handoff(
    evidence: &EvidenceVersion,
    goal: &str,
    selected: Option<&[String]>,
) -> HandoffDraft {
    let selected_changes = evidence
        .files
        .iter()
        .filter(|f| match selected {
            None => true,
            Some(paths) => paths.iter().any(|p| p == &f.path),
        })
        .map(|f| SelectedChange {
            path: f.path.clone(),
            diff: sanitize(&f.diff),
        })
        .collect();

    HandoffDraft {
        goal: sanitize(goal),
        summary: sanitize(&evidence.summary),
        selected_changes,
        verification: evidence.verification.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribution::Attribution;
    use crate::evidence::{
        ChangeKind, EvidenceVersion, FileEvidence, VerificationSource, VerificationStatus,
    };

    fn evidence() -> EvidenceVersion {
        EvidenceVersion {
            version: 2,
            outcome: crate::evidence::Outcome::Finished,
            attribution: Attribution::AgentOnly,
            summary: "修复了登录超时；调试期间打印过 password=hunter2secret".to_string(),
            files: vec![
                FileEvidence {
                    path: "src/auth.rs".to_string(),
                    change: ChangeKind::Modified,
                    diff: "+let key = \"sk-abcdefgh12345678\";".to_string(),
                    truncated: false,
                },
                FileEvidence {
                    path: "src/lib.rs".to_string(),
                    change: ChangeKind::Modified,
                    diff: "+pub mod auth;".to_string(),
                    truncated: false,
                },
            ],
            verification: Verification {
                status: VerificationStatus::Passed,
                detail: "cargo test 通过".to_string(),
                source: VerificationSource::Agent,
            },
            created_at: "2026-07-26T08:30:00Z".to_string(),
        }
    }

    #[test]
    fn none_selection_carries_all_files() {
        let draft = build_handoff(&evidence(), "修复登录超时", None);
        assert_eq!(draft.selected_changes.len(), 2);
        assert_eq!(draft.goal, "修复登录超时");
        assert_eq!(draft.verification.status, VerificationStatus::Passed);
        assert_eq!(draft.verification.source, VerificationSource::Agent);
    }

    #[test]
    fn selection_filters_files_and_ignores_unknown_paths() {
        let selected = vec!["src/lib.rs".to_string(), "not/in/evidence.rs".to_string()];
        let draft = build_handoff(&evidence(), "修复登录超时", Some(&selected));
        assert_eq!(draft.selected_changes.len(), 1);
        assert_eq!(draft.selected_changes[0].path, "src/lib.rs");
    }

    #[test]
    fn empty_selection_yields_no_changes() {
        let draft = build_handoff(&evidence(), "修复登录超时", Some(&[]));
        assert!(draft.selected_changes.is_empty());
    }

    #[test]
    fn handoff_sanitizes_goal_summary_and_diffs() {
        let draft = build_handoff(
            &evidence(),
            "目标里混入了 Bearer tok12345678 也要脱敏",
            None,
        );
        assert!(!draft.goal.contains("tok12345678"));
        assert!(!draft.summary.contains("hunter2secret"));
        let auth_diff = &draft.selected_changes[0].diff;
        assert!(!auth_diff.contains("sk-abcdefgh12345678"), "{auth_diff}");
        assert!(auth_diff.contains("[REDACTED]"));
    }

    #[test]
    fn draft_serialization_contains_only_whitelist_fields() {
        // 类型上只有 goal/summary/selected_changes/verification 四个字段；
        // 序列化结果不可能出现对话、日志或凭据字段
        let draft = build_handoff(&evidence(), "修复登录超时", None);
        let json = serde_json::to_value(&draft).unwrap();
        let obj = json.as_object().unwrap();
        let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["goal", "selected_changes", "summary", "verification"]);
    }
}
