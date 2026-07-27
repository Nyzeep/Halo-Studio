//! 受限文件读写、目录列举与元数据操作。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use halo_protocol::methods::fs::{
    FsEncoding, FsEntry, FsEntryKind, FsLineEnding, FsListResult, FsReadResult, FsWriteEncoding,
    FsWriteResult,
};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use super::cage;
use super::limits;
use super::FsError;

/// 列举目录。`depth` 为 1 时只返回直接子项；递归结果按父先子后的先序排列。
pub fn list(root: &Path, rel: &str, depth: u32) -> Result<FsListResult, FsError> {
    if !(1..=limits::FS_LIST_MAX_DEPTH).contains(&depth) {
        return Err(FsError::InvalidName(format!(
            "depth 必须位于 1 到 {} 之间",
            limits::FS_LIST_MAX_DEPTH
        )));
    }
    let root_canonical = canonical_root(root)?;
    let directory = cage::resolve_existing(&root_canonical, rel)?;
    if !directory.is_dir() {
        return Err(FsError::InvalidName(
            "fs.list 的 path 必须是目录".to_string(),
        ));
    }

    let mut entries = Vec::new();
    let mut truncated = false;
    walk_directory(
        &root_canonical,
        &directory,
        1,
        depth,
        &mut entries,
        &mut truncated,
    )?;
    Ok(FsListResult {
        path: cage::to_wire_rel(&root_canonical, &directory),
        entries,
        truncated,
    })
}

/// 读取一个不大于 8 MiB 的文本文件。
pub fn read(root: &Path, rel: &str) -> Result<FsReadResult, FsError> {
    let root_canonical = canonical_root(root)?;
    let abs = cage::resolve_existing(&root_canonical, rel)?;
    let metadata = fs::metadata(&abs).map_err(io_error("读取文件元数据失败"))?;
    if !metadata.is_file() {
        return Err(FsError::InvalidName(
            "fs.read 的 path 必须是文件".to_string(),
        ));
    }
    if metadata.len() > limits::FS_READ_MAX_BYTES {
        return Err(FsError::TooLarge {
            size: metadata.len(),
        });
    }

    let bytes = fs::read(&abs).map_err(io_error("读取文件失败"))?;
    let size = bytes.len() as u64;
    let (content, encoding, lossy) = decode_content(&bytes, size)?;
    Ok(FsReadResult {
        path: cage::to_wire_rel(&root_canonical, &abs),
        line_ending: detect_line_ending(&content),
        content,
        encoding,
        lossy,
        hash: sha256(&bytes),
        size,
        mtime: format_mtime(&metadata)?,
        readonly: metadata.permissions().readonly(),
    })
}

/// 使用乐观锁原子覆盖已有文件。
pub fn write(
    root: &Path,
    rel: &str,
    content: &str,
    expected_hash: &str,
    encoding: FsWriteEncoding,
) -> Result<FsWriteResult, FsError> {
    let root_canonical = canonical_root(root)?;
    let target = cage::resolve_target(&root_canonical, rel)?;
    cage::ensure_not_git_protected(&root_canonical, &target.abs)?;
    if !target.exists {
        return Err(FsError::NotFound(rel.to_string()));
    }

    let current = fs::read(&target.abs).map_err(io_error("读取待写入文件失败"))?;
    let current_hash = sha256(&current);
    if current_hash != expected_hash {
        let metadata = fs::metadata(&target.abs).map_err(io_error("读取冲突文件元数据失败"))?;
        return Err(FsError::Conflict {
            current_hash,
            mtime: format_mtime(&metadata)?,
        });
    }

    let bytes = encode_content(content, encoding);
    if bytes.len() as u64 > limits::FS_WRITE_MAX_BYTES {
        return Err(FsError::TooLarge {
            size: bytes.len() as u64,
        });
    }
    atomic_replace(&target.abs, &bytes)?;
    let metadata = fs::metadata(&target.abs).map_err(io_error("写入后读取文件元数据失败"))?;
    Ok(FsWriteResult {
        path: cage::to_wire_rel(&root_canonical, &target.abs),
        hash: sha256(&bytes),
        size: bytes.len() as u64,
        mtime: format_mtime(&metadata)?,
    })
}

