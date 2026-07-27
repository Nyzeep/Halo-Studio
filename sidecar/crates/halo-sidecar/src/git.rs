//! GitClient：以子进程方式调用 git.exe 的只读观察客户端。
//!
//! 红线（docs/module-contracts.md 第 6 节）：绝不执行 commit/push/branch/checkout/
//! stash 等修改性命令；唯一的"写"是把工作区快照写入**临时索引**（GIT_INDEX_FILE
//! 指向临时文件）再 write-tree，不触碰真实索引与工作树。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 工作区校验错误：区分路径无效 / 不可读 / 非 Git 仓库，供上层映射契约错误码。
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("{0}")]
    PathInvalid(String),
    #[error("{0}")]
    NotReadable(String),
    #[error("{0}")]
    NotGit(String),
    #[error("Git 命令执行失败：{0}")]
    Command(String),
}

/// 工作区真实路径校验结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceProbe {
    pub real_path: String,
    pub git_root: String,
    pub root_commit: Option<String>,
}

/// 任务关联变更中的单文件条目；change 取值为契约锁定的小写蛇形字符串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub change: String,
    pub diff: String,
}

pub struct GitClient {
    repo: PathBuf,
}

impl GitClient {
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        GitClient { repo: repo.into() }
    }

    /// 真实路径校验：存在 → canonicalize → 可读 → git 仓库。
    pub fn validate_workspace(path: &str) -> Result<WorkspaceProbe, GitError> {
        let meta = std::fs::metadata(path)
            .map_err(|_| GitError::PathInvalid(format!("工作区路径不存在或无法访问：{path}")))?;
        if !meta.is_dir() {
            return Err(GitError::PathInvalid(format!(
                "工作区路径不是目录：{path}"
            )));
        }
        let real = std::fs::canonicalize(path)
            .map_err(|_| GitError::PathInvalid(format!("工作区路径无法解析为真实路径：{path}")))?;
        let real_path = strip_verbatim(&real);
        std::fs::read_dir(&real)
            .map_err(|_| GitError::NotReadable(format!("工作区目录不可读取：{real_path}")))?;

        let client = GitClient::new(&real);
        let toplevel = client
            .run(&["rev-parse", "--show-toplevel"], None)
            .map_err(|_| GitError::NotGit(format!("该目录不是 Git 仓库：{real_path}")))?;
        let toplevel = first_line(&toplevel);
        if toplevel.is_empty() {
            return Err(GitError::NotGit(format!("该目录不是 Git 仓库：{real_path}")));
        }
        // git 在 Windows 上输出正斜杠路径；canonicalize 统一为系统真实路径
        let git_root = match std::fs::canonicalize(&toplevel) {
            Ok(p) => strip_verbatim(&p),
            Err(_) => toplevel.replace('/', "\\"),
        };

        let root_commit = GitClient::new(&git_root).root_commit()?;
        Ok(WorkspaceProbe {
            real_path,
            git_root,
            root_commit,
        })
    }

    /// 仓库首个提交（目录替换检测锚点）；空仓库（unborn HEAD）返回 None。
    pub fn root_commit(&self) -> Result<Option<String>, GitError> {
        match self.run(&["rev-list", "--max-parents=0", "HEAD"], None) {
            Ok(out) => Ok(non_empty(first_line(&out))),
            // 空仓库没有 HEAD：如实返回 None，而不是错误
            Err(_) => Ok(None),
        }
    }

    /// 当前 HEAD 提交；空仓库返回 None。
    pub fn head(&self) -> Result<Option<String>, GitError> {
        match self.run(&["rev-parse", "--verify", "HEAD"], None) {
            Ok(out) => Ok(non_empty(first_line(&out))),
            Err(_) => Ok(None),
        }
    }

    /// 临时索引基线：GIT_INDEX_FILE=<临时文件> git add -A + git write-tree。
    /// 返回包含全部已跟踪与未跟踪（未被忽略）文件的树对象哈希；真实索引不受影响。
    pub fn capture_tree(&self) -> Result<String, GitError> {
        let idx = std::env::temp_dir().join(format!("halo-baseline-index-{}", uuid::Uuid::new_v4()));
        let result = (|| {
            self.run(&["add", "-A"], Some(&idx))?;
            let out = self.run(&["write-tree"], Some(&idx))?;
            let tree = first_line(&out);
            if tree.is_empty() {
                return Err(GitError::Command("write-tree 未返回树对象".to_string()));
            }
            Ok(tree)
        })();
        let _ = std::fs::remove_file(&idx);
        result
    }

    /// `git status --porcelain -z` 脏文件清单（含未跟踪；重命名同时记录新旧路径）。
    pub fn status_dirty_files(&self) -> Result<Vec<String>, GitError> {
        let out = self.run(&["status", "--porcelain", "-z"], None)?;
        let mut files = Vec::new();
        let mut tokens = out.split('\0');
        while let Some(entry) = tokens.next() {
            if entry.len() < 4 {
                continue;
            }
            let status = &entry[..2];
            let path = entry[3..].to_string();
            files.push(path);
            // 重命名/复制条目带第二个 NUL 分隔的原路径
            if status.starts_with('R') || status.starts_with('C') {
                if let Some(orig) = tokens.next() {
                    if !orig.is_empty() {
                        files.push(orig.to_string());
                    }
                }
            }
        }
        Ok(files)
    }

    /// 两棵树之间的关联变更，按文件切分（含 added/modified/deleted/renamed 与
    /// per-file diff 文本）。
    pub fn diff_trees(&self, base_tree: &str, end_tree: &str) -> Result<Vec<FileDiff>, GitError> {
        // 1. 名称与状态（-z 避免路径转义，空格/中文路径原样输出）
        let raw = self.run(
            &["diff-tree", "-r", "-M", "--name-status", "-z", base_tree, end_tree],
            None,
        )?;
        let mut entries: Vec<(String, String)> = Vec::new(); // (path, change)
        let mut tokens = raw.split('\0');
        while let Some(status) = tokens.next() {
            if status.is_empty() {
                continue;
            }
            let kind = match status.chars().next() {
                Some('A') => "added",
                Some('D') => "deleted",
                Some('R') => "renamed",
                Some('C') => "added",
                _ => "modified",
            };
            let Some(first_path) = tokens.next() else { break };
            let path = if kind == "renamed" || status.starts_with('C') {
                // R/C：第一个是旧路径，第二个是新路径
                match tokens.next() {
                    Some(newer) => newer.to_string(),
                    None => first_path.to_string(),
                }
            } else {
                first_path.to_string()
            };
            entries.push((path, kind.to_string()));
        }

        // 2. 整体 patch，按 "diff --git " 边界切分后与路径匹配
        let patch = self.run(&["diff-tree", "-r", "-M", "-p", base_tree, end_tree], None)?;
        let chunks = split_patch_chunks(&patch);

        Ok(entries
            .into_iter()
            .map(|(path, change)| {
                let diff = chunks
                    .iter()
                    .find(|(p, _)| p == &path)
                    .map(|(_, text)| text.clone())
                    .unwrap_or_default();
                FileDiff { path, change, diff }
            })
            .collect())
    }

    /// 统一 git 调用入口：只读命令 + 临时索引写入两类。
    /// core.quotepath=off 使空格与中文路径原样输出。
    fn run(&self, args: &[&str], index_file: Option<&Path>) -> Result<String, GitError> {
        let mut cmd = Command::new("git");
        cmd.arg("-c")
            .arg("core.quotepath=off")
            .args(args)
            .current_dir(&self.repo)
            .stdin(Stdio::null());
        if let Some(idx) = index_file {
            cmd.env("GIT_INDEX_FILE", idx);
        }
        let output = cmd
            .output()
            .map_err(|e| GitError::Command(format!("无法启动 git 进程：{e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let brief: String = stderr.chars().take(300).collect();
            return Err(GitError::Command(format!(
                "git {} 退出码非零：{}",
                args.first().unwrap_or(&""),
                brief.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// 去掉 Windows canonicalize 产生的 \\?\ 前缀，保持用户可读路径。
fn strip_verbatim(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// 把整体 patch 文本按 "diff --git " 起始行切分为 (路径, 单文件 diff) 列表。
/// 路径优先取 "+++ b/…"；删除文件取 "--- a/…"；纯重命名取 "rename to …"。
fn split_patch_chunks(patch: &str) -> Vec<(String, String)> {
    let mut chunks: Vec<(String, String)> = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            if let Some(lines) = current.take() {
                if let Some(entry) = chunk_to_entry(&lines) {
                    chunks.push(entry);
                }
            }
            current = Some(vec![line]);
        } else if let Some(lines) = current.as_mut() {
            lines.push(line);
        }
    }
    if let Some(lines) = current.take() {
        if let Some(entry) = chunk_to_entry(&lines) {
            chunks.push(entry);
        }
    }
    chunks
}

fn chunk_to_entry(lines: &[&str]) -> Option<(String, String)> {
    // 路径含空格时 git 在 ---/+++ 行尾追加制表符消歧，须剥掉
    let clean = |s: &str| s.trim_end_matches(['\t', '\r']).to_string();
    let mut path: Option<String> = None;
    for line in lines {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            path = Some(clean(rest));
            break;
        }
        if line.starts_with("+++ /dev/null") {
            // 删除文件：向前找 --- a/
            for l in lines {
                if let Some(rest) = l.strip_prefix("--- a/") {
                    path = Some(clean(rest));
                    break;
                }
            }
            break;
        }
    }
    if path.is_none() {
        for line in lines {
            if let Some(rest) = line.strip_prefix("rename to ") {
                path = Some(clean(rest));
                break;
            }
        }
    }
    path.map(|p| (p, lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 在 tempdir 下建真实 git 仓库；目录名故意含空格与中文。
    fn init_repo(root: &Path) -> PathBuf {
        let repo = root.join("我的 测试仓库");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        repo
    }

    fn git(repo: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-c")
            .arg("core.quotepath=off")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("git 不可用");
        assert!(
            out.status.success(),
            "git {:?} 失败：{}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn commit_all(repo: &Path, msg: &str) {
        git(repo, &["add", "-A"]);
        git(
            repo,
            &[
                "-c",
                "user.name=halo-test",
                "-c",
                "user.email=halo@test.local",
                "commit",
                "-m",
                msg,
                "--no-gpg-sign",
            ],
        );
    }

    #[test]
    fn validate_rejects_missing_path_as_invalid() {
        let err = GitClient::validate_workspace("Z:\\不存在\\no-such-dir-42").unwrap_err();
        assert!(matches!(err, GitError::PathInvalid(_)), "{err:?}");
    }

    #[test]
    fn validate_rejects_plain_dir_as_not_git() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("普通 目录");
        fs::create_dir_all(&dir).unwrap();
        let err = GitClient::validate_workspace(dir.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, GitError::NotGit(_)), "{err:?}");
    }

    #[test]
    fn validate_rejects_file_path_as_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("一个文件.txt");
        fs::write(&file, "x").unwrap();
        let err = GitClient::validate_workspace(file.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, GitError::PathInvalid(_)), "{err:?}");
    }

    #[test]
    fn validate_resolves_real_path_root_commit_for_spaced_cjk_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = init_repo(tmp.path());
        fs::write(repo.join("a.txt"), "hello").unwrap();
        commit_all(&repo, "初始提交");

        let probe = GitClient::validate_workspace(repo.to_str().unwrap()).unwrap();
        assert!(probe.real_path.contains("我的 测试仓库"), "{}", probe.real_path);
        assert!(!probe.real_path.starts_with(r"\\?\"));
        assert!(probe.git_root.ends_with("我的 测试仓库"), "{}", probe.git_root);
        let root = probe.root_commit.expect("已有提交的仓库应有根提交");
        assert_eq!(root.len(), 40);
    }

    #[test]
    fn validate_empty_repo_has_no_root_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = init_repo(tmp.path());
        let probe = GitClient::validate_workspace(repo.to_str().unwrap()).unwrap();
        assert_eq!(probe.root_commit, None);
    }

    #[test]
    fn capture_tree_includes_untracked_and_leaves_real_index_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = init_repo(tmp.path());
        fs::write(repo.join("tracked.txt"), "v1").unwrap();
        commit_all(&repo, "初始提交");
        fs::write(repo.join("未跟踪 文件.txt"), "new").unwrap();

        let client = GitClient::new(&repo);
        let status_before = git(&repo, &["status", "--porcelain"]);
        let tree = client.capture_tree().unwrap();
        assert_eq!(tree.len(), 40);
        // 真实索引未被污染：status 输出不变，未跟踪文件仍是未跟踪
        let status_after = git(&repo, &["status", "--porcelain"]);
        assert_eq!(status_before, status_after);
        assert!(status_after.contains("?? "), "{status_after}");
    }

    #[test]
    fn diff_trees_reports_added_modified_deleted_renamed_with_per_file_diff() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = init_repo(tmp.path());
        fs::write(repo.join("modify.txt"), "old content\nline2\n").unwrap();
        fs::write(repo.join("delete.txt"), "to be removed\n").unwrap();
        let rename_body: String = "stable line\n".repeat(30);
        fs::write(repo.join("rename-old.txt"), &rename_body).unwrap();
        commit_all(&repo, "基线提交");

        let client = GitClient::new(&repo);
        let base = client.capture_tree().unwrap();

        fs::write(repo.join("modify.txt"), "new content\nline2\n").unwrap();
        fs::remove_file(repo.join("delete.txt")).unwrap();
        fs::rename(repo.join("rename-old.txt"), repo.join("rename-new.txt")).unwrap();
        fs::write(repo.join("新增 文件.txt"), "添加的中文内容\n").unwrap();

        let end = client.capture_tree().unwrap();
        let diffs = client.diff_trees(&base, &end).unwrap();

        let by_path = |p: &str| {
            diffs
                .iter()
                .find(|d| d.path == p)
                .unwrap_or_else(|| panic!("缺少 {p}：{diffs:?}"))
        };
        let added = by_path("新增 文件.txt");
        assert_eq!(added.change, "added");
        assert!(added.diff.contains("添加的中文内容"), "{}", added.diff);

        let modified = by_path("modify.txt");
        assert_eq!(modified.change, "modified");
        assert!(modified.diff.contains("-old content"));
        assert!(modified.diff.contains("+new content"));

        let deleted = by_path("delete.txt");
        assert_eq!(deleted.change, "deleted");
        assert!(deleted.diff.contains("-to be removed"));

        let renamed = by_path("rename-new.txt");
        assert_eq!(renamed.change, "renamed");
        assert!(
            renamed.diff.contains("rename from rename-old.txt"),
            "{}",
            renamed.diff
        );
        assert_eq!(diffs.len(), 4, "{diffs:?}");
    }

    #[test]
    fn status_dirty_files_lists_baseline_dirt() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = init_repo(tmp.path());
        fs::write(repo.join("clean.txt"), "committed").unwrap();
        fs::write(repo.join("dirty.txt"), "v1").unwrap();
        commit_all(&repo, "初始提交");
        fs::write(repo.join("dirty.txt"), "v2 已修改").unwrap();
        fs::write(repo.join("未跟踪 中文.txt"), "untracked").unwrap();

        let client = GitClient::new(&repo);
        let dirty = client.status_dirty_files().unwrap();
        assert!(dirty.iter().any(|f| f == "dirty.txt"), "{dirty:?}");
        assert!(dirty.iter().any(|f| f == "未跟踪 中文.txt"), "{dirty:?}");
        assert!(!dirty.iter().any(|f| f == "clean.txt"), "{dirty:?}");
    }

    #[test]
    fn head_none_for_empty_repo_some_after_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = init_repo(tmp.path());
        let client = GitClient::new(&repo);
        assert_eq!(client.head().unwrap(), None);
        fs::write(repo.join("a.txt"), "x").unwrap();
        commit_all(&repo, "首个提交");
        assert!(client.head().unwrap().is_some());
    }
}
