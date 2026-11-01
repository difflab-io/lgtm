//! BDD integration tests for `SnapshotStore`, `extensions::gitoxide`, `extensions::git`, and `ReviewService` local/worktree/anchor/session behavior.

mod common;

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

use common::TestContext;
use lgtm_forge::{
    models::{IdentityKind, ReviewGroupBy, ReviewOutcome, SessionState},
    services::{CreateCommentInput, UpdateCommentInput},
    utils::compute_repo_slug,
};

// fixtures --------------------------------------------------------------------

fn write_test_transcript(suite: &str, case: &str, body: &str) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir);
    let out_dir = workspace_root
        .join("target")
        .join("test-outputs")
        .join("snapshot_store")
        .join(suite);
    std::fs::create_dir_all(&out_dir).expect("create test transcript output dir");
    let out_path = out_dir.join(format!("{case}.log"));
    std::fs::write(&out_path, body).expect("write test transcript log");
}

fn format_review_detail_transcript(detail: &lgtm_forge::services::ReviewDetail) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Review #{}  {}  [{}]  ({}..{})",
        detail.review.number,
        detail.review.name,
        if detail.review.context.is_local() {
            "local"
        } else {
            "remote"
        },
        &detail.review.base_sha[..7.min(detail.review.base_sha.len())],
        &detail.review.head_sha[..7.min(detail.review.head_sha.len())],
    );
    let _ = writeln!(out, "\nSessions ({}):", detail.sessions.len());
    for sess in &detail.sessions {
        let kind = match &sess.author.kind {
            IdentityKind::Human { .. } => "human".to_owned(),
            IdentityKind::Agent { provider, model } => format!("agent:{provider}/{model}"),
        };
        let _ = writeln!(
            out,
            "  - Session #{} by {} ({kind}) state={:?} outcome={:?} body={:?}",
            sess.number, sess.author.name, sess.state, sess.outcome, sess.body
        );
    }
    let _ = writeln!(out, "\nDiff Hunks ({}):", detail.diff_hunks.len());
    for hunk in &detail.diff_hunks {
        let _ = writeln!(
            out,
            "  @@ {} @ {} (start_line={}) @@",
            hunk.file_path,
            &hunk.commit_sha[..7.min(hunk.commit_sha.len())],
            hunk.start_line
        );
        for line in &hunk.lines {
            let prefix = match line.kind {
                lgtm_forge::models::DiffLineKind::Context => ' ',
                lgtm_forge::models::DiffLineKind::Added => '+',
                lgtm_forge::models::DiffLineKind::Removed => '-',
            };
            let _ = writeln!(out, "  {prefix} {:>3} | {}", line.line_number, line.content);
        }
    }
    let _ = writeln!(out, "\nComments ({}):", detail.comments.len());
    for c in &detail.comments {
        let kind = match &c.author.kind {
            IdentityKind::Human { .. } => "human".to_owned(),
            IdentityKind::Agent { provider, model } => format!("agent:{provider}/{model}"),
        };
        let anchor_desc = match &c.anchor {
            Some(a) => format!(
                "{}:{:?}..{} @ {} (orig {:?}..{} @ {})",
                a.file_path,
                a.start_line,
                a.line,
                &a.commit_sha[..7.min(a.commit_sha.len())],
                a.original_start_line,
                a.original_line,
                &a.original_commit_sha[..7.min(a.original_commit_sha.len())],
            ),
            None => "general".to_owned(),
        };
        let _ = writeln!(
            out,
            "  - C{} [session={}] by {} ({kind}) reply_to={:?} resolved={} outdated={} anchor=[{anchor_desc}]: {}",
            c.number,
            c.session_id,
            c.author.name,
            c.reply_to_number,
            c.resolved,
            c.outdated,
            c.body
        );
    }
    out
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

// tests -----------------------------------------------------------------------

mod given_a_working_checkout {
    use super::*;

    pub mod when_staging_committing_and_snapshotting_locally {
        use super::*;

