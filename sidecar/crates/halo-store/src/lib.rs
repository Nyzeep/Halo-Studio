//! SQLite 本地持久化：交付历史只保存脱敏、大小受限的摘要与 Diff 证据。
//! 契约见 docs/module-contracts.md 第 4 节；记录形状同构于 docs/ipc-protocol.md。
//!
//! 边界约束（代码本身说不出的部分）：
//! - 本 crate 不依赖任何其他 halo crate；记录结构体为 store 自有类型，由 halo-sidecar 负责与协议 DTO 互转。
//! - 脱敏（sanitize）由 sidecar 在入库前完成；本层只按 `StoreLimits` 强制大小上限并记录截断标记（防御纵深）。
//! - 凭据红线：所有表结构只允许出现凭据引用名（`credential_ref`），任何列都不得承载密钥明文。

mod error;
mod limits;
mod records;
mod store;

pub use error::StoreError;
pub use limits::StoreLimits;
pub use records::{
    DecisionRecord, EvidenceDraft, EvidenceRecord, FileChangeDraft, FileEvidenceRecord,
    HandoffRecord, LaunchConfigRecord, SelectedChangeRecord, TaskRecord, TrustRecord,
};
pub use store::Store;
