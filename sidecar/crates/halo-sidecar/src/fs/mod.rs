//! 受限工作区文件系统能力。
//!
//! UI 永远通过 `fs.*` IPC 使用本模块；路径牢笼与写保护必须留在 Sidecar，
//! 不能由 Python/QML 侧复刻。

pub mod cage;
pub mod ops;
pub mod search;

/// 文件系统能力的统一限制，避免各处理器出现不一致的上限。
pub mod limits {
    pub const FS_READ_MAX_BYTES: u64 = 8 * 1024 * 1024;
    pub const FS_WRITE_MAX_BYTES: u64 = 8 * 1024 * 1024;
    pub const FS_BINARY_SNIFF_BYTES: usize = 8 * 1024;
    pub const FS_LIST_MAX_ENTRIES: usize = 10_000;
    pub const FS_LIST_MAX_DEPTH: u32 = 8;
    pub const FS_SEARCH_DEFAULT_RESULTS: u32 = 500;
    pub const FS_SEARCH_MAX_RESULTS: u32 = 20_000;
    pub const FS_SEARCH_FILE_MAX_MATCHES: usize = 100;
    pub const FS_SEARCH_FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;
    pub const FS_SEARCH_TIME_BUDGET_MS: u64 = 5_000;
    pub const FS_PREVIEW_MAX_BYTES: usize = 512;
}

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("路径超出工作区范围：{0}")]
    OutsideWorkspace(String),
    #[error("路径不存在：{0}")]
    NotFound(String),
    #[error("目标已存在：{0}")]
    AlreadyExists(String),
    #[error("文件大小 {size} 字节超过上限")]
    TooLarge { size: u64 },
    #[error("二进制文件不支持读入编辑器")]
    Binary { size: u64 },
    #[error("文件内容已被外部修改")]
    Conflict { current_hash: String, mtime: String },
    #[error(".git 目录受只读保护：{0}")]
    GitProtected(String),
    #[error("非法文件名：{0}")]
    InvalidName(String),
    #[error("文件系统操作失败：{0}")]
    Io(String),
}
