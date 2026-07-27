//! WORKSPACE_NOT_READABLE（尽力而为）：用 icacls 对临时目录拒绝当前用户的
//! 列目录权限（RD），workspace.open 应返回 WORKSPACE_NOT_READABLE。
//! 只拒绝 RD（列目录/读数据）而非整个 R：保留读属性权限，让 metadata/canonicalize
//! 通过、read_dir 失败，从而精确落在 NotReadable 分支而不是 PathInvalid。
//! 测试无论成败都经 Drop 守卫恢复 ACL，保证临时目录可被清理。

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use support::Sidecar;

/// deny ACE 守卫：构造时拒绝当前用户 RD，Drop 时移除该 deny 项。
struct DenyReadGuard {
    path: PathBuf,
    principal: String,
}

impl DenyReadGuard {
    fn apply(path: &Path) -> Option<DenyReadGuard> {
        let principal = current_user_sid_principal()?;
        let out = Command::new("icacls")
            .arg(path)
            .arg("/deny")
            .arg(format!("{principal}:(RD)"))
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(DenyReadGuard {
            path: path.to_path_buf(),
            principal,
        })
    }
}

fn current_user_sid_principal() -> Option<String> {
    let out = Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let sid = stdout.split(',').last()?.trim().trim_matches('"');
    sid.starts_with("S-").then(|| format!("*{sid}"))
}

impl Drop for DenyReadGuard {
    fn drop(&mut self) {
        // 必须恢复 ACL：否则 tempdir 清理失败，遗留垃圾目录
        let _ = Command::new("icacls")
            .arg(&self.path)
            .arg("/remove:d")
            .arg(&self.principal)
            .output();
    }
}

#[test]
fn unreadable_directory_maps_to_workspace_not_readable() {
    let tmp = tempfile::tempdir().expect("创建临时目录失败");
    let dir = tmp.path().join("不可读 目录");
    std::fs::create_dir_all(&dir).expect("创建目录失败");

    // 尽力而为：icacls 不可用或 deny 设置失败时跳过（不产生假失败）
    let guard = match DenyReadGuard::apply(&dir) {
        Some(g) => g,
        None => {
            eprintln!("跳过：icacls 不可用或无法设置 deny ACE，本机无法稳定构造不可读目录");
            return;
        }
    };
    if std::fs::read_dir(&dir).is_ok() {
        drop(guard);
        eprintln!("跳过：当前宿主 ACL 规则未能构造不可读目录");
        return;
    }

    let mut sc = Sidecar::start(&[]);
    sc.hello();
    let err = sc.err(
        "workspace.open",
        json!({"path": dir.to_string_lossy()}),
        "WORKSPACE_NOT_READABLE",
    );
    assert!(
        err["message"]
            .as_str()
            .unwrap_or_default()
            .contains("不可读"),
        "{err}"
    );

    // 显式恢复 ACL（panic 路径由 Drop 兜底），确保 tempdir 清理成功
    drop(guard);
}
