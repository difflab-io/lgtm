//! Integration tests for review commands.
mod review_commands {
    //! Review commands report missing RPC server without accessing persistence locally.
    use std::process::Command;

    // tests ------------------------------------------------------------------

    #[test]
    fn should_reject_commands_without_backend_or_creating_runtime_files() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("unused-home");
        for args in [
            vec!["session", "start", "--directory"],
            vec!["session", "show", "--directory"],
            vec!["session", "complete", "--directory"],
            vec!["session", "export", "--directory"],
            vec!["session", "refresh", "--pr", "acme/repo#1"],
            vec!["session", "submit", "--pr", "acme/repo#1"],
            vec!["session", "list", "--all"],
            vec!["comments", "list", "--directory"],
            vec!["comments", "add", "--directory", "--body", "Dummy"],
            vec![
                "comments",
                "reply",
                "thread",
                "--directory",
                "--body",
                "Dummy",
            ],
            vec!["comments", "resolve", "thread", "--directory"],
            vec!["comments", "reopen", "thread", "--directory"],
        ] {
            let out = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .env("LGTM_HOME", &home)
                .env("LGTM_DATABASE_URL", "invalid://must-not-connect")
                .current_dir(root.path())
                .args(&args)
                .output()
                .unwrap();
            assert!(!out.status.success(), "{args:?}");
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("backend"),
                "{args:?}: {:?}",
                out
            );
            assert!(out.stdout.is_empty());
            assert!(!home.exists(), "{args:?} created runtime or database files");
        }
    }

    #[test]
    fn should_expose_review_command_help() {
        for command in ["review", "session", "comments"] {
            let out = Command::new(env!("CARGO_BIN_EXE_lgtm"))
                .args([command, "--help"])
                .output()
                .unwrap();
            assert!(out.status.success());
        }
    }
}
