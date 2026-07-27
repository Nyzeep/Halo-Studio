//! 工作区信任评估。
//!
//! 信任决定的持久化键 =（real_path, root_commit）；任一不匹配即视为目录被替换/重建，
//! 必须降级为 Untrusted 并置 identity_changed，要求用户重新确认（ipc-protocol.md 3.1）。

use serde::{Deserialize, Serialize};

/// 工作区身份：canonicalize 后的真实路径 + 仓库首个提交（空仓库为 None）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceIdentity {
    pub real_path: String,
    pub root_commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    Untrusted,
    Trusted,
}

/// 已持久化的信任决定（由 halo-store 保存，字段同构映射）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRecord {
    pub real_path: String,
    pub root_commit: Option<String>,
    pub trusted: bool,
    pub decided_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustEvaluation {
    pub state: TrustState,
    /// true 表示存在旧信任决定但工作区身份已变化：无论旧决定为何，一律降级为
    /// Untrusted，需要用户重新确认。
    pub identity_changed: bool,
}

pub fn evaluate_trust(saved: Option<&TrustRecord>, current: &WorkspaceIdentity) -> TrustEvaluation {
    match saved {
        None => TrustEvaluation {
            state: TrustState::Untrusted,
            identity_changed: false,
        },
        Some(rec) => {
            let identity_matches =
                rec.real_path == current.real_path && rec.root_commit == current.root_commit;
            if identity_matches {
                TrustEvaluation {
                    state: if rec.trusted {
                        TrustState::Trusted
                    } else {
                        TrustState::Untrusted
                    },
                    identity_changed: false,
                }
            } else {
                TrustEvaluation {
                    state: TrustState::Untrusted,
                    identity_changed: true,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(path: &str, root: Option<&str>) -> WorkspaceIdentity {
        WorkspaceIdentity {
            real_path: path.to_string(),
            root_commit: root.map(str::to_string),
        }
    }

    fn record(path: &str, root: Option<&str>, trusted: bool) -> TrustRecord {
        TrustRecord {
            real_path: path.to_string(),
            root_commit: root.map(str::to_string),
            trusted,
            decided_at: "2026-07-26T08:00:00Z".to_string(),
        }
    }

    #[test]
    fn no_saved_record_is_untrusted_without_identity_change() {
        let eval = evaluate_trust(None, &identity("D:\\repo", Some("abc")));
        assert_eq!(eval.state, TrustState::Untrusted);
        assert!(!eval.identity_changed);
    }

    #[test]
    fn matching_trusted_record_keeps_trust() {
        let rec = record("D:\\repo", Some("abc"), true);
        let eval = evaluate_trust(Some(&rec), &identity("D:\\repo", Some("abc")));
        assert_eq!(eval.state, TrustState::Trusted);
        assert!(!eval.identity_changed);
    }

    #[test]
    fn matching_untrusted_record_stays_untrusted_without_identity_change() {
        let rec = record("D:\\repo", Some("abc"), false);
        let eval = evaluate_trust(Some(&rec), &identity("D:\\repo", Some("abc")));
        assert_eq!(eval.state, TrustState::Untrusted);
        assert!(!eval.identity_changed);
    }

    #[test]
    fn root_commit_mismatch_downgrades_trust() {
        let rec = record("D:\\repo", Some("abc"), true);
        let eval = evaluate_trust(Some(&rec), &identity("D:\\repo", Some("def")));
        assert_eq!(eval.state, TrustState::Untrusted);
        assert!(eval.identity_changed);
    }

    #[test]
    fn real_path_mismatch_downgrades_trust() {
        let rec = record("D:\\repo", Some("abc"), true);
        let eval = evaluate_trust(Some(&rec), &identity("D:\\other repo 中文", Some("abc")));
        assert_eq!(eval.state, TrustState::Untrusted);
        assert!(eval.identity_changed);
    }

    #[test]
    fn root_commit_none_vs_some_counts_as_identity_change() {
        // 空仓库被替换为有历史的仓库（或反向）都属于目录身份变化
        let rec = record("D:\\repo", None, true);
        let eval = evaluate_trust(Some(&rec), &identity("D:\\repo", Some("abc")));
        assert_eq!(eval.state, TrustState::Untrusted);
        assert!(eval.identity_changed);

        let rec2 = record("D:\\repo", Some("abc"), true);
        let eval2 = evaluate_trust(Some(&rec2), &identity("D:\\repo", None));
        assert_eq!(eval2.state, TrustState::Untrusted);
        assert!(eval2.identity_changed);
    }

    #[test]
    fn matching_empty_repo_identity_keeps_trust() {
        let rec = record("D:\\repo", None, true);
        let eval = evaluate_trust(Some(&rec), &identity("D:\\repo", None));
        assert_eq!(eval.state, TrustState::Trusted);
        assert!(!eval.identity_changed);
    }
}