        #[tokio::test]
        async fn should_preserve_working_tree_index_head_and_branch_refs_unchanged() {
            // Arrange (Given a working checkout with uncommitted edits)
            let ctx = TestContext::setup().await;
            let initial_head = ctx.checkout_head_sha();
            let initial_index = ctx.git_stdout(&["ls-files", "--stage"]);
            let initial_refs = ctx.git_stdout(&["show-ref"]);
            ctx.write_file(
                "src/lib.rs",
                "pub fn alpha() -> i32 {\n    42\n}\n\npub fn beta() -> i32 {\n    2\n}\n",
            );
            let status_before = ctx.checkout_status_porcelain();

            // Act (When staging, committing, and snapshotting in --local mode)
            let staged = ctx
                .service
                .add_paths(&ctx.git_dir, &["src/lib.rs".to_owned()], true)
                .await
                .unwrap();
            let committed = ctx
                .service
                .commit_changes(&ctx.git_dir, Some("Local shadow commit"), true)
                .await
                .unwrap();
            let (base_sha, snap_sha) = ctx.snapshots.ensure_snapshot(&ctx.git_dir).unwrap();

            // Assert (Then checkout .git/index, HEAD, and refs remain untouched while shadow repo holds the snapshot)
            let after_head = ctx.checkout_head_sha();
            let after_index = ctx.git_stdout(&["ls-files", "--stage"]);
            let after_refs = ctx.git_stdout(&["show-ref"]);
            let status_after = ctx.checkout_status_porcelain();

            assert_eq!(staged, vec!["src/lib.rs".to_owned()]);
            assert_eq!(after_head, initial_head);
            assert_eq!(after_index, initial_index);
            assert_eq!(after_refs, initial_refs);
            assert_eq!(status_after, status_before);
            assert_eq!(base_sha, initial_head);
            assert_eq!(snap_sha, committed.commit_sha);
            assert!(
                !Path::new(&ctx.git_dir)
                    .join("lgtm-checkout-staged.json")
                    .exists()
            );

            let transcript = format!(
                "Scenario: Given a working checkout / When staging, committing, and snapshotting locally\n\
                 initial_head: {initial_head}\n\
                 after_head:   {after_head}\n\
                 shadow_sha:   {snap_sha}\n\
                 status_before:\n{status_before}\n\
                 status_after:\n{status_after}\n\
                 index_unchanged: {}\n\
                 refs_unchanged:  {}\n",
                after_index == initial_index,
                after_refs == initial_refs
            );
            write_test_transcript(
                "given_a_working_checkout",
                "should_preserve_working_tree_index_head_and_branch_refs_unchanged",
                &transcript,
            );
        }
    }
}

mod given_external_git_commands {
    use super::*;

    pub mod when_interleaving_native_git_add_and_lgtm_commit {
        use super::*;

        #[tokio::test]
        async fn should_share_native_git_index_without_creating_shadow_snapshot_repo() {
            // Arrange (Given a working checkout where the user stages a file with native `git add`)
            let ctx = TestContext::setup().await;
            let initial_head = ctx.checkout_head_sha();
            ctx.write_file(
                "src/external.rs",
                "pub fn from_native_git() -> bool {\n    true\n}\n",
            );
            ctx.git(&["add", "src/external.rs"]);
            let staged_before_commit = ctx.git_stdout(&["diff", "--cached", "--name-only"]);

            // Act (When running non-local lgtm commit)
            let committed = ctx
                .service
                .commit_changes(&ctx.git_dir, Some("Commit native git staged file"), false)
                .await
                .unwrap();

            // Assert (Then the commit lands on the native branch with human authorship and no shadow snapshot repo is created)
            let head_after = ctx.checkout_head_sha();
            let status_after = ctx.checkout_status_porcelain();
            let snap_dir = ctx.snapshots.snapshot_dir(&ctx.git_dir).unwrap();

            assert_eq!(staged_before_commit, "src/external.rs");
            assert_eq!(committed.commit_sha, head_after);
            assert_ne!(head_after, initial_head);
            assert!(
                status_after.is_empty(),
                "working tree should be clean: {status_after}"
            );
            assert!(
                !snap_dir.exists(),
                "non-local git operations must not create ~/.lgtm/snapshots/$repoSlug"
            );
            assert_eq!(
                committed.author.kind,
                IdentityKind::Human {
                    email: "ada@example.com".to_owned(),
                    forge_login: None,
                }
            );

            let transcript = format!(
                "Scenario: Given external git commands / When interleaving native git add and lgtm commit\n\
                 staged_via_native_git: {staged_before_commit}\n\
                 committed_sha:         {}\n\
                 commit_author:         {} ({:?})\n\
                 working_tree_clean:    {}\n\
                 snapshot_dir_created:  {}\n",
                committed.commit_sha,
                committed.author.name,
                committed.author.kind,
                status_after.is_empty(),
                snap_dir.exists(),
            );
            write_test_transcript(
                "given_external_git_commands",
                "should_share_native_git_index_without_creating_shadow_snapshot_repo",
                &transcript,
            );
        }
    }
}

