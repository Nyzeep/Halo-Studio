//! identity_changed 端到端（契约 3.1）：信任决定持久化键 =（real_path, root_commit）。
//! 目录被替换（删 .git 重新 init + 新初始提交 → root_commit 变化）后重新打开同一
//! 路径：必须降级为 untrusted 且 identity_changed=true，runtime.start 被
//! WORKSPACE_NOT_TRUSTED 拒绝，直到用户重新确认信任。

mod support;

use std::path::Path;

use serde_json::json;
use support::{fake_pi_exe, git, Sidecar, TestRepo};

/// Windows 下 git 对象文件带只读属性：先递归清除只读再删除整个目录。
fn remove_dir_all_force(dir: &Path) {
    clear_readonly(dir);
    std::fs::remove_dir_all(dir).expect("删除 .git 目录失败");
}

fn clear_readonly(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = std::fs::metadata(&p) {
            let mut perm = meta.permissions();
            if perm.readonly() {
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(&p, perm);
            }
            if meta.is_dir() {
                clear_readonly(&p);
            }
        }
    }
}

#[test]
fn rebuilt_repo_downgrades_trust_with_identity_changed() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    sc.hello();

    // 打开并信任，记录原 root_commit
    let ws = sc.ok("workspace.open", json!({"path": repo.path_str()}));
    let old_root = ws["root_commit"].as_str().expect("应有根提交").to_string();
    let ws_id = ws["workspace_id"].as_str().expect("缺少 workspace_id").to_string();
    let trusted = sc.ok(
        "workspace.trust",
        json!({"workspace_id": ws_id, "decision": "trust"}),
    );
    assert_eq!(trusted["trust"], "trusted");
    sc.ok("workspace.close", json!({}));

    // 目录替换：删 .git 重新 init + 新初始提交 → root_commit 必然变化
    remove_dir_all_force(&repo.root.join(".git"));
    git(&repo.root, &["init", "-b", "main"]);
    std::fs::write(repo.root.join("rebuilt.txt"), "重建后的新内容\n").expect("写文件失败");
    git(&repo.root, &["add", "-A"]);
    git(
        &repo.root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-m",
            "重建初始提交",
            "--no-gpg-sign",
        ],
    );

    // 重新打开同一路径：信任降级 + identity_changed=true
    let reopened = sc.ok("workspace.open", json!({"path": repo.path_str()}));
    assert_eq!(reopened["active"], true);
    assert_eq!(reopened["trust"], "untrusted", "目录替换后必须降级为 untrusted");
    assert_eq!(reopened["identity_changed"], true, "必须提示身份变化，要求重新确认");
    let new_root = reopened["root_commit"].as_str().expect("应有新根提交");
    assert_ne!(new_root, old_root, "重建后的根提交必须与原提交不同");

    // 降级后 runtime.start 被拒（不加载任何项目内配置/插件）
    let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);
    sc.err(
        "runtime.start",
        json!({"agent": "pi", "config_id": cfg}),
        "WORKSPACE_NOT_TRUSTED",
    );
}