/// 创建单个 UTF-8 文件；不覆盖已有目标。
pub fn create_file(root: &Path, rel: &str, content: &str) -> Result<FsEntry, FsError> {
    let root_canonical = canonical_root(root)?;
    let target = cage::resolve_target(&root_canonical, rel)?;
    cage::ensure_not_git_protected(&root_canonical, &target.abs)?;
    if target.exists {
        return Err(FsError::AlreadyExists(rel.to_string()));
    }
    let bytes = content.as_bytes();
    if bytes.len() as u64 > limits::FS_WRITE_MAX_BYTES {
        return Err(FsError::TooLarge {
            size: bytes.len() as u64,
        });
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target.abs)
        .map_err(io_error("创建文件失败"))?;
    file.write_all(bytes).map_err(io_error("写入新文件失败"))?;
    file.sync_all().map_err(io_error("同步新文件失败"))?;
    entry_from_path(&root_canonical, &target.abs)
}

/// 创建单层目录；不自动补全父目录。
pub fn create_dir(root: &Path, rel: &str) -> Result<FsEntry, FsError> {
    let root_canonical = canonical_root(root)?;
    let target = cage::resolve_target(&root_canonical, rel)?;
    cage::ensure_not_git_protected(&root_canonical, &target.abs)?;
    if target.exists {
        return Err(FsError::AlreadyExists(rel.to_string()));
    }
    fs::create_dir(&target.abs).map_err(io_error("创建目录失败"))?;
    entry_from_path(&root_canonical, &target.abs)
}

/// 重命名或移动一个文件系统条目，目标不可存在。
pub fn rename(root: &Path, from: &str, to: &str) -> Result<FsEntry, FsError> {
    let root_canonical = canonical_root(root)?;
    let source = cage::resolve_existing(&root_canonical, from)?;
    cage::ensure_not_git_protected(&root_canonical, &source)?;
    let target = cage::resolve_target(&root_canonical, to)?;
    cage::ensure_not_git_protected(&root_canonical, &target.abs)?;
    if target.exists {
        return Err(FsError::AlreadyExists(to.to_string()));
    }
    fs::rename(&source, &target.abs).map_err(io_error("重命名文件失败"))?;
    entry_from_path(&root_canonical, &target.abs)
}

/// 返回单个文件或目录的元数据。
pub fn stat(root: &Path, rel: &str) -> Result<FsEntry, FsError> {
    let root_canonical = canonical_root(root)?;
    let abs = cage::resolve_existing(&root_canonical, rel)?;
    entry_from_path(&root_canonical, &abs)
}

