//! store 自有记录类型：字段与 docs/ipc-protocol.md 中对应消息同构，
//! 枚举类字段以契约锁定的小写蛇形字符串存储（"pi"、"review_ready"、"agent_only"…），
//! 由 halo-sidecar 负责与协议 DTO / halo-core 领域类型互转。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 工作区信任记录。键为 canonicalize 后的真实路径；
/// 信任判定键 =（real_path, root_commit），root_commit 变化的降级判断在 halo-core。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRecord {
    pub real_path: String,
    pub root_commit: Option<String>,
    pub trusted: bool,
    pub decided_at: String,
}

/// 受管启动配置记录（同构 IPC `LaunchConfig`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchConfigRecord {
    pub config_id: String,
    pub name: String,
    /// "pi" | "opencode"
    pub agent: String,
    pub executable_path: String,
    pub model: String,
    /// "off" | "low" | "medium" | "high"
    pub thinking_level: String,
    /// 只存 Windows 凭据管理器条目名（引用名），绝不存任何密钥明文
    pub credential_ref: Option<String>,
    pub extra_args: Vec<String>,
    /// 白名单校验由 halo-config 负责，本层只做透明存取
    pub env_overrides: BTreeMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 任务记录（同构 IPC `TaskStatus`；latest_evidence_version 由 evidence 表推导，不落列）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: String,
    pub agent: String,
    pub title: String,
    /// 脱敏、限长后的任务目标；交接在重启后只能从此字段恢复目标。
    pub goal: String,
    /// created|running|awaiting_action|finishing|review_ready|accepted|rejected|cancelled|failed|interrupted
    pub state: String,
    /// "agent_only" | "mixed"
    pub attribution: String,
    /// 任务活跃期经工作台发生人工写入的路径集合。旧记录默认空集合。
    #[serde(default)]
    pub manual_edit_paths: Vec<String>,
    pub baseline_head: Option<String>,
    pub baseline_captured_at: String,
    pub created_at: String,
    pub ended_at: Option<String>,
    /// "native" | "forced"，仅取消结束的任务有值
    pub cancel_mode: Option<String>,
}

/// 追加证据的输入：不含版本号与截断标记——版本由 Store 分配（max+1），
/// 截断与标记由 Store 按 `StoreLimits` 执行，调用方无法指定版本即无法构成改写路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceDraft {
    /// finished|cancelled|failed|interrupted
    pub outcome: String,
    pub attribution: String,
    pub attribution_reasons: Vec<String>,
    pub summary: String,
    pub files: Vec<FileChangeDraft>,
    /// passed|failed|not_run
    pub verification_status: String,
    pub verification_detail: String,
    /// "agent" | "user_marked"
    pub verification_source: String,
    pub baseline_dirty_files: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChangeDraft {
    pub path: String,
    /// modified|added|deleted|renamed
    pub change: String,
    pub diff: String,
    /// 结束树中该文件字节的 sha256；删除或超过读取上限时为空。
    #[serde(default)]
    pub end_hash: Option<String>,
}

/// 交付证据版本（同构 IPC `ReviewBundle` 的持久化部分；is_latest 由读取时推导）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub task_id: String,
    pub version: u32,
    pub outcome: String,
    pub attribution: String,
    pub attribution_reasons: Vec<String>,
    pub summary: String,
    pub summary_truncated: bool,
    pub files: Vec<FileEvidenceRecord>,
    pub verification_status: String,
    pub verification_detail: String,
    pub verification_source: String,
    pub baseline_dirty_files: Vec<String>,
    /// 本版本任一字段被截断即为 true（含 summary、file diff、verification_detail、reasons）
    pub truncated: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEvidenceRecord {
    pub path: String,
    pub change: String,
    pub diff: String,
    pub truncated: bool,
    /// 旧证据文件 JSON 没有该字段时回退为 None。
    #[serde(default)]
    pub end_hash: Option<String>,
}

/// 审查决定记录（同构 IPC `Decision`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecord {
    /// "accepted" | "rejected"
    pub kind: String,
    pub task_id: String,
    pub evidence_version: u32,
    pub decided_at: String,
    pub reason: Option<String>,
    pub reason_truncated: bool,
}

/// 交接包记录（同构 IPC `HandoffPackage`；构造上就不包含对话、日志与凭据）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub handoff_id: String,
    pub task_id: String,
    pub source_agent: String,
    pub target_agent: Option<String>,
    pub goal: String,
    pub goal_truncated: bool,
    pub summary: String,
    pub summary_truncated: bool,
    pub selected_changes: Vec<SelectedChangeRecord>,
    pub verification_status: String,
    pub verification_detail: String,
    /// 任一字段被截断即为 true
    pub truncated: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedChangeRecord {
    pub path: String,
    pub diff: String,
    pub truncated: bool,
}
