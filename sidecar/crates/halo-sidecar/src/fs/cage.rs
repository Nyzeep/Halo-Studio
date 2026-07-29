//! 工作区路径牢笼。
//!
//! 这里的函数是 `fs.*` 的安全核心：调用方只能拿到经 canonicalize 验证仍在
//! 工作区中的绝对路径，因而符号链接和 Windows junction 无法逃逸。

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use super::FsError;

/// 写类操作的已解析目标。`abs` 的父目录已 canonicalize 并经过牢笼校验。
#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub abs: PathBuf,
    pub exists: bool,
}

/// 解析一个必须已存在的读类目标（list/read/stat）。
pub fn resolve_existing(root: &Path, rel: &str) -> Result<PathBuf, FsError> {
    precheck_syntax(rel, false)?;
    let root_canonical = canonical_root(root)?;
    let candidate = root_canonical.join(rel);
    if !candidate.exists() {
        return Err(FsError::NotFound(rel.to_string()));
    }
    let canonical = std::fs::canonicalize(&candidate)
        .map_err(|err| FsError::Io(format!("无法解析路径 {rel}：{err}")))?;
    ensure_within_root(&root_canonical, &canonical)?;
    Ok(canonical)
}

/// 解析一个写类目标。目标可不存在，但父目录必须存在且位于工作区牢笼内。
pub fn resolve_target(root: &Path, rel: &str) -> Result<ResolvedTarget, FsError> {
    precheck_syntax(rel, true)?;
    let root_canonical = canonical_root(root)?;
    let candidate = root_canonical.join(rel);
    let parent = candidate
        .parent()
        .ok_or_else(|| FsError::InvalidName(rel.to_string()))?;
    if !parent.exists() {
        return Err(FsError::NotFound(rel.to_string()));
    }
    let parent_canonical = std::fs::canonicalize(parent)
        .map_err(|err| FsError::Io(format!("无法解析父目录 {rel}：{err}")))?;
    ensure_within_root(&root_canonical, &parent_canonical)?;

    let name = candidate
        .file_name()
        .ok_or_else(|| FsError::InvalidName("目标路径不能为空".to_string()))?;
    let abs = parent_canonical.join(name);
    let exists = abs.exists();
    if exists {
        let canonical = std::fs::canonicalize(&abs)
            .map_err(|err| FsError::Io(format!("无法解析目标 {rel}：{err}")))?;
        ensure_within_root(&root_canonical, &canonical)?;
    }
    Ok(ResolvedTarget { abs, exists })
}

/// 写类操作的 `.git` 保护。读类操作不调用本函数，因而可只读观察 Git 元数据。
pub fn ensure_not_git_protected(root: &Path, abs: &Path) -> Result<(), FsError> {
    let root_canonical = canonical_root(root)?;
    ensure_within_root(&root_canonical, abs)?;
    let rel = to_wire_rel(&root_canonical, abs);
    let first = rel.split('/').next().unwrap_or_default();
    if first.eq_ignore_ascii_case(".git") {
        return Err(FsError::GitProtected(rel));
    }
    Ok(())
}

/// 将已验证的绝对路径转为协议规定的 `/` 相对路径。
pub fn to_wire_rel(root_canonical: &Path, abs: &Path) -> String {
    let root_len = root_canonical.components().count();
    abs.components()
        .skip(root_len)
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_root(root: &Path) -> Result<PathBuf, FsError> {
    std::fs::canonicalize(root).map_err(|err| FsError::Io(format!("无法解析工作区根目录：{err}")))
}

fn precheck_syntax(rel: &str, creating: bool) -> Result<(), FsError> {
    if rel.contains('\0') {
        return Err(FsError::OutsideWorkspace(rel.to_string()));
    }
    let path = Path::new(rel);
    if path.is_absolute() || path.has_root() {
        return Err(FsError::OutsideWorkspace(rel.to_string()));
    }

    let mut normal_count = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(FsError::OutsideWorkspace(rel.to_string()));
            }
            Component::Normal(part) => {
                normal_count += 1;
                if creating {
                    validate_creating_component(part)?;
                }
            }
            Component::CurDir => {}
        }
    }
    if creating && normal_count == 0 {
        return Err(FsError::InvalidName("目标路径不能为空".to_string()));
    }
    Ok(())
}

fn validate_creating_component(part: &OsStr) -> Result<(), FsError> {
    let name = part.to_string_lossy();
    if name.ends_with(' ') || name.ends_with('.') {
        return Err(FsError::InvalidName(name.into_owned()));
    }
    let base = name.split('.').next().unwrap_or_default();
    let upper = base.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.starts_with("COM") && reserved_number(&upper[3..]))
        || (upper.starts_with("LPT") && reserved_number(&upper[3..]));
    if reserved {
        return Err(FsError::InvalidName(name.into_owned()));
    }
    Ok(())
}

fn reserved_number(value: &str) -> bool {
    matches!(value, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

fn ensure_within_root(root_canonical: &Path, candidate: &Path) -> Result<(), FsError> {
    let root_parts: Vec<_> = root_canonical.components().collect();
    let candidate_parts: Vec<_> = candidate.components().collect();
    let contained = candidate_parts.len() >= root_parts.len()
        && root_parts
            .iter()
            .zip(candidate_parts.iter())
            .all(|(root_part, candidate_part)| component_eq(*root_part, *candidate_part));
    if contained {
        Ok(())
    } else {
        Err(FsError::OutsideWorkspace(
            candidate.to_string_lossy().into_owned(),
        ))
    }
}

fn component_eq(left: Component<'_>, right: Component<'_>) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_names_reject_windows_device_names_and_trailing_dots() {
        let root = tempfile::tempdir().unwrap();
        for path in ["CON", "com1.txt", "dir/aux", "bad.", "bad "] {
            assert!(matches!(
                resolve_target(root.path(), path),
                Err(FsError::InvalidName(_))
            ));
        }
    }

    #[test]
    fn wire_paths_use_forward_slashes() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("src").join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let root_canonical = std::fs::canonicalize(root.path()).unwrap();
        assert_eq!(to_wire_rel(&root_canonical, &nested), "src/nested");
    }
}
