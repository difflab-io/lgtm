//! Integration tests for `lgtm-cli` local review, comment, and staging commands.

mod review_commands {
    use std::{
        path::{Path, PathBuf},
        process::{Command, Output},
    };
    use tempfile::TempDir;

    // fixtures ----------------------------------------------------------------

    struct CliWorkspace {
        _temp: TempDir,
        home: PathBuf,
        repo: PathBuf,
    }

    impl CliWorkspace {
        fn setup() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join("home");
            let repo = temp.path().join("repo");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(repo.join("src")).unwrap();

            Self::git(&repo, &["init", "-q", "-b", "feature/diff-260"]);
            Self::git(&repo, &["config", "user.name", "Ada Lovelace"]);
            Self::git(&repo, &["config", "user.email", "ada@example.com"]);
            std::fs::write(
                repo.join("src/lib.rs"),
                "pub fn alpha() -> i32 {\n    1\n}\n\npub fn beta() -> i32 {\n    2\n}\n",
            )
            .unwrap();
            Self::git(&repo, &["add", "."]);
            Self::git(&repo, &["commit", "-qm", "Initial commit"]);

            Self {
                _temp: temp,
                home,
                repo,
            }
        }

        // Private -------------------------------------------------------------

        fn lgtm(&self, args: &[&str]) -> Output {
            Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", &self.home)
                .env("RUST_LOG", "warn")
                .current_dir(&self.repo)
                .args(args)
                .output()
                .unwrap()
        }

        fn lgtm_json(&self, args: &[&str]) -> serde_json::Value {
            let mut full_args = Vec::with_capacity(args.len() + 1);
            full_args.extend_from_slice(args);
            full_args.push("--json");
            let out = self.lgtm(&full_args);
            assert!(
                out.status.success(),
                "lgtm {:?} failed:\nstdout: {}\nstderr: {}",
                full_args,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice(&out.stdout).unwrap()
        }

        fn git(repo: &Path, args: &[&str]) -> String {
            let out = Command::new("git")
                .current_dir(repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        }
    }

    impl Drop for CliWorkspace {
        fn drop(&mut self) {
            if self.home.exists() {
                let _ = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                    .env("LGTM_HOME", &self.home)
                    .current_dir(&self.repo)
                    .arg("down")
                    .output();
            }
        }
    }

    // tests -------------------------------------------------------------------

    #[test]
    fn should_run_local_add_commit_review_comment_refresh_show_and_close_end_to_end() {
        // Arrange
        let ws = CliWorkspace::setup();
        let initial_head = CliWorkspace::git(&ws.repo, &["rev-parse", "HEAD"]);
        std::fs::write(
            ws.repo.join("src/lib.rs"),
            "pub fn alpha() -> i32 {\n    10\n}\n\npub fn beta() -> i32 {\n    20\n}\n",
        )
        .unwrap();

        // Act
        let added = ws.lgtm_json(&["add", "src/lib.rs", "--local"]);
        let committed = ws.lgtm_json(&[
            "commit",
            "--local",
            "--as",
            "claude-sonnet-4-5",
            "-m",
            "Shadow snapshot commit",
        ]);
        let created = ws.lgtm_json(&["review", "new", "--local"]);
        let review_name = created["review"]["name"].as_str().unwrap().to_owned();
        let listed = ws.lgtm_json(&["review", "list", "--local"]);

        let comment = ws.lgtm_json(&[
            "comment",
            "new",
            "Extract constant",
            "--file",
            "src/lib.rs",
            "--line",
            "6",
            "--local",
            "--as",
            "claude-sonnet-4-5",
        ]);
        let comment_num = comment["comment"]["number"].as_u64().unwrap();
        let reply = ws.lgtm_json(&[
            "comment",
            "new",
            "Will extract in next pass",
            "--reply-to",
            &comment_num.to_string(),
            "--local",
        ]);
        let reply_num = reply["comment"]["number"].as_u64().unwrap();

        std::fs::write(
            ws.repo.join("src/lib.rs"),
            "// Header\npub fn alpha() -> i32 {\n    10\n}\n\npub fn beta() -> i32 {\n    20\n}\n",
        )
        .unwrap();
        let refreshed = ws.lgtm_json(&["review", "refresh", &review_name, "--local"]);
        let show_json = ws.lgtm_json(&[
            "review",
            "show",
            &review_name,
            "--local",
            "--group-by",
            "file",
            "--unresolved",
        ]);
        let show_text_out = ws.lgtm(&[
            "review",
            "show",
            &review_name,
            "--local",
            "--group-by",
            "file",
        ]);
        assert!(show_text_out.status.success());
        let show_text = String::from_utf8_lossy(&show_text_out.stdout);

        let updated = ws.lgtm_json(&[
            "comment",
            "update",
            &comment_num.to_string(),
            "--resolved",
            "true",
            "--local",
        ]);
        let deleted = ws.lgtm_json(&["comment", "delete", &reply_num.to_string(), "--local"]);
        let closed = ws.lgtm_json(&[
            "review",
            "close",
            &review_name,
            "--local",
            "--with",
            "approve",
            "--body",
            "LGTM local pass",
            "--as",
            "claude-sonnet-4-5",
        ]);

        // Assert
        assert_eq!(added["staged_paths"], serde_json::json!(["src/lib.rs"]));
        assert_eq!(committed["local"], true);
        assert_eq!(committed["author"]["kind"], "agent");
        assert_eq!(
            CliWorkspace::git(&ws.repo, &["rev-parse", "HEAD"]),
            initial_head
        );
        assert_eq!(listed["reviews"].as_array().unwrap().len(), 1);
        assert_eq!(refreshed["updated_anchors"], 1);
        assert_eq!(show_json["comments"][0]["anchor"]["line"], 7);
        assert_eq!(show_json["comments"][0]["anchor"]["original_line"], 6);
        assert!(!show_json["diff_hunks"].as_array().unwrap().is_empty());
        assert!(show_text.contains("══ src/lib.rs"));
        assert!(show_text.contains("── line 7"));
        assert!(show_text.contains("Extract constant"));
        assert_eq!(updated["comment"]["resolved"], true);
        assert_eq!(deleted["deleted_number"], reply_num);
        assert_eq!(closed["session"]["state"], "published");
        assert_eq!(closed["session"]["outcome"], "approve");
    }

    #[test]
    fn should_return_structured_json_error_for_invalid_diff_line_or_missing_review() {
        // Arrange
        let ws = CliWorkspace::setup();

        // Act
        let missing = ws.lgtm(&["review", "show", "R999-nonexistent", "--local", "--json"]);
        let push_local = ws.lgtm(&["push", "--local", "--json"]);

        // Assert
        assert!(!missing.status.success());
        let missing_body: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
        assert_eq!(missing_body["error"]["code"], "not_found");

        assert!(!push_local.status.success());
        let push_body: serde_json::Value = serde_json::from_slice(&push_local.stdout).unwrap();
        assert_eq!(push_body["error"]["code"], "invalid_argument");
    }
}
