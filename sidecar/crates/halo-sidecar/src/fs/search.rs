//! 有界的工作区文件搜索。

use std::fs;
use std::path::Path;
use std::time::Instant;

use globset::{Glob, GlobSet, GlobSetBuilder};
use halo_protocol::methods::fs::{FsSearchItem, FsSearchParams, FsSearchResult};
use regex::RegexBuilder;

use crate::git::GitClient;

use super::cage;
use super::limits;
use super::FsError;

/// 在 Git 候选文件中按可选 glob 与内容正则搜索。
pub fn search(root: &Path, git: &GitClient, params: &FsSearchParams) -> Result<FsSearchResult, FsError> {
    if params.max_results == 0 || params.max_results > limits::FS_SEARCH_MAX_RESULTS {
        return Err(FsError::InvalidName(format!(
            "max_results 必须位于 1 到 {} 之间",
            limits::FS_SEARCH_MAX_RESULTS
        )));
    }
    let root_canonical = std::fs::canonicalize(root)
        .map_err(|err| FsError::Io(format!("无法解析工作区根目录：{err}")))?;
    let matcher = build_glob(params.glob.as_deref())?;
    let candidates = git
        .ls_candidate_files()
        .map_err(|err| FsError::Io(format!("枚举 Git 候选文件失败：{err}")))?;
    let paths: Vec<_> = candidates
        .into_iter()
        .filter(|path| matcher.as_ref().is_none_or(|set| set.is_match(path)))
        .collect();

    let Some(query) = params.query.as_deref() else {
        let truncated = paths.len() > params.max_results as usize;
        let items = paths
            .into_iter()
            .take(params.max_results as usize)
            .map(path_only_item)
            .collect();
        return Ok(FsSearchResult {
            items,
            truncated,
            scanned_files: 0,
        });
    };

    let regex = RegexBuilder::new(query)
        .case_insensitive(!params.case_sensitive)
        .build()
        .map_err(|err| FsError::InvalidName(format!("搜索正则无效：{err}")))?;
    let started = Instant::now();
    let mut scanned_files = 0u64;
    let mut items = Vec::new();
    let mut truncated = false;

    for path in paths {
        if started.elapsed().as_millis() >= u128::from(limits::FS_SEARCH_TIME_BUDGET_MS) {
            truncated = true;
            break;
        }
        if items.len() >= params.max_results as usize {
            truncated = true;
            break;
        }
        let abs = match cage::resolve_existing(&root_canonical, &path) {
            Ok(abs) => abs,
            Err(FsError::NotFound(_)) => continue,
            Err(err) => return Err(err),
        };
        let metadata = match fs::metadata(&abs) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => continue,
            Err(_) => continue,
        };
        if metadata.len() > limits::FS_SEARCH_FILE_MAX_BYTES {
            continue;
        }
        let bytes = match fs::read(&abs) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if bytes
            .iter()
            .take(limits::FS_BINARY_SNIFF_BYTES)
            .any(|byte| *byte == 0)
        {
            continue;
        }
        scanned_files += 1;
        let content = String::from_utf8_lossy(&bytes);
        for matched in regex.find_iter(&content).take(limits::FS_SEARCH_FILE_MAX_MATCHES) {
            if items.len() >= params.max_results as usize {
                truncated = true;
                break;
            }
            items.push(match_item(&path, &content, matched.start()));
        }
        if truncated {
            break;
        }
    }

    Ok(FsSearchResult {
        items,
        truncated,
        scanned_files,
    })
}

fn build_glob(pattern: Option<&str>) -> Result<Option<GlobSet>, FsError> {
    let Some(pattern) = pattern.filter(|pattern| !pattern.trim().is_empty()) else {
        return Ok(None);
    };
    let glob = Glob::new(pattern)
        .map_err(|err| FsError::InvalidName(format!("glob 无效：{err}")))?;
    let mut builder = GlobSetBuilder::new();
    builder.add(glob);
    builder
        .build()
        .map(Some)
        .map_err(|err| FsError::InvalidName(format!("glob 无效：{err}")))
}

fn path_only_item(path: String) -> FsSearchItem {
    FsSearchItem {
        path,
        line: None,
        column: None,
        preview: None,
        preview_truncated: None,
    }
}

fn match_item(path: &str, content: &str, start: usize) -> FsSearchItem {
    let prefix = &content[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = content[line_start..start].chars().count() as u32 + 1;
    let line_end = content[start..]
        .find('\n')
        .map_or(content.len(), |offset| start + offset);
    let (preview, preview_truncated) = truncate_preview(&content[line_start..line_end]);
    FsSearchItem {
        path: path.to_string(),
        line: Some(line),
        column: Some(column),
        preview: Some(preview),
        preview_truncated: Some(preview_truncated),
    }
}

fn truncate_preview(text: &str) -> (String, bool) {
    if text.len() <= limits::FS_PREVIEW_MAX_BYTES {
        return (text.to_string(), false);
    }
    let mut end = limits::FS_PREVIEW_MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}