mod given_linked_git_worktrees {
    use super::*;

    pub mod when_creating_local_snapshots_across_branches {
        use super::*;

        #[tokio::test]
        async fn should_share_canonical_repo_slug_and_isolate_shadow_refs_per_branch() {
            // Arrange (Given a primary checkout and a linked `git worktree` on a second branch)
            let ctx = TestContext::setup().await;
            let wt_second: PathBuf = ctx.worktree_dir.parent().unwrap().join("repo-worktree-b");
            let wt_second_str = wt_second.to_string_lossy().into_owned();
            ctx.git(&[
                "worktree",
                "add",
                "-b",
                "feature/worktree-b",
                &wt_second_str,
            ]);

            ctx.write_file("src/lib.rs", "pub fn branch_a() -> i32 {\n    100\n}\n");
            std::fs::write(
                wt_second.join("src/lib.rs"),
                "pub fn branch_b() -> i32 {\n    200\n}\n",
            )
            .unwrap();

            // Act (When creating local reviews in both worktrees)
            let slug_primary = compute_repo_slug(&ctx.git_dir).unwrap();
            let slug_worktree = compute_repo_slug(&wt_second_str).unwrap();
            let (review_a, _) = ctx
                .service
                .create_review(&ctx.git_dir, true, None)
                .await
                .unwrap();
            let (review_b, _) = ctx
                .service
                .create_review(&wt_second_str, true, None)
                .await
                .unwrap();
            let all_local = ctx
                .service
                .get_reviews(&wt_second_str, true, true)
                .await
                .unwrap();

            // Assert (Then both worktrees resolve to the same $repoSlug and DB repo key while isolating shadow refs per branch)
            assert_eq!(slug_primary, slug_worktree);
            assert_ne!(review_a.head_sha, review_b.head_sha);
            assert_eq!(all_local.len(), 2);

            let snap_dir = ctx.snapshots.snapshot_dir(&ctx.git_dir).unwrap();
            let shadow_refs = git_in(
                &snap_dir,
                &["for-each-ref", "--format=%(refname) %(objectname)"],
            );
            assert!(shadow_refs.contains("refs/lgtm/snapshots/feature/diff-241"));
            assert!(shadow_refs.contains("refs/lgtm/snapshots/feature/worktree-b"));

            let transcript = format!(
                "Scenario: Given linked git worktrees / When creating local snapshots across branches\n\
                 slug_primary:  {slug_primary}\n\
                 slug_worktree: {slug_worktree}\n\
                 review_a:      {} (head={})\n\
                 review_b:      {} (head={})\n\
                 shadow_refs:\n{shadow_refs}\n",
                review_a.name, review_a.head_sha, review_b.name, review_b.head_sha
            );
            write_test_transcript(
                "given_linked_git_worktrees",
                "should_share_canonical_repo_slug_and_isolate_shadow_refs_per_branch",
                &transcript,
            );
        }
    }
}

mod given_an_unchanged_worktree {
    use super::*;

