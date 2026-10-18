//! Provider contracts at the real HTTP boundary.
#[allow(dead_code)]
mod common;
use common::github_http::Fixture;
use lgtm_backend::forge::{Github, Repository};
use serde_json::json;
use std::sync::{Arc, Mutex};

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_use_only_fixture_credentials_and_preserve_server_paging() {
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    let fixture = Fixture::new(move |r| {
        assert!(
            r.headers
                .to_lowercase()
                .contains("bearer lgtm-test-fixture-only")
        );
        captured.lock().unwrap().push(r.path.clone());
        (
            200,
            json!([{"number":7,"title":"Review","head":{"sha":"head"},"base":{"sha":"base"}}]),
        )
    });
    let github = Github::for_test(&fixture.endpoint).unwrap();
    let (entries, next) = github
        .get_pull_requests(
            &Repository::parse("https://github.com/acme/repo.git").unwrap(),
            "",
            2,
        )
        .await
        .unwrap();
    assert_eq!(entries[0].head, "head");
    assert_eq!(entries[0].repository_url, "https://github.com/acme/repo");
    assert_eq!(next, 0);
    assert!(requests.lock().unwrap()[0].contains("page=2"));
}
#[tokio::test]
async fn should_surface_http_rejection_without_provider_body_or_credentials() {
    let fixture = Fixture::new(|_| {
        (
            403,
            json!({"message":"SECRET diagnostic must stay private","documentation_url":"https://docs.github.com"}),
        )
    });
    let error = Github::for_test(&fixture.endpoint)
        .unwrap()
        .get_account_id()
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("403"));
    assert!(!error.contains("SECRET"));
    assert!(!error.contains("fixture-only"));
}
#[tokio::test]
async fn should_refuse_partial_graphql_snapshots_and_unsupported_hosts() {
    let fixture = Fixture::new(|r| {
        if r.method == "GET" {
            (
                200,
                json!({"number":1,"title":"PR","head":{"sha":"head"},"base":{"sha":"base"}}),
            )
        } else {
            (200, json!({"errors":[{"message":"forbidden"}]}))
        }
    });
    let github = Github::for_test(&fixture.endpoint).unwrap();
    assert!(
        github
            .get_snapshot(
                &Repository::parse("https://github.com/acme/repo").unwrap(),
                1
            )
            .await
            .is_err()
    );
    assert!(Repository::parse("https://gitlab.com/acme/repo").is_err());
    assert!(Github::for_test("https://api.github.com").is_err());
    assert!(Github::for_test("http://user:secret@127.0.0.1:1234").is_err());
}
#[tokio::test]
async fn should_accept_empty_delete_response() {
    let fixture = Fixture::new(|r| {
        assert_eq!(r.method, "DELETE");
        (204, serde_json::Value::Null)
    });
    Github::for_test(&fixture.endpoint)
        .unwrap()
        .delete("/repos/acme/repo/issues/comments/1")
        .await
        .unwrap();
}
