//! accept/reject 只写本地结论、绝不触碰 Git（契约 3.5）。
//! 决定前后分别记录 `git rev-parse HEAD` 与 `git status --porcelain`，
//! 断言两者完全不变：不提交/不回滚/不删除，工作树与索引原样保留。

mod support;

use serde_json::json;
use support::{fake_pi_exe, git_capture, Sidecar, TestRepo};

/// 跑完 hello → open/trust → config → runtime → task 的 happy 链路，
/// 返回 (task_id, evidence_version)，任务处于 review_ready。
fn run_chain_to_review_ready(sc: &mut Sidecar, repo: &TestRepo, title: &str) -> (String, u64) {
    sc.hello();
    sc.open_and_trust(&repo.path_str());
    let cfg = sc.save_config("pi", &fake_pi_exe(), &[], None);
    sc.start_runtime("pi", &cfg);
    let task_id = sc.create_task("pi", &cfg, title);
    let finished = sc.wait_task_finished(&task_id);
    assert_eq!(finished["outcome"], "finished");
    let version = finished["evidence_version"]
        .as_u64()
        .expect("缺少 evidence_version");
    (task_id, version)
}

/// 仓库状态快照：(HEAD 提交, porcelain status 全文)。
fn git_state(repo: &TestRepo) -> (String, String) {
    (
        git_capture(&repo.root, &["rev-parse", "HEAD"]),
        git_capture(&repo.root, &["status", "--porcelain"]),
    )
}

#[test]
fn accept_never_touches_git_head_or_worktree() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    let (task_id, version) = run_chain_to_review_ready(&mut sc, &repo, "accept 不动 Git");

    let (head_before, status_before) = git_state(&repo);
    // Agent 写入的未跟踪文件与基线脏文件都应在 status 中——决定后必须原样保留
    assert!(status_before.contains("hello_from_agent.txt"), "{status_before}");
    assert!(status_before.contains("tracked_dirty.txt"), "{status_before}");

    let decision = sc.ok(
        "delivery.accept",
        json!({"task_id": task_id, "evidence_version": version}),
    );
    assert_eq!(decision["decision"]["kind"], "accepted");

    let (head_after, status_after) = git_state(&repo);
    assert_eq!(head_before, head_after, "accept 不得移动 HEAD");
    assert_eq!(status_before, status_after, "accept 不得改动工作树/索引");
}

#[test]
fn reject_never_touches_git_head_or_worktree() {
    let repo = TestRepo::new();
    let mut sc = Sidecar::start(&[]);
    let (task_id, version) = run_chain_to_review_ready(&mut sc, &repo, "reject 不动 Git");

    let (head_before, status_before) = git_state(&repo);
    assert!(status_before.contains("hello_from_agent.txt"), "{status_before}");

    let decision = sc.ok(
        "delivery.reject",
        json!({"task_id": task_id, "evidence_version": version, "reason": "不符合预期"}),
    );
    assert_eq!(decision["decision"]["kind"], "rejected");

    let (head_after, status_after) = git_state(&repo);
    assert_eq!(head_before, head_after, "reject 不得移动 HEAD");
    assert_eq!(status_before, status_after, "reject 不得回滚/删除工作区文件");
    // 拒绝后 Agent 产物文件仍在磁盘上
    assert!(
        repo.root.join("hello_from_agent.txt").exists(),
        "reject 不得删除 Agent 写入的文件"
    );
}