    pub mod when_calling_ensure_snapshot_repeatedly {
        use super::*;

        #[tokio::test]
        async fn should_return_the_same_shadow_commit_sha_idempotently() {
            // Arrange (Given a worktree with uncommitted edits)
            let ctx = TestContext::setup().await;
            ctx.write_file("src/lib.rs", "pub fn idempotent() -> i32 {\n    7\n}\n");

            // Act (When calling ensure_snapshot twice without changing any files)
            let (base_1, head_1) = ctx.snapshots.ensure_snapshot(&ctx.git_dir).unwrap();
            let (base_2, head_2) = ctx.snapshots.ensure_snapshot(&ctx.git_dir).unwrap();

            // Assert (Then both calls return the exact same base_sha and shadow head_sha)
            assert_eq!(base_1, base_2);
            assert_eq!(head_1, head_2);

            let transcript = format!(
                "Scenario: Given an unchanged worktree / When calling ensure_snapshot repeatedly\n\
                 first_call:  base={base_1} head={head_1}\n\
                 second_call: base={base_2} head={head_2}\n\
                 idempotent:  {}\n",
                head_1 == head_2
            );
            write_test_transcript(
                "given_an_unchanged_worktree",
                "should_return_the_same_shadow_commit_sha_idempotently",
                &transcript,
            );
        }
    }
}

mod given_an_active_local_review {
    use super::*;

    pub mod when_code_is_extracted_and_agent_relocates_anchors {
        use super::*;