fn walk_directory(
    root_canonical: &Path,
    directory: &Path,
    level: u32,
    max_depth: u32,
    output: &mut Vec<FsEntry>,
    truncated: &mut bool,
) -> Result<(), FsError> {
    let mut children = fs::read_dir(directory)
        .map_err(io_error("读取目录失败"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error("枚举目录项失败"))?;
    children.retain(|entry| {
        !(directory == root_canonical
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(".git"))
    });
    children.sort_by(|left, right| {
        let left_type = left.file_type().ok();
        let right_type = right.file_type().ok();
        let left_dir = left_type.map(|kind| kind.is_dir()).unwrap_or(false);
        let right_dir = right_type.map(|kind| kind.is_dir()).unwrap_or(false);
        right_dir.cmp(&left_dir).then_with(|| {
            left.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .cmp(&right.file_name().to_string_lossy().to_ascii_lowercase())
        })
    });

    for child in children {
        if output.len() >= limits::FS_LIST_MAX_ENTRIES {
            *truncated = true;
            return Ok(());
        }
        let file_type = child.file_type().map_err(io_error("读取目录项类型失败"))?;
        let path = child.path();
        output.push(entry_from_path(root_canonical, &path)?);
        if level < max_depth && file_type.is_dir() && !file_type.is_symlink() {
            walk_directory(
                root_canonical,
                &path,
                level + 1,
                max_depth,
                output,
                truncated,
            )?;
            if *truncated {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn entry_from_path(root_canonical: &Path, path: &Path) -> Result<FsEntry, FsError> {
    let metadata = fs::symlink_metadata(path).map_err(io_error("读取文件元数据失败"))?;
    let kind = if metadata.file_type().is_symlink() {
        FsEntryKind::Symlink
    } else if metadata.is_dir() {
        FsEntryKind::Dir
    } else {
        FsEntryKind::File
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(FsEntry {
        name,
        path: cage::to_wire_rel(root_canonical, path),
        kind,
        size: if matches!(kind, FsEntryKind::File) {
            metadata.len()
        } else {
            0
        },
        mtime: format_mtime(&metadata)?,
        readonly: metadata.permissions().readonly(),
    })
}

fn canonical_root(root: &Path) -> Result<PathBuf, FsError> {
    std::fs::canonicalize(root).map_err(io_error("无法解析工作区根目录"))
}

fn decode_content(bytes: &[u8], size: u64) -> Result<(String, FsEncoding, bool), FsError> {
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        if let Ok(text) = String::from_utf8(rest.to_vec()) {
            return Ok((text, FsEncoding::Utf8Bom, false));
        }
    }
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        if let Some(text) = decode_utf16(rest, true) {
            return Ok((text, FsEncoding::Utf16le, false));
        }
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        if let Some(text) = decode_utf16(rest, false) {
            return Ok((text, FsEncoding::Utf16be, false));
        }
    }
    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return Ok((text, FsEncoding::Utf8, false));
    }
    if bytes
        .iter()
        .take(limits::FS_BINARY_SNIFF_BYTES)
        .any(|byte| *byte == 0)
    {
        return Err(FsError::Binary { size });
    }
    Ok((
        String::from_utf8_lossy(bytes).into_owned(),
        FsEncoding::Unknown,
        true,
    ))
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units).ok()
}

fn encode_content(content: &str, encoding: FsWriteEncoding) -> Vec<u8> {
    match encoding {
        FsWriteEncoding::Utf8 => content.as_bytes().to_vec(),
        FsWriteEncoding::Utf8Bom => {
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend_from_slice(content.as_bytes());
            bytes
        }
        FsWriteEncoding::Utf16le => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes
        }
        FsWriteEncoding::Utf16be => {
            let mut bytes = vec![0xfe, 0xff];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            bytes
        }
    }
}

fn detect_line_ending(content: &str) -> FsLineEnding {
    let has_crlf = content.contains("\r\n");
    let without_crlf = content.replace("\r\n", "");
    let has_lf = without_crlf.contains('\n') || without_crlf.contains('\r');
    match (has_crlf, has_lf) {
        (false, false) => FsLineEnding::None,
        (false, true) => FsLineEnding::Lf,
        (true, false) => FsLineEnding::Crlf,
        (true, true) => FsLineEnding::Mixed,
    }
}

fn atomic_replace(target: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let parent = target
        .parent()
        .ok_or_else(|| FsError::Io("目标缺少父目录".to_string()))?;
    let temporary = parent.join(format!("halo-fs-tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(io_error("创建临时文件失败"))?;
        file.write_all(bytes)
            .map_err(io_error("写入临时文件失败"))?;
        file.sync_all().map_err(io_error("同步临时文件失败"))?;
        fs::rename(&temporary, target).map_err(io_error("原子替换文件失败"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{digest:x}")
}

fn format_mtime(metadata: &fs::Metadata) -> Result<String, FsError> {
    let modified = metadata.modified().map_err(io_error("读取修改时间失败"))?;
    let timestamp = OffsetDateTime::from(modified);
    timestamp
        .format(&Rfc3339)
        .map_err(|err| FsError::Io(format!("格式化修改时间失败：{err}")))
}

fn io_error(context: &'static str) -> impl FnOnce(std::io::Error) -> FsError {
    move |err| FsError::Io(format!("{context}：{err}"))
}
