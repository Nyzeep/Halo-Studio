use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 配置事务错误。
#[derive(Debug, thiserror::Error)]
pub enum TxError {
    #[error("配置事务冲突：文件在事务开始后被外部修改")]
    Conflict,
    #[error("配置文件 IO 错误（{context}）：{source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("配置事务校验失败：{context}")]
    VerifyFailed { context: String },
}

/// 提交回执：除备份路径外携带目标路径与前后内容哈希，
/// 使回滚可以脱离事务对象独立校验。
#[derive(Debug, Clone)]
pub struct TxReceipt {
    pub path: PathBuf,
    pub backup_path: PathBuf,
    pub original_sha256: String,
    pub new_sha256: String,
}

/// 对 Pi/OpenCode 原生配置文件的独立受管变更：
/// Diff 预览 → 冲突检测 → 备份 → 原子写入 → 可验证回滚。
/// 与 Agent 任务完全无关；目标文件必须已存在且为 UTF-8 文本。
#[derive(Debug)]
pub struct ConfigTransaction {
    path: PathBuf,
    original_content: String,
    original_sha256: String,
    new_content: String,
}

impl ConfigTransaction {
    /// 读取原文件并记录内容与 sha256；文件不存在或非 UTF-8 文本时失败。
    pub fn begin(path: &Path, new_content: String) -> Result<Self, TxError> {
        let bytes = fs::read(path).map_err(|e| TxError::Io {
            context: format!("读取原配置文件 {}", path.display()),
            source: e,
        })?;
        let original_sha256 = sha256_hex(&bytes);
        let original_content = String::from_utf8(bytes).map_err(|e| TxError::Io {
            context: format!("原配置文件 {} 不是 UTF-8 文本", path.display()),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        Ok(ConfigTransaction {
            path: path.to_path_buf(),
            original_content,
            original_sha256,
            new_content,
        })
    }

    /// 生成供用户确认的统一格式文本 diff。
    pub fn preview(&self) -> String {
        similar::TextDiff::from_lines(&self.original_content, &self.new_content)
            .unified_diff()
            .context_radius(3)
            .header("当前", "拟写入")
            .to_string()
    }

    /// 冲突检测（重读文件，hash 变化 => Conflict）→ 备份原文件（写后校验）
    /// → 临时文件写入 + rename 原子替换 → 提交后校验。
    /// 冲突检测失败时未产生任何写入。
    pub fn commit(self) -> Result<TxReceipt, TxError> {
        let current = fs::read(&self.path).map_err(|e| TxError::Io {
            context: format!("提交前重读配置文件 {}", self.path.display()),
            source: e,
        })?;
        if sha256_hex(&current) != self.original_sha256 {
            return Err(TxError::Conflict);
        }

        let backup_path = sibling_path(&self.path, "halo-bak")?;
        fs::write(&backup_path, &current).map_err(|e| TxError::Io {
            context: format!("写入备份文件 {}", backup_path.display()),
            source: e,
        })?;
        let backup_bytes = fs::read(&backup_path).map_err(|e| TxError::Io {
            context: format!("回读备份文件 {}", backup_path.display()),
            source: e,
        })?;
        if sha256_hex(&backup_bytes) != self.original_sha256 {
            return Err(TxError::VerifyFailed {
                context: "备份文件内容与原文件不一致".to_string(),
            });
        }

        atomic_write(&self.path, self.new_content.as_bytes())?;

        let new_sha256 = sha256_hex(self.new_content.as_bytes());
        let written = fs::read(&self.path).map_err(|e| TxError::Io {
            context: format!("提交后回读配置文件 {}", self.path.display()),
            source: e,
        })?;
        if sha256_hex(&written) != new_sha256 {
            return Err(TxError::VerifyFailed {
                context: "提交后文件内容与拟写入内容不一致".to_string(),
            });
        }

        Ok(TxReceipt {
            path: self.path,
            backup_path,
            original_sha256: self.original_sha256,
            new_sha256,
        })
    }

    /// 契约形状的关联函数写法；等价于模块级 [`rollback`]。
    pub fn rollback(receipt: &TxReceipt) -> Result<(), TxError> {
        rollback(receipt)
    }
}

/// 从备份可验证恢复：先校验备份完整性，再原子写回，最后校验恢复结果。
/// 备份被篡改时目标文件保持原样不动。
pub fn rollback(receipt: &TxReceipt) -> Result<(), TxError> {
    let backup_bytes = fs::read(&receipt.backup_path).map_err(|e| TxError::Io {
        context: format!("读取备份文件 {}", receipt.backup_path.display()),
        source: e,
    })?;
    if sha256_hex(&backup_bytes) != receipt.original_sha256 {
        return Err(TxError::VerifyFailed {
            context: "备份文件内容校验失败，拒绝回滚".to_string(),
        });
    }

    atomic_write(&receipt.path, &backup_bytes)?;

    let restored = fs::read(&receipt.path).map_err(|e| TxError::Io {
        context: format!("回滚后回读配置文件 {}", receipt.path.display()),
        source: e,
    })?;
    if sha256_hex(&restored) != receipt.original_sha256 {
        return Err(TxError::VerifyFailed {
            context: "回滚后文件内容校验失败".to_string(),
        });
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// 与目标同目录的唯一兄弟路径（备份/临时文件必须同卷才能 rename 原子替换）。
fn sibling_path(path: &Path, tag: &str) -> Result<PathBuf, TxError> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| TxError::Io {
            context: format!("配置文件路径 {} 缺少有效文件名", path.display()),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "无效路径"),
        })?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let unique = format!("{file_name}.{tag}-{}-{nanos}", std::process::id());
    Ok(path.with_file_name(unique))
}

/// 临时文件写入 + sync + rename 原子替换（Windows 上 rename 使用
/// MOVEFILE_REPLACE_EXISTING，可覆盖既有目标）。
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), TxError> {
    let tmp_path = sibling_path(path, "halo-tmp")?;
    let write_result = (|| -> Result<(), TxError> {
        let mut file = fs::File::create(&tmp_path).map_err(|e| TxError::Io {
            context: format!("创建临时文件 {}", tmp_path.display()),
            source: e,
        })?;
        file.write_all(content).map_err(|e| TxError::Io {
            context: format!("写入临时文件 {}", tmp_path.display()),
            source: e,
        })?;
        file.sync_all().map_err(|e| TxError::Io {
            context: format!("落盘临时文件 {}", tmp_path.display()),
            source: e,
        })?;
        drop(file);
        fs::rename(&tmp_path, path).map_err(|e| TxError::Io {
            context: format!("原子替换 {} -> {}", tmp_path.display(), path.display()),
            source: e,
        })
    })();
    if write_result.is_err() {
        // 失败时尽力清理临时文件；清理失败不掩盖原始错误。
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "model = \"gpt-5\"\nthinking = \"medium\"\n";
    const UPDATED: &str = "model = \"gpt-5\"\nthinking = \"high\"\n";

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi-config.toml");
        fs::write(&path, ORIGINAL).unwrap();
        (dir, path)
    }

    #[test]
    fn begin_on_missing_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let err = ConfigTransaction::begin(&missing, UPDATED.to_string()).unwrap_err();
        assert!(matches!(err, TxError::Io { .. }));
    }

    #[test]
    fn preview_shows_removed_and_added_lines() {
        let (_dir, path) = setup();
        let tx = ConfigTransaction::begin(&path, UPDATED.to_string()).unwrap();
        let diff = tx.preview();
        assert!(diff.contains("-thinking = \"medium\""));
        assert!(diff.contains("+thinking = \"high\""));
    }

    #[test]
    fn commit_replaces_file_and_keeps_verified_backup() {
        let (_dir, path) = setup();
        let tx = ConfigTransaction::begin(&path, UPDATED.to_string()).unwrap();
        let receipt = tx.commit().unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), UPDATED);
        assert_eq!(
            fs::read_to_string(&receipt.backup_path).unwrap(),
            ORIGINAL
        );
        assert_eq!(receipt.path, path);
        assert_eq!(receipt.original_sha256, sha256_hex(ORIGINAL.as_bytes()));
        assert_eq!(receipt.new_sha256, sha256_hex(UPDATED.as_bytes()));
    }

    #[test]
    fn commit_detects_external_modification_as_conflict_without_writing() {
        let (dir, path) = setup();
        let tx = ConfigTransaction::begin(&path, UPDATED.to_string()).unwrap();

        let external = "model = \"gpt-5\"\n# 外部编辑\n";
        fs::write(&path, external).unwrap();

        let err = tx.commit().unwrap_err();
        assert!(matches!(err, TxError::Conflict));
        // 冲突时不产生任何写入：外部内容保持原样，目录中无备份/临时残留。
        assert_eq!(fs::read_to_string(&path).unwrap(), external);
        let residual: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("halo-bak") || n.contains("halo-tmp"))
            .collect();
        assert!(residual.is_empty(), "残留文件：{residual:?}");
    }

    #[test]
    fn rollback_restores_original_content_with_hash_verification() {
        let (_dir, path) = setup();
        let tx = ConfigTransaction::begin(&path, UPDATED.to_string()).unwrap();
        let receipt = tx.commit().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), UPDATED);

        rollback(&receipt).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), ORIGINAL);
    }

    #[test]
    fn rollback_refuses_tampered_backup_and_leaves_target_untouched() {
        let (_dir, path) = setup();
        let tx = ConfigTransaction::begin(&path, UPDATED.to_string()).unwrap();
        let receipt = tx.commit().unwrap();

        fs::write(&receipt.backup_path, "被篡改的备份").unwrap();

        let err = rollback(&receipt).unwrap_err();
        assert!(matches!(err, TxError::VerifyFailed { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), UPDATED);
    }

    #[test]
    fn sequential_transactions_have_distinct_backups() {
        let (_dir, path) = setup();
        let tx1 = ConfigTransaction::begin(&path, UPDATED.to_string()).unwrap();
        let receipt1 = tx1.commit().unwrap();

        let next = "model = \"gpt-5\"\nthinking = \"low\"\n";
        let tx2 = ConfigTransaction::begin(&path, next.to_string()).unwrap();
        let receipt2 = tx2.commit().unwrap();

        assert_ne!(receipt1.backup_path, receipt2.backup_path);
        assert_eq!(fs::read_to_string(&path).unwrap(), next);
        // 回滚第二笔事务后应回到第一笔的结果。
        rollback(&receipt2).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), UPDATED);
    }
}