        #[tokio::test]
        async fn should_mark_old_anchor_outdated_relocate_across_files_and_publish_agent_session_while_human_session_stays_draft()
         {
            // Arrange (Given a local review where a human reviewer comments on src/lib.rs:6)
            let ctx = TestContext::setup().await;
            ctx.write_file(
                "src/lib.rs",
                "pub fn alpha() -> i32 {\n    10\n}\n\npub fn compute_tax(amount: i32) -> i32 {\n    amount * 2\n}\n",
            );
            let (review, human_session) = ctx
                .service
                .create_review(&ctx.git_dir, true, None)
                .await
                .unwrap();
            let initial_snap_sha = review.head_sha.clone();
            let root_comment = ctx
                .service
                .create_comment(CreateCommentInput {
                    git_dir: ctx.git_dir.clone(),
                    review_selector: Some(review.name.clone()),
                    body: "Extract compute_tax into src/tax.rs and validate negative inputs"
                        .to_owned(),
                    file_path: Some("src/lib.rs".into()),
                    start_line: None,
                    line: Some(6),
                    reply_to: None,
                    local: true,
                    as_agent: None,
                })
                .await
                .unwrap();

            // Act 1 (When the agent extracts compute_tax to src/tax.rs, commits locally, and refreshes the review)
            ctx.write_file("src/lib.rs", "pub fn alpha() -> i32 {\n    10\n}\n");
            ctx.write_file(
                "src/tax.rs",
                "pub fn compute_tax(amount: i32) -> i32 {\n    assert!(amount >= 0);\n    amount * 2\n}\n",
            );
            let shadow_commit = ctx
                .service
                .commit_changes(
                    &ctx.git_dir,
                    Some("Extract compute_tax to src/tax.rs"),
                    true,
                )
                .await
                .unwrap();
            let (after_extract_review, shifted_count) = ctx
                .service
                .refresh_review(&ctx.git_dir, Some(&review.name), true)
                .await
                .unwrap();
            let detail_after_extract = ctx
                .service
                .get_review_detail(
                    &ctx.git_dir,
                    Some(&review.name),
                    true,
                    ReviewGroupBy::File,
                    false,
                )
                .await
                .unwrap();
            let outdated_root = detail_after_extract
                .comments
                .iter()
                .find(|c| c.number == root_comment.number)
                .unwrap();
            assert!(
                outdated_root.outdated,
                "extracted line in src/lib.rs must be marked outdated"
            );

            // Act 2 (And the agent relocates the comment anchor to src/tax.rs:3, replies in its own overlapping session, and closes its session)
            let relocated = ctx
                .service
                .update_comment(UpdateCommentInput {
                    git_dir: ctx.git_dir.clone(),
                    comment_number: root_comment.number,
                    body: None,
                    file_path: Some("src/tax.rs".into()),
                    start_line: Some(2),
                    line: Some(3),
                    resolved: None,
                    local: true,
                    as_agent: Some("anthropic/claude-sonnet-4-5".into()),
                })
                .await
                .unwrap();
            let agent_reply = ctx
                .service
                .create_comment(CreateCommentInput {
                    git_dir: ctx.git_dir.clone(),
                    review_selector: Some(review.name.clone()),
                    body: "Moved to src/tax.rs:2-3 with non-negative assertion".to_owned(),
                    file_path: None,
                    start_line: None,
                    line: None,
                    reply_to: Some(root_comment.number),
                    local: true,
                    as_agent: Some("anthropic/claude-sonnet-4-5".into()),
                })
                .await
                .unwrap();
            let (_, published_agent_session) = ctx
                .service
                .close_review(
                    &ctx.git_dir,
                    Some(&review.name),
                    true,
                    ReviewOutcome::Comment,
                    Some("Addressed extraction and relocated anchor".to_owned()),
                    Some("anthropic/claude-sonnet-4-5"),
                )
                .await
                .unwrap();
            let final_detail = ctx
                .service
                .get_review_detail(
                    &ctx.git_dir,
                    Some(&review.name),
                    true,
                    ReviewGroupBy::Session,
                    false,
                )
                .await
                .unwrap();

            // Assert (Then the comment is anchored on src/tax.rs:2-3 with outdated=false, original_* preserved, agent session Published, and human session still Draft)
            assert_eq!(shifted_count, 1);
            assert_eq!(
                shadow_commit.author.kind,
                IdentityKind::Human {
                    email: "ada@example.com".to_owned(),
                    forge_login: None,
                }
            );
            assert!(!relocated.outdated);
            let anchor = relocated.anchor.as_ref().unwrap();
            assert_eq!(anchor.file_path, "src/tax.rs");
            assert_eq!(anchor.start_line, Some(2));
            assert_eq!(anchor.line, 3);
            assert_eq!(anchor.commit_sha, after_extract_review.head_sha);
            assert_eq!(anchor.original_line, 6);
            assert_eq!(anchor.original_commit_sha, initial_snap_sha);

            assert_ne!(agent_reply.session_id, human_session.id);
            assert_eq!(published_agent_session.state, SessionState::Published);

            let human_sess_after = final_detail
                .sessions
                .iter()
                .find(|s| s.id == human_session.id)
                .unwrap();
            let agent_sess_after = final_detail
                .sessions
                .iter()
                .find(|s| s.id == published_agent_session.id)
                .unwrap();
            assert_eq!(
                human_sess_after.state,
                SessionState::Draft,
                "human session must remain Draft when agent publishes its overlapping session"
            );
            assert_eq!(agent_sess_after.state, SessionState::Published);
            assert_eq!(final_detail.diff_hunks.len(), 1);
            assert_eq!(final_detail.diff_hunks[0].file_path, "src/tax.rs");

            let mut transcript = String::new();
            let _ = writeln!(
                transcript,
                "Scenario: Given an active local review / When code is extracted and agent relocates anchors\n"
            );
            let _ = writeln!(
                transcript,
                "--- After Code Extraction (Before Agent Relocation) ---"
            );
            transcript.push_str(&format_review_detail_transcript(&detail_after_extract));
            let _ = writeln!(
                transcript,
                "\n--- After Agent Cross-File Relocation & Overlapping Session Publish ---"
            );
            transcript.push_str(&format_review_detail_transcript(&final_detail));

            write_test_transcript(
                "given_an_active_local_review",
                "should_mark_old_anchor_outdated_relocate_across_files_and_publish_agent_session_while_human_session_stays_draft",
                &transcript,
            );
        }
    }
}
