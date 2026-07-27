//! halo-core：领域状态机与交付证据的纯逻辑层。
//!
//! 纪律（docs/module-contracts.md 第 0/2 节）：
//! - 无 IO：不依赖进程、网络、rusqlite，也不依赖其他 halo crate；
//! - 领域类型为 core 自有类型，由 halo-sidecar 负责与协议 DTO 互转；
//! - 任何错误 message、Debug 输出、序列化结果都不得携带凭据明文。

pub mod attribution;
pub mod evidence;
pub mod handoff;
pub mod limits;
pub mod task;
pub mod text;
pub mod trust;

pub use attribution::{Attribution, Baseline, ChangePartition};
pub use evidence::{
    ChangeKind, EvidenceDraft, EvidenceLog, EvidenceVersion, FileEvidence, Outcome, Verification,
    VerificationSource, VerificationStatus,
};
pub use handoff::{build_handoff, HandoffDraft, SelectedChange};
pub use task::{TaskEvent, TaskState, TransitionError};
pub use text::{cap, sanitize};
pub use trust::{evaluate_trust, TrustEvaluation, TrustRecord, TrustState, WorkspaceIdentity};
