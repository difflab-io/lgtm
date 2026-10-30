//! Skeleton CLI commands fail explicitly before opening any application state.
use std::process::Command;

// tests -----------------------------------------------------------------------

#[test]
fn should_reject_unimplemented_operations_without_starting_the_daemon() {
    // Arrange
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("unused home");
    for args in [
        vec!["commit", "--local", "--as", "Agent", "-m", "snapshot"],
        vec!["push", "--local"],
        vec!["review", "new", "--local"],
        vec!["review", "close", "review-id", "--with", "approve"],
        vec!["comment", "Feedback", "--local", "--as", "Agent"],
    ] {
        // Act
        let response = Command::new(env!("CARGO_BIN_EXE_lgtm"))
            .env("LGTM_HOME", &home)
            .arg("--json")
            .args(args)
            .output()
            .unwrap();
        // Assert
        assert!(!response.status.success());
        let body: serde_json::Value = serde_json::from_slice(&response.stdout).unwrap();
        assert_eq!(body["error"]["code"], "not_implemented");
        assert!(!home.exists());
    }
}
