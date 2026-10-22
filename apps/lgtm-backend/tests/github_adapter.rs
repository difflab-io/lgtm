//! Provider contracts at the real HTTP boundary.
use lgtm_backend::forge::ForgeClient;
#[allow(dead_code)]
mod common;
use common::github_http::Fixture;
use lgtm_backend::forge::{GithubClient, Repository};
use serde_json::json;
use std::sync::{Arc, Mutex};

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_list_branch_prs_without_search_indexing_or_per_pr_requests() {
    let heads = Arc::new(Mutex::new(vec![]));
    let captured = heads.clone();
    let fixture = Fixture::new(move |r| {
        let url = url::Url::parse(&format!("http://localhost{}", r.path)).unwrap();
        assert_eq!(url.path(), "/repos/acme/repo/pulls");
        let params = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        captured.lock().unwrap().push(params["head"].to_string());
        assert_eq!(params["page"], "2");
        assert_eq!(params["state"], "open");
        (
            200,
            json!([{"number":7,"title":"Review","head":{"sha":"head"},"base":{"sha":"base"}}]),
        )
    });
    let github = GithubClient::for_test(&fixture.endpoint).unwrap();
    let repo = Repository::from_repository_url("https://github.com/acme/repo").unwrap();
    for query in ["head:feature/review", "head:contributor:feature/review"] {
        let (entries, next) = github.get_pull_requests(&repo, query, 2).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].number, 7);
        assert_eq!(next, 0);
    }
    assert_eq!(
        *heads.lock().unwrap(),
        ["acme:feature/review", "contributor:feature/review"]
    );
}

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
    let github = GithubClient::for_test(&fixture.endpoint).unwrap();
    let (entries, next) = github
        .get_pull_requests(
            &Repository::from_repository_url("https://github.com/acme/repo.git").unwrap(),
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
    let error = GithubClient::for_test(&fixture.endpoint)
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
    let github = GithubClient::for_test(&fixture.endpoint).unwrap();
    assert!(
        github
            .get_snapshot(
                &Repository::from_repository_url("https://github.com/acme/repo").unwrap(),
                1
            )
            .await
            .is_err()
    );
    assert!(Repository::from_repository_url("https://gitlab.com/acme/repo").is_err());
    assert!(GithubClient::for_test("https://api.github.com").is_err());
    assert!(GithubClient::for_test("http://user:secret@127.0.0.1:1234").is_err());
}
#[tokio::test]
async fn should_accept_empty_delete_response() {
    let fixture = Fixture::new(|r| {
        assert_eq!(r.method, "DELETE");
        (204, serde_json::Value::Null)
    });
    GithubClient::for_test(&fixture.endpoint)
        .unwrap()
        .delete("/repos/acme/repo/issues/comments/1")
        .await
        .unwrap();
}

#[tokio::test]
async fn should_keep_original_single_line_coordinates_separate_from_relocated_current_ranges() {
    use lgtm_backend::models::Anchor;
    let fixture = Fixture::new(|r| {
        if r.path == "/repos/acme/repo/pulls/1" {
            return (
                200,
                json!({"number":1,"title":"PR","head":{"sha":"current"},"base":{"sha":"base"},"state":"open"}),
            );
        }
        if r.path == "/graphql" {
            return (
                200,
                json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[{
                "id":"T1","path":"file.txt","line":3,"startLine":3,"originalLine":2,"originalStartLine":null,
                "diffSide":"RIGHT","subjectType":"LINE","isResolved":false,"isOutdated":false,
                "comments":{"nodes":[{"databaseId":1,"body":"Existing private draft","updatedAt":"2026-10-20T00:00:00Z",
                    "originalCommit":{"oid":"original"},"commit":{"oid":"current"},"author":{"databaseId":1,"login":"fixture"},"pullRequestReview":{"databaseId":5,"state":"PENDING"}}],"pageInfo":{"hasNextPage":false}}
            }],"pageInfo":{"hasNextPage":false}}}}}}),
            );
        }
        (200, json!([]))
    });
    let snapshot = GithubClient::for_test(&fixture.endpoint)
        .unwrap()
        .get_snapshot(
            &Repository::from_repository_url("https://github.com/acme/repo").unwrap(),
            1,
        )
        .await
        .unwrap();
    let thread = &snapshot.threads[0];
    assert!(matches!(
        &thread.anchor,
        Anchor::Line {
            start: 2,
            end: 2,
            ..
        }
    ));
    assert!(matches!(
        &thread.observation.as_ref().unwrap().current,
        Some(Anchor::Line {
            start: 3,
            end: 3,
            ..
        })
    ));
    let observation = thread.observation.as_ref().unwrap();
    assert_eq!(observation.head, "current");
    assert!(
        serde_json::to_value(&thread.anchor)
            .unwrap()
            .get("commit")
            .is_none()
    );
}

#[tokio::test]
async fn should_repeat_only_the_observation_when_commits_change_while_loading_pages() {
    let reads = Arc::new(Mutex::new(0));
    let observed = reads.clone();
    let fixture = Fixture::new(move |r| {
        if r.path == "/repos/acme/repo/pulls/1" {
            let mut count = observed.lock().unwrap();
            *count += 1;
            return (
                200,
                json!({"number":1,"title":"PR","head":{"sha":if *count == 1 {"old"} else {"current"}},"base":{"sha":"base"},"state":"open"}),
            );
        }
        if r.path == "/graphql" {
            return (
                200,
                json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}}),
            );
        }
        assert_eq!(
            r.method, "GET",
            "observation recovery must never write to GitHub"
        );
        (200, json!([]))
    });
    let snapshot = GithubClient::for_test(&fixture.endpoint)
        .unwrap()
        .get_snapshot(
            &Repository::from_repository_url("https://github.com/acme/repo").unwrap(),
            1,
        )
        .await
        .unwrap();
    assert_eq!(snapshot.head, "current");
    assert_eq!(
        *reads.lock().unwrap(),
        4,
        "two bounded complete observations"
    );
}

#[tokio::test]
async fn should_follow_sdk_next_page_links_even_when_the_page_is_not_full() {
    let endpoint = Arc::new(Mutex::new(String::new()));
    let base = endpoint.clone();
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    let fixture = Fixture::with_headers(move |request| {
        captured.lock().unwrap().push(request.path.clone());
        if request.path.contains("cursor=second") {
            (200, json!([{"id":2}]), vec![])
        } else {
            (
                200,
                json!([{"id":1}]),
                vec![(
                    "link",
                    format!(
                        "<{}/comments?cursor=second>; rel=\"next\"",
                        base.lock().unwrap()
                    ),
                )],
            )
        }
    });
    *endpoint.lock().unwrap() = fixture.endpoint.clone();
    let client = octocrab::Octocrab::builder()
        .base_uri(&fixture.endpoint)
        .unwrap()
        .personal_token("lgtm-test-fixture-only".to_owned())
        .build()
        .unwrap();
    let entries = lgtm_backend::extensions::gh::get_all(&client, "/comments")
        .await
        .unwrap();
    assert_eq!(entries, vec![json!({"id":1}), json!({"id":2})]);
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert!(requests.lock().unwrap()[1].contains("cursor=second"));
}
