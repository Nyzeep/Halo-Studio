/// 存储层错误。message 为中文用户可读文案；任何变体都不得携带凭据明文。
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("数据库操作失败：{0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("记录编解码失败：{0}")]
    Json(#[from] serde_json::Error),

    #[error("数据库目录创建失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("交付证据为追加式：任务 {task_id} 的版本 {version} 已存在，禁止改写")]
    EvidenceVersionExists { task_id: String, version: u32 },

    #[error("数据库 schema 版本 {found} 高于本程序支持的 {supported}，请先升级 Halo Studio")]
    SchemaTooNew { found: usize, supported: usize },
}
