//! 归因与任务基线。
//!
//! 诚实归因红线（requirements-alignment/01 用户故事 9）：基线前已有修改永不归因给
//! Agent；任务期间发生人工编辑时必须迁移为 Mixed，不得声称全部由 Agent 编写。

use serde::{Deserialize, Serialize};

/// 经工作台完成的人工文件操作。这个枚举只描述已经成功落盘的操作，
/// 不承载任何 UI 或文件系统实现细节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualEditOp {
    Write,
    CreateFile,
    CreateDir,
    Rename,
}

/// 生成持久化的人工介入说明。
///
/// 时间由调用方按本地时区格式化；当本地时区不可用时，调用方传入带 ` UTC`
/// 后缀的值即可。这里保持纯函数，便于锁定文案而不把时钟或平台能力带入 core。
pub fn manual_edit_note(
    op: ManualEditOp,
    path: &str,
    to_path: Option<&str>,
    local_hhmm: &str,
) -> String {
    match op {
        ManualEditOp::Write => format!("用户于 {local_hhmm} 经工作台保存 {path}"),
        ManualEditOp::CreateFile => format!("用户于 {local_hhmm} 经工作台新建文件 {path}"),
        ManualEditOp::CreateDir => format!("用户于 {local_hhmm} 经工作台新建目录 {path}"),
        ManualEditOp::Rename => format!(
            "用户于 {local_hhmm} 经工作台重命名 {path} → {}",
            to_path.unwrap_or(path)
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Attribution {
    AgentOnly,
    Mixed { reasons: Vec<String> },
}

impl Attribution {
    pub fn agent_only() -> Self {
        Attribution::AgentOnly
    }

    /// 记录一次人工介入：AgentOnly → Mixed；已是 Mixed 则追加原因。
    /// 只增不减：不存在从 Mixed 回到 AgentOnly 的路径。
    pub fn with_manual_edit(self, reason: impl Into<String>) -> Self {
        match self {
            Attribution::AgentOnly => Attribution::Mixed {
                reasons: vec![reason.into()],
            },
            Attribution::Mixed { mut reasons } => {
                reasons.push(reason.into());
                Attribution::Mixed { reasons }
            }
        }
    }

    pub fn is_mixed(&self) -> bool {
        matches!(self, Attribution::Mixed { .. })
    }
}

/// 任务基线：创建任务时记录的 Git 状态（HEAD、临时索引 write-tree 的树对象、
/// 脏文件清单）。dirty_files 中的文件即使在任务期间再次变化，也不得归因给 Agent。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub head: Option<String>,
    pub tree: String,
    pub dirty_files: Vec<String>,
    pub captured_at: String,
}

/// 任务结束时变更文件的划分结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePartition {
    /// 基线时干净、任务期间发生变化的文件：允许归因给 Agent（在无人工编辑时）。
    pub agent_attributable: Vec<String>,
    /// 基线时已脏的文件：永不归因给 Agent，UI 单独展示。
    pub baseline_dirty: Vec<String>,
}

impl Baseline {
    /// 基线前是否已有修改；已脏文件永不归因 Agent。
    pub fn is_attributable_to_agent(&self, path: &str) -> bool {
        !self.dirty_files.iter().any(|f| f == path)
    }

    pub fn partition_changes(&self, changed_files: &[String]) -> ChangePartition {
        let mut agent_attributable = Vec::new();
        let mut baseline_dirty = Vec::new();
        for f in changed_files {
            if self.is_attributable_to_agent(f) {
                agent_attributable.push(f.clone());
            } else {
                baseline_dirty.push(f.clone());
            }
        }
        ChangePartition {
            agent_attributable,
            baseline_dirty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline(dirty: &[&str]) -> Baseline {
        Baseline {
            head: Some("abc123".to_string()),
            tree: "tree456".to_string(),
            dirty_files: dirty.iter().map(|s| s.to_string()).collect(),
            captured_at: "2026-07-26T08:00:00Z".to_string(),
        }
    }

    #[test]
    fn agent_only_becomes_mixed_on_manual_edit() {
        let a = Attribution::agent_only();
        assert!(!a.is_mixed());
        let a = a.with_manual_edit("用户于 08:12 标记人工编辑");
        assert_eq!(
            a,
            Attribution::Mixed {
                reasons: vec!["用户于 08:12 标记人工编辑".to_string()]
            }
        );
    }

    #[test]
    fn mixed_accumulates_reasons_and_never_reverts() {
        let a = Attribution::agent_only()
            .with_manual_edit("第一次人工编辑")
            .with_manual_edit("第二次人工编辑");
        match a {
            Attribution::Mixed { reasons } => {
                assert_eq!(reasons.len(), 2);
                assert_eq!(reasons[0], "第一次人工编辑");
                assert_eq!(reasons[1], "第二次人工编辑");
            }
            Attribution::AgentOnly => panic!("Mixed 不允许回到 AgentOnly"),
        }
    }

    #[test]
    fn baseline_dirty_file_is_never_agent_attributable() {
        let b = baseline(&["docs/x.md", "src/pre existing 中文.rs"]);
        // 即使该文件在任务期间又被修改，也不归因 Agent
        let changed = vec![
            "src/auth.rs".to_string(),
            "docs/x.md".to_string(),
            "src/pre existing 中文.rs".to_string(),
        ];
        let p = b.partition_changes(&changed);
        assert_eq!(p.agent_attributable, vec!["src/auth.rs".to_string()]);
        assert_eq!(
            p.baseline_dirty,
            vec![
                "docs/x.md".to_string(),
                "src/pre existing 中文.rs".to_string()
            ]
        );
        assert!(!b.is_attributable_to_agent("docs/x.md"));
        assert!(b.is_attributable_to_agent("src/auth.rs"));
    }

    #[test]
    fn clean_baseline_attributes_all_changes() {
        let b = baseline(&[]);
        let changed = vec!["a.rs".to_string(), "b.rs".to_string()];
        let p = b.partition_changes(&changed);
        assert_eq!(p.agent_attributable, changed);
        assert!(p.baseline_dirty.is_empty());
    }

    #[test]
    fn manual_edit_notes_lock_operation_wording_and_rename_paths() {
        assert_eq!(
            manual_edit_note(ManualEditOp::Write, "src/a.rs", None, "14:32"),
            "用户于 14:32 经工作台保存 src/a.rs"
        );
        assert_eq!(
            manual_edit_note(ManualEditOp::CreateFile, "src/a.rs", None, "14:32"),
            "用户于 14:32 经工作台新建文件 src/a.rs"
        );
        assert_eq!(
            manual_edit_note(ManualEditOp::CreateDir, "src", None, "14:32 UTC"),
            "用户于 14:32 UTC 经工作台新建目录 src"
        );
        assert_eq!(
            manual_edit_note(
                ManualEditOp::Rename,
                "src/old.rs",
                Some("src/new.rs"),
                "14:32"
            ),
            "用户于 14:32 经工作台重命名 src/old.rs → src/new.rs"
        );
    }
}
