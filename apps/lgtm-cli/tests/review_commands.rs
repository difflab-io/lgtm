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
            vec!["session", "start", "--notes"],
            vec!["session", "show", "--notes"],
            vec!["session", "complete", "--notes"],
            vec!["session", "export", "--notes"],
            vec!["session", "refresh", "--pr", "acme/repo#1"],
            vec!["session", "submit", "--pr", "acme/repo#1"],
            vec!["session", "list", "--all"],
            vec!["comments", "list", "--notes"],
            vec!["comments", "add", "--notes", "--body", "Dummy"],
            vec!["comments", "reply", "thread", "--notes", "--body", "Dummy"],
            vec!["comments", "resolve", "thread", "--notes"],
            vec!["comments", "reopen", "thread", "--notes"],
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
