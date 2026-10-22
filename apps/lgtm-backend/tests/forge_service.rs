//! Full submission orchestration with real databases and a dummy HTTP forge.
use lgtm_backend::models::ComparisonKind;
mod common;
use common::github_http::{Fixture, Request};
use lgtm_backend::{
    forge::GithubClient,
    models::{Anchor, CommitContext, ContextKey, ForgeRepositoryUrl},
    services::forge::ForgeService,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

// fixtures --------------------------------------------------------------------
#[derive(Default)]
struct Remote {
    review: Option<Value>,
    comments: Vec<Value>,
    threads: Vec<Value>,
    lose_draft_response: bool,
    writes: usize,
    content_reads: usize,
    lose_response: bool,
    head_changed: bool,
    base_changed: bool,
    lifecycle: &'static str,
    lose_mutation_response: bool,
    snapshot_unavailable: bool,
    event_failure: Option<u16>,
    missing_comment: bool,
    requests: Vec<String>,
    other_viewer: bool,
    deny_mutation: bool,
    account_switched: bool,
    author_is_viewer: bool,
}
fn fixture(state: Arc<Mutex<Remote>>) -> Fixture {
    Fixture::new(move |r: Request| {
        let mut state = state.lock().unwrap();
        state.requests.push(format!(
            "{} {} {}",
            r.method,
            r.path,
            r.body["query"].as_str().unwrap_or("")
        ));
        let path = r.path.split('?').next().unwrap();
        if r.method == "GET" {
            return match path {
                "/user" => (200, json!({"id":if state.account_switched {2} else {1}})),
                "/repos/acme/repo/pulls/1" => (
                    200,
                    json!({"number":1,"title":"PR","user":{"id":if state.author_is_viewer {1} else {2}},"head":{"sha":if state.head_changed{"changed"}else{"head"}},"base":{"sha":if state.base_changed {"new-base"} else {"base"}},"state":if state.lifecycle == "closed" || state.lifecycle == "merged" {"closed"} else {"open"},"merged":state.lifecycle == "merged"}),
                ),
                "/repos/acme/repo/pulls/1/files" => {
                    (200, json!([{"filename":"file.txt","status":"modified"}]))
                }
                "/repos/acme/repo/contents/file.txt" => {
                    state.content_reads += 1;
                    (
                        200,
                        json!({"encoding":"base64","size":4,"content":if r.path.contains("ref=head") {"bmV3Cg=="} else {"b2xkCg=="}}),
                    )
                }
                "/repos/acme/repo/pulls/1/reviews" => {
                    (200, json!(state.review.iter().collect::<Vec<_>>()))
                }
                "/repos/acme/repo/issues/1/comments" => (200, json!(state.comments)),
                "/repos/acme/repo/pulls/1/comments"
                | "/repos/acme/repo/pulls/1/reviews/5/comments" => (
                    200,
                    json!(
                        state
                            .threads
                            .iter()
                            .flat_map(|t| t["comments"]["nodes"].as_array().unwrap())
                            .map(|c| json!({"id":c["databaseId"],"body":c["body"],"user":{"id":1,"login":"fixture"}}))
                            .collect::<Vec<_>>()
                    ),
                ),
                _ => panic!("Unexpected GET {}", r.path),
            };
        }
        if path == "/graphql" {
            let query = r.body["query"].as_str().unwrap();
            if query.starts_with("mutation") && state.deny_mutation {
                return (403, json!({"message":"Not permitted"}));
            }
            if query.contains("updatePullRequestReviewComment") {
                state.writes += 1;
                state.threads[0]["comments"]["nodes"][0]["body"] =
                    r.body["variables"]["body"].clone();
                if state.lose_mutation_response {
                    state.snapshot_unavailable = true;
                    return (500, json!({"message":"accepted edit response lost"}));
                }
                return (
                    200,
                    json!({"data":{"updatePullRequestReviewComment":{"pullRequestReviewComment":state.threads[0]["comments"]["nodes"][0]}}}),
                );
            }
            if query.contains("deletePullRequestReviewComment") {
                state.writes += 1;
                for thread in &mut state.threads {
                    thread["comments"]["nodes"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|comment| comment["id"] != r.body["variables"]["id"]);
                }
                state
                    .threads
                    .retain(|thread| !thread["comments"]["nodes"].as_array().unwrap().is_empty());
                if state.lose_mutation_response {
                    state.snapshot_unavailable = true;
                    return (500, json!({"message":"accepted deletion response lost"}));
                }
                return (
                    200,
                    json!({"data":{"deletePullRequestReviewComment":{"clientMutationId":null}}}),
                );
            }
            if query.contains("resolveReviewThread(") || query.contains("unresolveReviewThread(") {
                state.writes += 1;
                state.threads[0]["isResolved"] = json!(!query.contains("unresolveReviewThread("));
                if state.lose_mutation_response {
                    state.snapshot_unavailable = true;
                    return (500, json!({"message":"accepted resolution response lost"}));
                }
                return (
                    200,
                    json!({"data":{(if query.contains("unresolveReviewThread(") {"unresolveReviewThread"} else {"resolveReviewThread"}):{"thread":state.threads[0]}}}),
                );
            }
            if query.contains("node(id:") {
                if state.snapshot_unavailable {
                    return (
                        503,
                        json!({"message":"target observation temporarily unavailable"}),
                    );
                }
                let node = state
                    .threads
                    .iter()
                    .find(|t| t["id"] == r.body["variables"]["id"]);
                return (
                    200,
                    json!({"data":{"node":node.map(|t| {let mut t=t.clone(); for c in t["comments"]["nodes"].as_array_mut().unwrap() {c["viewerDidAuthor"]=json!(!state.other_viewer);}t})}}),
                );
            }
            if query.contains("addPullRequestReviewThread(") {
                state.writes += 1;
                let input = &r.body["variables"]["input"];
                let thread = json!({"id":"T_10","path":input["path"],"subjectType":input["subjectType"],"line":input["line"],"startLine":input["startLine"],"diffSide":input["side"],"isResolved":false,"comments":{"nodes":[{"id":"C_10","databaseId":10,"body":input["body"],"updatedAt":"2026-10-18T00:00:00Z","author":{"databaseId":1,"login":"user"},"pullRequestReview":{"databaseId":5,"state":"PENDING"}}],"pageInfo":{"hasNextPage":false}}});
                state.threads.push(thread.clone());
                if state.lose_draft_response {
                    return (500, json!({"message":"draft response lost"}));
                }
                return (
                    200,
                    json!({"data":{"addPullRequestReviewThread":{"thread":thread}}}),
                );
            }
            assert!(query.contains("reviewThreads"));
            if state.snapshot_unavailable {
                return (
                    503,
                    json!({"message":"observation temporarily unavailable"}),
                );
            }
            return (
                200,
                json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":state.threads,"pageInfo":{"hasNextPage":false}}}}}}),
            );
        }
        if path == "/repos/acme/repo/pulls/comments/10" {
            if state
                .review
                .as_ref()
                .is_some_and(|v| v["state"] == "PENDING")
            {
                return (
                    404,
                    json!({"message":"Private pending comment unavailable through REST"}),
                );
            }
            state.writes += 1;
            if r.method == "PATCH" {
                state.threads[0]["comments"]["nodes"][0]["body"] = r.body["body"].clone();
                return (200, state.threads[0]["comments"]["nodes"][0].clone());
            }
            assert_eq!(r.method, "DELETE");
            state.threads.clear();
            return (204, Value::Null);
        }
        state.writes += 1;
        match path {
            "/repos/acme/repo/pulls/1/reviews" => {
                assert!(r.body.get("event").is_none());
                let review = json!({"id":5,"node_id":"PRR_fixture","state":"PENDING","body":r.body["body"],"commit_id":r.body["commit_id"],"user":{"id":1,"login":"fixture"}});
                state.review = Some(review.clone());
                (200, review)
            }
            "/repos/acme/repo/issues/1/comments" => {
                if state.missing_comment {
                    return (
                        503,
                        json!({"message":"comment not accepted","documentation_url":"https://docs.github.com"}),
                    );
                }
                let comment = json!({"id":10,"body":r.body["body"],"user":{"id":1,"login":"user"},"updated_at":"2026-10-18T00:00:00Z"});
                state.comments.push(comment.clone());
                (200, comment)
            }
            "/repos/acme/repo/pulls/1/reviews/5/events" => {
                if let Some(status) = state.event_failure.take() {
                    return (
                        status,
                        json!({"message":"event not accepted","documentation_url":"https://docs.github.com"}),
                    );
                }
                let review = state.review.as_mut().unwrap();
                review["state"] = json!(match r.body["event"].as_str().unwrap() {
                    "APPROVE" => "APPROVED",
                    "REQUEST_CHANGES" => "CHANGES_REQUESTED",
                    _ => "COMMENTED",
                });
                review["body"] = r.body["body"].clone();
                let response = review.clone();
                for thread in &mut state.threads {
                    thread["comments"]["nodes"][0]["pullRequestReview"]["state"] =
                        response["state"].clone();
                }
                if state.lose_response {
                    (
                        500,
                        json!({"message":"response lost after accepted write","documentation_url":"https://docs.github.com"}),
                    )
                } else {
                    (200, response)
                }
            }
            _ => panic!("Unexpected POST {}", r.path),
        }
    })
}

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_submit_final_body_and_comments_once_and_reconcile_lost_response() {
    for lose_response in [false, true] {
        for (reviews, _, _temp) in common::databases().await {
            let state = Arc::new(Mutex::new(Remote {
                lose_response,
                ..Default::default()
            }));
            let fixture = fixture(state.clone());
            let forge = ForgeService::with_provider(
                reviews.clone(),
                GithubClient::for_test(&fixture.endpoint).unwrap(),
            );
            let actor = reviews
                .auth()
                .ensure_agent("Assistant", "test-model", "test-provider")
                .await
                .unwrap();
            let context = reviews
                .ensure_context(
                    ContextKey::ForgeBacked {
                        repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                            .unwrap(),
                        number: 1,
                    },
                    None,
                )
                .await
                .unwrap();
            let context = forge.refresh(&context.id).await.unwrap();
            let session = reviews
                .start_session(
                    &context,
                    &actor,
                    CommitContext {
                        base: Some("base".into()),
                        head: Some("head".into()),
                    },
                    crate::common::comparison(&context),
                    None,
                )
                .await
                .unwrap();
            let (_, metadata) = forge.get_workspace(&context.id, "", true).await.unwrap();
            assert_eq!(metadata.len(), 1);
            assert_eq!(metadata[0].0.previous_path, "file.txt");
            assert!(metadata[0].0.content.is_empty());
            assert!(metadata[0].1.is_empty());
            assert_eq!(state.lock().unwrap().content_reads, 0);
            let (_, missing) = forge
                .get_workspace(&context.id, "other.txt", false)
                .await
                .unwrap();
            assert!(missing.is_empty());
            assert_eq!(state.lock().unwrap().content_reads, 0);
            let (root, files) = forge.get_workspace(&context.id, "", false).await.unwrap();
            assert_eq!(root, "https://github.com/acme/repo");
            assert_eq!(files[0].0.content, "new\n");
            assert_eq!(files[0].1, "old\n");
            let comment = reviews
                .add_comment(
                    &session.id,
                    &actor,
                    "Check assumptions",
                    Anchor::General,
                    None,
                    None,
                )
                .await
                .unwrap();
            let submitted = forge
                .submit(
                    &session.id,
                    &actor,
                    "approve",
                    "Final review body",
                    "retry-key",
                    "head",
                )
                .await
                .unwrap();
            assert_eq!(
                submitted.submission_status,
                if lose_response {
                    "uncertain"
                } else {
                    "confirmed"
                }
            );
            assert_eq!(
                submitted.completion().unwrap().body.as_deref(),
                Some("Final review body")
            );
            assert_eq!(
                reviews
                    .get_comment(&context.id, &comment.id)
                    .await
                    .unwrap()
                    .forge_comment_id
                    .as_deref(),
                Some("10")
            );
            let repeated = forge
                .submit(
                    &session.id,
                    &actor,
                    "approve",
                    "Final review body",
                    "retry-key",
                    "head",
                )
                .await
                .unwrap();
            assert_eq!(repeated.submission_status, "confirmed");
            assert_eq!(repeated.forge_review_id.as_deref(), Some("5"));
            assert_eq!(
                state.lock().unwrap().writes,
                3,
                "retry must not replay accepted writes"
            );
            assert!(
                state.lock().unwrap().comments[0]["body"]
                    .as_str()
                    .unwrap()
                    .contains("test-provider/test-model")
            );
            assert!(
                forge
                    .submit(
                        &session.id,
                        &actor,
                        "approve",
                        "Different body",
                        "retry-key",
                        "head"
                    )
                    .await
                    .is_err()
            );
        }
    }
}
#[tokio::test]
async fn should_reject_stale_head_before_freezing_or_writing() {
    for (reviews, _, _temp) in common::databases().await {
        let state = Arc::new(Mutex::new(Remote::default()));
        let fixture = fixture(state.clone());
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&fixture.endpoint).unwrap(),
        );
        let actor = reviews
            .auth()
            .ensure_human("Reviewer", "reviewer@example.com")
            .await
            .unwrap();
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let context = forge.refresh(&context.id).await.unwrap();
        let session = reviews
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
                crate::common::comparison(&context),
                None,
            )
            .await
            .unwrap();
        state.lock().unwrap().head_changed = true;
        assert!(
            forge
                .submit(&session.id, &actor, "comment", "body", "key", "head")
                .await
                .is_err()
        );
        assert_eq!(
            reviews.get_session(&session.id).await.unwrap().state(),
            "active"
        );
        assert_eq!(state.lock().unwrap().writes, 0);
    }
}

#[tokio::test]
async fn should_roll_back_every_staged_comment_when_a_later_payload_changed() {
    use lgtm_backend::models::Completion;
    for (reviews, _, _temp) in common::databases().await {
        let fixture = fixture(Arc::new(Mutex::new(Remote::default())));
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&fixture.endpoint).unwrap(),
        );
        let actor = reviews
            .auth()
            .ensure_human("Reviewer", "reviewer@example.com")
            .await
            .unwrap();
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let context = forge.refresh(&context.id).await.unwrap();
        let session = reviews
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
                crate::common::comparison(&context),
                None,
            )
            .await
            .unwrap();
        let first = reviews
            .add_comment(
                &session.id,
                &actor,
                "First draft",
                Anchor::General,
                None,
                None,
            )
            .await
            .unwrap();
        let second = reviews
            .add_comment(
                &session.id,
                &actor,
                "Second draft",
                Anchor::General,
                None,
                None,
            )
            .await
            .unwrap();
        reviews
            .edit_comment(
                &session.id,
                &second.id,
                &actor,
                "Changed second draft",
                None,
            )
            .await
            .unwrap();
        let result = reviews
            .prepare_submission(
                &session.id,
                &actor,
                Completion {
                    outcome: Some("comment".into()),
                    body: Some("Final".into()),
                    submit: true,
                },
                "rollback",
                &[
                    (first.clone(), first.body.clone(), "first-key".into()),
                    (second.clone(), second.body.clone(), "second-key".into()),
                ],
                None,
            )
            .await;
        assert!(result.is_err());
        assert_eq!(
            reviews.get_session(&session.id).await.unwrap().state(),
            "active"
        );
        let first = reviews.get_comment(&context.id, &first.id).await.unwrap();
        assert_eq!(first.posting.state().as_str(), "unposted");
        assert_eq!(first.visibility, "draft");
        assert!(first.posting_request_key.is_none());
    }
}

#[tokio::test]
async fn should_finalize_only_verified_pending_review_and_never_replay_missing_comment() {
    for (failure, missing) in [(403, false), (503, false), (503, true)] {
        for (reviews, _, _temp) in common::databases().await {
            let state = Arc::new(Mutex::new(Remote {
                event_failure: Some(failure),
                missing_comment: missing,
                ..Default::default()
            }));
            let fixture = fixture(state.clone());
            let forge = ForgeService::with_provider(
                reviews.clone(),
                GithubClient::for_test(&fixture.endpoint).unwrap(),
            );
            let actor = reviews
                .auth()
                .ensure_human("Reviewer", "reviewer@example.com")
                .await
                .unwrap();
            let context = reviews
                .ensure_context(
                    ContextKey::ForgeBacked {
                        repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                            .unwrap(),
                        number: 1,
                    },
                    None,
                )
                .await
                .unwrap();
            let context = forge.refresh(&context.id).await.unwrap();
            let session = reviews
                .start_session(
                    &context,
                    &actor,
                    CommitContext {
                        base: Some("base".into()),
                        head: Some("head".into()),
                    },
                    crate::common::comparison(&context),
                    None,
                )
                .await
                .unwrap();
            reviews
                .add_comment(
                    &session.id,
                    &actor,
                    "Frozen draft",
                    Anchor::General,
                    None,
                    None,
                )
                .await
                .unwrap();
            let failed = forge
                .submit(
                    &session.id,
                    &actor,
                    "comment",
                    "Final body",
                    "recover",
                    "head",
                )
                .await
                .unwrap();
            assert_eq!(
                failed.submission_status,
                if failure == 403 {
                    "unsubmitted"
                } else {
                    "uncertain"
                }
            );
            assert_eq!(
                state.lock().unwrap().review.as_ref().unwrap()["state"],
                "PENDING"
            );
            let writes = state.lock().unwrap().writes;
            let recovered = forge.reconcile(&session.id, &actor).await;
            if missing {
                let error = recovered.unwrap_err().to_string();
                assert!(error.contains("no writes retried"));
                assert_eq!(state.lock().unwrap().writes, writes);
                assert_eq!(
                    reviews
                        .get_session(&session.id)
                        .await
                        .unwrap()
                        .submission_status,
                    "uncertain"
                );
            } else if failure == 403 {
                assert_eq!(recovered.unwrap().submission_status, "unsubmitted");
                assert_eq!(
                    state.lock().unwrap().writes,
                    writes,
                    "a rejection is not an uncertain write to replay"
                );
                let resumed = forge
                    .submit(
                        &session.id,
                        &actor,
                        "comment",
                        "Changed final body",
                        "new-submit",
                        "head",
                    )
                    .await
                    .unwrap();
                assert_eq!(resumed.submission_status, "confirmed");
                assert_eq!(resumed.forge_review_id.as_deref(), Some("5"));
            } else {
                assert_eq!(recovered.unwrap().submission_status, "confirmed");
                assert_eq!(
                    state.lock().unwrap().writes,
                    writes + 1,
                    "only final event on existing review is retried"
                );
                assert_eq!(state.lock().unwrap().comments.len(), 1);
            }
        }
    }
}

mod pending_drafts {
    use super::*;

    // tests -------------------------------------------------------------------
    #[tokio::test]
    async fn should_sync_edit_refresh_and_submit_the_same_pending_review_after_restart() {
        for lose_draft_response in [false, true] {
            for (reviews, _, _temp) in common::databases().await {
                let state = Arc::new(Mutex::new(Remote {
                    lose_draft_response,
                    ..Default::default()
                }));
                let fixture = fixture(state.clone());
                let forge = ForgeService::with_provider(
                    reviews.clone(),
                    GithubClient::for_test(&fixture.endpoint).unwrap(),
                );
                let actor = reviews
                    .auth()
                    .ensure_human("Reviewer", "reviewer@example.test")
                    .await
                    .unwrap();
                let context = reviews
                    .ensure_context(
                        ContextKey::ForgeBacked {
                            repository_url: ForgeRepositoryUrl::parse(
                                "https://github.com/acme/repo",
                            )
                            .unwrap(),
                            number: 1,
                        },
                        None,
                    )
                    .await
                    .unwrap();
                let context = forge.refresh(&context.id).await.unwrap();
                let session = reviews
                    .start_session(
                        &context,
                        &actor,
                        CommitContext {
                            base: Some("base".into()),
                            head: Some("head".into()),
                        },
                        crate::common::comparison(&context),
                        None,
                    )
                    .await
                    .unwrap();
                let comment = reviews
                    .add_comment(
                        &session.id,
                        &actor,
                        "Draft feedback",
                        Anchor::File {
                            path: "file.txt".into(),
                        },
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                let result = forge.sync_draft(&session.id, &comment.id, &actor).await;
                assert_eq!(result.is_err(), lose_draft_response);
                assert_eq!(
                    state.lock().unwrap().review.as_ref().unwrap()["state"],
                    "PENDING"
                );
                assert_eq!(state.lock().unwrap().writes, 2);
                // A fresh service has no in-memory receipt or lock; recover only from durable/provider evidence.
                let forge = ForgeService::with_provider(
                    reviews.clone(),
                    GithubClient::for_test(&fixture.endpoint).unwrap(),
                );
                let draft = forge
                    .sync_draft(&session.id, &comment.id, &actor)
                    .await
                    .unwrap();
                assert_eq!(draft.visibility, "draft");
                assert_eq!(draft.forge_comment_id.as_deref(), Some("10"));
                assert_eq!(
                    state.lock().unwrap().writes,
                    2,
                    "recovery must not duplicate a pending review or draft"
                );
                forge
                    .edit_comment(&session.id, &comment.id, &actor, "Edited draft feedback")
                    .await
                    .unwrap();
                forge.refresh(&context.id).await.unwrap();
                let conversation = reviews.get_conversation(&context.id, true).await.unwrap();
                assert_eq!(
                    conversation.comments.len(),
                    1,
                    "refresh must not import the pending review marker or duplicate the local comment"
                );
                assert_eq!(conversation.comments[0].body, "Edited draft feedback");
                assert_eq!(conversation.comments[0].visibility, "draft");
                let outcome = if lose_draft_response {
                    "request_changes"
                } else {
                    "comment"
                };
                let submitted = forge
                    .submit(
                        &session.id,
                        &actor,
                        outcome,
                        "Final body",
                        "submit-key",
                        "head",
                    )
                    .await
                    .unwrap();
                assert_eq!(submitted.forge_review_id.as_deref(), Some("5"));
                assert_eq!(submitted.submission_status, "confirmed");
                assert_eq!(
                    submitted.completion().unwrap().outcome,
                    if lose_draft_response {
                        "reject"
                    } else {
                        "comment"
                    }
                );
                assert_eq!(
                    state.lock().unwrap().writes,
                    4,
                    "create review + draft + edit + submit; no repost"
                );
                assert_eq!(
                    state.lock().unwrap().review.as_ref().unwrap()["body"],
                    format!("Final body\n\n<!-- lgtm-review:{} -->", session.id)
                );
            }
        }
    }
}

#[tokio::test]
async fn should_keep_original_anchors_and_refresh_current_provider_locations_independently() {
    for (reviews, _, _temp) in common::databases().await {
        let original = "a".repeat(40);
        let state = Arc::new(Mutex::new(Remote::default()));
        state.lock().unwrap().threads.push(json!({
            "id":"T_observed","path":"file.txt","subjectType":"LINE","diffSide":"RIGHT",
            "line":8,"startLine":7,"originalLine":3,"originalStartLine":2,"isOutdated":false,"isResolved":false,
            "comments":{"nodes":[{"id":"C_observed","databaseId":77,"body":"Keep this feedback","updatedAt":"2026-10-19T00:00:00Z","author":{"databaseId":1,"login":"user"},"originalCommit":{"oid":original},"commit":{"oid":"head"},"pullRequestReview":{"databaseId":5,"state":"COMMENTED"}}],"pageInfo":{"hasNextPage":false}}
        }));
        let server = fixture(state.clone());
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&server.endpoint).unwrap(),
        );
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        forge.refresh(&context.id).await.unwrap();
        let first = reviews.get_conversation(&context.id, true).await.unwrap();
        let thread = &first.threads[0];
        assert_eq!(
            thread.anchor,
            Anchor::Line {
                path: "file.txt".into(),
                start: 2,
                end: 3,
                side: "new".into(),
            }
        );
        assert_eq!(
            thread.forge_observation.as_ref().unwrap().current,
            Some(Anchor::Line {
                path: "file.txt".into(),
                start: 7,
                end: 8,
                side: "new".into(),
            })
        );
        assert!(!thread.forge_observation.as_ref().unwrap().outdated);
        {
            let mut remote = state.lock().unwrap();
            remote.threads[0]["isOutdated"] = json!(true);
            remote.threads[0]["isResolved"] = json!(true);
            remote.threads[0]["line"] = Value::Null;
            remote.threads[0]["startLine"] = Value::Null;
        }
        forge.refresh(&context.id).await.unwrap();
        let refreshed = reviews.get_conversation(&context.id, true).await.unwrap();
        let current = &refreshed.threads[0];
        assert_eq!(current.id, thread.id);
        assert_eq!(current.anchor, thread.anchor);
        assert_eq!(current.state, "resolved");
        assert!(current.forge_observation.as_ref().unwrap().outdated);
        assert_eq!(
            current.forge_observation.as_ref().unwrap().current,
            Some(Anchor::File {
                path: "file.txt".into()
            }),
            "a known file may remain current, but original lines must not become current coordinates"
        );
        assert_eq!(refreshed.comments[0].id, first.comments[0].id);
    }
}

#[tokio::test]
async fn should_follow_a_changed_base_but_reject_closed_or_merged_publication_without_losing_drafts()
 {
    for (reviews, _, _temp) in common::databases().await {
        let state = Arc::new(Mutex::new(Remote::default()));
        let server = fixture(state.clone());
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&server.endpoint).unwrap(),
        );
        let actor = reviews
            .auth()
            .ensure_human("Reviewer", "reviewer@example.test")
            .await
            .unwrap();
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let context = forge.refresh(&context.id).await.unwrap();
        let session = reviews
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
                ComparisonKind::PullRequest,
                None,
            )
            .await
            .unwrap();
        let saved = reviews
            .add_comment(
                &session.id,
                &actor,
                "Saved draft",
                Anchor::File {
                    path: "file.txt".into(),
                },
                None,
                None,
            )
            .await
            .unwrap();
        state.lock().unwrap().base_changed = true;
        let posted = forge
            .sync_draft(&session.id, &saved.id, &actor)
            .await
            .unwrap();
        assert_eq!(posted.posting.state().as_str(), "confirmed");
        let followed = reviews.get_session(&session.id).await.unwrap();
        assert_eq!(followed.id, session.id);
        assert_eq!(followed.commits.base.as_deref(), Some("new-base"));
        assert!(followed.forge_review_id.is_some());
        for (base_changed, lifecycle) in [(true, "closed"), (true, "merged")] {
            {
                let mut remote = state.lock().unwrap();
                remote.base_changed = base_changed;
                remote.lifecycle = lifecycle;
            }
            let writes = state.lock().unwrap().writes;
            assert!(
                forge
                    .sync_draft(&session.id, &saved.id, &actor)
                    .await
                    .is_err()
            );
            assert!(
                forge
                    .submit(
                        &session.id,
                        &actor,
                        "comment",
                        "Final feedback",
                        lifecycle,
                        "head"
                    )
                    .await
                    .is_err()
            );
            assert_eq!(state.lock().unwrap().writes, writes);
            assert_eq!(
                reviews
                    .get_comment(&context.id, &saved.id)
                    .await
                    .unwrap()
                    .body,
                "Saved draft"
            );
            assert_eq!(
                reviews.get_session(&session.id).await.unwrap().state(),
                "active"
            );
        }
        {
            let mut remote = state.lock().unwrap();
            remote.base_changed = false;
            remote.lifecycle = "open";
        }
        forge.refresh(&context.id).await.unwrap();
        assert_eq!(
            forge
                .sync_draft(&session.id, &saved.id, &actor)
                .await
                .unwrap()
                .posting
                .state()
                .as_str(),
            "confirmed"
        );
    }
}

#[tokio::test]
async fn should_recover_remote_edits_resolution_and_deletion_after_response_loss_without_replaying_writes()
 {
    use lgtm_backend::models::{ForgeChangeInput, ForgeChangeObject, MutationGuard};
    for (reviews, url, _temp) in common::databases().await {
        let state = Arc::new(Mutex::new(Remote::default()));
        let server = fixture(state.clone());
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&server.endpoint).unwrap(),
        );
        let actor = reviews
            .auth()
            .ensure_human("Reviewer", "reviewer@example.test")
            .await
            .unwrap();
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let context = forge.refresh(&context.id).await.unwrap();
        let session = reviews
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
                ComparisonKind::PullRequest,
                None,
            )
            .await
            .unwrap();
        let draft = reviews
            .add_comment(
                &session.id,
                &actor,
                "Original draft",
                Anchor::File {
                    path: "file.txt".into(),
                },
                None,
                None,
            )
            .await
            .unwrap();
        forge
            .sync_draft(&session.id, &draft.id, &actor)
            .await
            .unwrap();
        {
            let mut remote = state.lock().unwrap();
            remote.review.as_mut().unwrap()["state"] = json!("COMMENTED");
            remote.review.as_mut().unwrap()["body"] = json!("");
            for thread in &mut remote.threads {
                for comment in thread["comments"]["nodes"].as_array_mut().unwrap() {
                    comment["pullRequestReview"]["state"] = json!("COMMENTED");
                }
            }
        }
        forge.refresh(&context.id).await.unwrap();
        for (key, input) in [
            (
                "edit",
                ForgeChangeInput::Edit {
                    comment_id: draft.id,
                    body: "Accepted edit".into(),
                },
            ),
            (
                "resolve",
                ForgeChangeInput::Resolve {
                    thread_id: draft.thread_id,
                    resolved: true,
                },
            ),
            (
                "delete",
                ForgeChangeInput::Delete {
                    comment_id: draft.id,
                },
            ),
        ] {
            let current = reviews.get_context_by_id(&context.id).await.unwrap();
            let guard = MutationGuard::new(Some(current.revision), key.into());
            state.lock().unwrap().lose_mutation_response = true;
            assert!(
                forge
                    .apply_change(&session.id, &actor, input.clone(), Some(&guard))
                    .await
                    .is_err()
            );
            let writes = state.lock().unwrap().writes;
            let receipt = reviews
                .database()
                .mutations
                .get_result(&context.id, &actor.id, key)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&receipt.result_json).unwrap(),
                json!({}),
                "uncertain PR writes retain identity and a payload hash, never a second comment body"
            );
            assert!(
                reviews
                    .database()
                    .mutations
                    .get_pending(&context.id, Some(&input.object().1))
                    .await
                    .unwrap()
            );
            assert!(
                forge
                    .submit(
                        &session.id,
                        &actor,
                        "comment",
                        "Final body",
                        "while-pending",
                        "head"
                    )
                    .await
                    .is_err()
            );
            assert_eq!(state.lock().unwrap().writes, writes);
            {
                let mut remote = state.lock().unwrap();
                remote.lose_mutation_response = false;
                remote.snapshot_unavailable = false;
            }
            // A fresh service instance recovers only the original saved action.
            let reopened = common::service(&url).await.unwrap();
            let restarted = ForgeService::with_provider(
                reopened.clone(),
                GithubClient::for_test(&server.endpoint).unwrap(),
            );
            let result = restarted
                .apply_change(&session.id, &actor, input.clone(), Some(&guard))
                .await
                .unwrap();
            match (key, result) {
                ("edit", ForgeChangeObject::Comment(comment)) => {
                    assert_eq!(comment.body, "Accepted edit")
                }
                ("resolve", ForgeChangeObject::Thread(thread)) => {
                    assert_eq!(thread.state, "resolved")
                }
                ("delete", ForgeChangeObject::Deleted { comment_id }) => {
                    assert_eq!(comment_id, draft.id);
                    assert!(
                        reopened
                            .get_conversation(&context.id, true)
                            .await
                            .unwrap()
                            .comments
                            .is_empty()
                    );
                    assert!(
                        reopened
                            .get_comment(&context.id, &comment_id)
                            .await
                            .is_err()
                    );
                }
                _ => panic!("unexpected remote result"),
            }
            assert!(
                !reviews
                    .database()
                    .mutations
                    .get_pending(&context.id, Some(&input.object().1))
                    .await
                    .unwrap()
            );
            restarted
                .apply_change(&session.id, &actor, input, Some(&guard))
                .await
                .unwrap();
            assert_eq!(
                state.lock().unwrap().writes,
                writes,
                "response loss and accepted retries must never repeat the mutation"
            );
            reopened.database().close().await;
        }
    }
}

#[tokio::test]
async fn should_keep_pr_identity_and_pending_review_across_pushes_but_freeze_completed_comparisons()
{
    for (reviews, _, _temp) in common::databases().await {
        let remote = Arc::new(Mutex::new(Remote::default()));
        let server = fixture(remote.clone());
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&server.endpoint).unwrap(),
        );
        let actor = reviews
            .auth()
            .ensure_human("Reviewer", "continuity@example.test")
            .await
            .unwrap();
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let context = forge.refresh(&context.id).await.unwrap();
        let session = reviews
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
                ComparisonKind::PullRequest,
                None,
            )
            .await
            .unwrap();
        let comment = reviews
            .add_comment(
                &session.id,
                &actor,
                "Keep this pending feedback",
                Anchor::File {
                    path: "file.txt".into(),
                },
                None,
                None,
            )
            .await
            .unwrap();
        let saved = forge
            .sync_draft(&session.id, &comment.id, &actor)
            .await
            .unwrap();
        let pending_id = reviews
            .get_session(&session.id)
            .await
            .unwrap()
            .forge_review_id;
        remote.lock().unwrap().head_changed = true;
        let writes = remote.lock().unwrap().writes;
        forge.refresh(&context.id).await.unwrap();
        let current = reviews.get_session(&session.id).await.unwrap();
        assert_eq!(current.id, session.id);
        assert_eq!(current.commits.head.as_deref(), Some("changed"));
        assert_eq!(current.forge_review_id, pending_id);
        assert_eq!(current.state(), "active");
        assert_eq!(remote.lock().unwrap().writes, writes);
        let same = forge
            .sync_draft(&session.id, &comment.id, &actor)
            .await
            .unwrap();
        assert_eq!(same.forge_comment_id, saved.forge_comment_id);
        assert_eq!(same.body, saved.body);
        assert_eq!(
            remote.lock().unwrap().writes,
            writes,
            "confirmed drafts are never reposted"
        );
        let completed = forge
            .submit(
                &session.id,
                &actor,
                "comment",
                "Final review",
                "complete-current",
                "changed",
            )
            .await
            .unwrap();
        assert_eq!(completed.submission_status, "confirmed");
        assert_eq!(completed.forge_review_id, pending_id);
        remote.lock().unwrap().head_changed = false;
        forge.refresh(&context.id).await.unwrap();
        let retained = reviews.get_session(&session.id).await.unwrap();
        assert_eq!(
            retained.commits, completed.commits,
            "completed comparison is historical evidence"
        );
        assert_eq!(retained.forge_review_id, pending_id);
    }
}

#[tokio::test]
async fn should_use_targeted_actions_and_never_restore_confirmed_remote_deletions() {
    use lgtm_backend::models::{ForgeChangeInput, ForgeChangeObject, MutationGuard};
    for (reviews, url, _temp) in common::databases().await {
        let remote = Arc::new(Mutex::new(Remote::default()));
        let server = fixture(remote.clone());
        let forge = ForgeService::with_provider(
            reviews.clone(),
            GithubClient::for_test(&server.endpoint).unwrap(),
        );
        let actor = reviews
            .auth()
            .ensure_human("Reviewer", "reviewer@example.test")
            .await
            .unwrap();
        let context = reviews
            .ensure_context(
                ContextKey::ForgeBacked {
                    repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                        .unwrap(),
                    number: 1,
                },
                None,
            )
            .await
            .unwrap();
        let context = forge.refresh(&context.id).await.unwrap();
        let session = reviews
            .start_session(
                &context,
                &actor,
                CommitContext {
                    base: Some("base".into()),
                    head: Some("head".into()),
                },
                ComparisonKind::PullRequest,
                None,
            )
            .await
            .unwrap();
        let draft = reviews
            .add_comment(
                &session.id,
                &actor,
                "Delete this remote feedback",
                Anchor::File {
                    path: "file.txt".into(),
                },
                None,
                None,
            )
            .await
            .unwrap();
        forge
            .sync_draft(&session.id, &draft.id, &actor)
            .await
            .unwrap();
        let delete = ForgeChangeInput::Delete {
            comment_id: draft.id,
        };

        // Another account cannot see private drafts; their absence must not erase cached feedback.
        let private_threads = {
            let mut state = remote.lock().unwrap();
            state.account_switched = true;
            std::mem::take(&mut state.threads)
        };
        assert!(forge.refresh(&context.id).await.is_err());
        assert!(reviews.get_comment(&context.id, &draft.id).await.is_ok());
        {
            let mut state = remote.lock().unwrap();
            state.account_switched = false;
            state.threads = private_threads;
            let mut reply = state.threads[0]["comments"]["nodes"][0].clone();
            reply["id"] = json!("C_11");
            reply["databaseId"] = json!(11);
            reply["body"] = json!("Surviving feedback");
            state.threads[0]["comments"]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(reply);
        }
        forge.refresh(&context.id).await.unwrap();

        // An account switch and an explicit permission rejection preserve the comment and release the intent.
        remote.lock().unwrap().other_viewer = true;
        assert!(
            forge
                .apply_change(
                    &session.id,
                    &actor,
                    delete.clone(),
                    Some(&MutationGuard::new(None, "wrong-viewer".into()))
                )
                .await
                .is_err()
        );
        assert!(
            !reviews
                .database()
                .mutations
                .get_pending(&context.id, Some(&draft.id))
                .await
                .unwrap()
        );
        assert!(reviews.get_comment(&context.id, &draft.id).await.is_ok());
        {
            let mut state = remote.lock().unwrap();
            state.other_viewer = false;
            state.deny_mutation = true;
        }
        assert!(
            forge
                .apply_change(
                    &session.id,
                    &actor,
                    delete.clone(),
                    Some(&MutationGuard::new(None, "denied-delete".into()))
                )
                .await
                .is_err()
        );
        assert!(
            !reviews
                .database()
                .mutations
                .get_pending(&context.id, Some(&draft.id))
                .await
                .unwrap()
        );
        assert!(reviews.get_comment(&context.id, &draft.id).await.is_ok());
        {
            let mut state = remote.lock().unwrap();
            state.deny_mutation = false;
            state.requests.clear();
        }

        // Resolution applies only after the provider publishes the thread.
        {
            let mut state = remote.lock().unwrap();
            state.review.as_mut().unwrap()["state"] = json!("COMMENTED");
            state.review.as_mut().unwrap()["body"] = json!("");
            for thread in &mut state.threads {
                for comment in thread["comments"]["nodes"].as_array_mut().unwrap() {
                    comment["pullRequestReview"]["state"] = json!("COMMENTED");
                }
            }
        }
        forge.refresh(&context.id).await.unwrap();
        remote.lock().unwrap().requests.clear();
        // Resolution is acknowledged by the mutation itself: no /user or whole-PR scans.
        forge
            .apply_change(
                &session.id,
                &actor,
                ForgeChangeInput::Resolve {
                    thread_id: draft.thread_id,
                    resolved: true,
                },
                Some(&MutationGuard::new(None, "resolve-fast".into())),
            )
            .await
            .unwrap();
        {
            let state = remote.lock().unwrap();
            assert_eq!(state.requests.len(), 1, "{:?}", state.requests);
            assert!(state.requests[0].contains("resolveReviewThread("));
        }
        let stale_threads = {
            let mut state = remote.lock().unwrap();
            state.requests.clear();
            state.threads.clone()
        };
        let guard = MutationGuard::new(None, "delete-fast".into());
        assert!(
            matches!(forge.apply_change(&session.id, &actor, delete.clone(), Some(&guard)).await.unwrap(), ForgeChangeObject::Deleted {comment_id} if comment_id == draft.id)
        );
        {
            let state = remote.lock().unwrap();
            assert_eq!(state.requests.len(), 2, "{:?}", state.requests);
            assert!(state.requests[0].contains("node(id:"));
            assert!(state.requests[1].contains("deletePullRequestReviewComment("));
        }
        assert!(reviews.get_comment(&context.id, &draft.id).await.is_err());
        let conversation = reviews.get_conversation(&context.id, true).await.unwrap();
        assert_eq!(conversation.comments.len(), 1);
        assert_eq!(conversation.comments[0].body, "Surviving feedback");
        assert_eq!(conversation.threads[0].id, draft.thread_id);
        // Only an identity receipt survives, never a deleted body or display placeholder.
        let receipt = reviews
            .database()
            .mutations
            .get_remote_delete_receipts(&context.id)
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.request_key == "delete-fast")
            .unwrap();
        assert!(!receipt.result_json.contains("Delete this remote feedback"));
        let requests = remote.lock().unwrap().requests.len();
        let reopened = common::service(&url).await.unwrap();
        let restarted = ForgeService::with_provider(
            reopened.clone(),
            GithubClient::for_test(&server.endpoint).unwrap(),
        );
        restarted
            .apply_change(&session.id, &actor, delete, Some(&guard))
            .await
            .unwrap();
        assert_eq!(
            remote.lock().unwrap().requests.len(),
            requests,
            "retry after restart uses the receipt without a network request"
        );
        remote.lock().unwrap().threads = stale_threads;
        restarted.refresh(&context.id).await.unwrap();
        let conversation = reopened.get_conversation(&context.id, true).await.unwrap();
        assert_eq!(
            conversation.comments.len(),
            1,
            "a stale provider page cannot resurrect a confirmed deletion"
        );
        assert_eq!(conversation.comments[0].body, "Surviving feedback");
        reopened.database().close().await;
    }
}

#[tokio::test]
async fn should_keep_self_review_validation_and_rejected_submissions_editable_without_duplicate_publication()
 {
    use lgtm_backend::models::Completion;
    for interrupted in [false, true] {
        for (reviews, url, _temp) in common::databases().await {
            let remote = Arc::new(Mutex::new(Remote {
                author_is_viewer: true,
                ..Default::default()
            }));
            let server = fixture(remote.clone());
            let forge = ForgeService::with_provider(
                reviews.clone(),
                GithubClient::for_test(&server.endpoint).unwrap(),
            );
            let actor = reviews
                .auth()
                .ensure_human("Reviewer", "self@example.test")
                .await
                .unwrap();
            let context = reviews
                .ensure_context(
                    ContextKey::ForgeBacked {
                        repository_url: ForgeRepositoryUrl::parse("https://github.com/acme/repo")
                            .unwrap(),
                        number: 1,
                    },
                    None,
                )
                .await
                .unwrap();
            let context = forge.refresh(&context.id).await.unwrap();
            let session = reviews
                .start_session(
                    &context,
                    &actor,
                    CommitContext {
                        base: Some("base".into()),
                        head: Some("head".into()),
                    },
                    ComparisonKind::PullRequest,
                    None,
                )
                .await
                .unwrap();
            let comment = reviews
                .add_comment(
                    &session.id,
                    &actor,
                    "Keep my pending feedback",
                    Anchor::File {
                        path: "file.txt".into(),
                    },
                    None,
                    None,
                )
                .await
                .unwrap();
            forge
                .sync_draft(&session.id, &comment.id, &actor)
                .await
                .unwrap();
            let writes = remote.lock().unwrap().writes;
            for outcome in ["approve", "request_changes"] {
                let error = forge
                    .submit(
                        &session.id,
                        &actor,
                        outcome,
                        "Keep final text",
                        outcome,
                        "head",
                    )
                    .await
                    .unwrap_err();
                assert!(error.to_string().contains("own PR"));
                assert_eq!(
                    reviews.get_session(&session.id).await.unwrap().state(),
                    "active"
                );
                assert_eq!(
                    remote.lock().unwrap().writes,
                    writes,
                    "self-review validation precedes freezing and all provider writes"
                );
            }
            if interrupted {
                // Recover a frozen attempt after restart, with a definite provider rejection.
                reviews
                    .prepare_submission(
                        &session.id,
                        &actor,
                        Completion {
                            outcome: Some("reject".into()),
                            body: Some("Keep final text".into()),
                            submit: true,
                        },
                        "rejected",
                        &[],
                        None,
                    )
                    .await
                    .unwrap();
                assert!(
                    reviews
                        .database()
                        .sessions
                        .update_submission(
                            &session.id,
                            "sending",
                            "failed",
                            Some("GitHub HTTP 422 Unprocessable Entity: request rejected"),
                            Some("5")
                        )
                        .await
                        .unwrap()
                );
                let recovered = forge.reconcile(&session.id, &actor).await.unwrap();
                assert_eq!(recovered.state(), "active");
                assert_eq!(
                    remote.lock().unwrap().writes,
                    writes,
                    "recovering a known rejection must not retry its event"
                );
            } else {
                remote.lock().unwrap().author_is_viewer = false;
                remote.lock().unwrap().event_failure = Some(422);
                let rejected = forge
                    .submit(
                        &session.id,
                        &actor,
                        "request_changes",
                        "Keep final text",
                        "rejected",
                        "head",
                    )
                    .await
                    .unwrap();
                assert_eq!(rejected.state(), "active");
                assert_eq!(rejected.submission_status, "unsubmitted");
                assert!(rejected.submission_error.unwrap().contains("422"));
            }
            let reopened = common::service(&url).await.unwrap();
            let forge = ForgeService::with_provider(
                reopened.clone(),
                GithubClient::for_test(&server.endpoint).unwrap(),
            );
            let current = reopened.get_session(&session.id).await.unwrap();
            assert_eq!(current.forge_review_id.as_deref(), Some("5"));
            assert_eq!(
                current.draft_body.as_deref(),
                Some("Keep final text"),
                "final text survives backend restart"
            );
            assert_eq!(
                reopened
                    .get_comment(&context.id, &comment.id)
                    .await
                    .unwrap()
                    .visibility,
                "draft"
            );
            let writes = remote.lock().unwrap().writes;
            assert!(
                forge
                    .submit(
                        &session.id,
                        &actor,
                        "request_changes",
                        "Keep final text",
                        "rejected",
                        "head"
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                remote.lock().unwrap().writes,
                writes,
                "a delayed rejected retry cannot publish"
            );
            // Both persistence entry points must recheck the receipt under their context lock.
            // An earlier API read can race recovery; it cannot freeze this rejected key again.
            let old_input = Completion {
                outcome: Some("reject".into()),
                body: Some("Keep final text".into()),
                submit: true,
            };
            assert!(
                reopened
                    .prepare_submission(
                        &session.id,
                        &actor,
                        old_input.clone(),
                        "rejected",
                        &[],
                        None
                    )
                    .await
                    .is_err()
            );
            assert!(
                reopened
                    .complete(
                        &context.id,
                        &actor,
                        old_input,
                        "rejected",
                        Some(&session.id),
                        None
                    )
                    .await
                    .is_err()
            );
            assert_eq!(
                reopened.get_session(&session.id).await.unwrap().state(),
                "active"
            );
            assert_eq!(
                forge
                    .reconcile(&session.id, &actor)
                    .await
                    .unwrap()
                    .submission_status,
                "unsubmitted"
            );
            assert_eq!(
                remote.lock().unwrap().writes,
                writes,
                "repeated recovery is read-only"
            );
            let published = forge
                .submit(
                    &session.id,
                    &actor,
                    "comment",
                    "Keep final text",
                    "corrected-outcome",
                    "head",
                )
                .await
                .unwrap();
            assert_eq!(published.submission_status, "confirmed");
            assert_eq!(published.forge_review_id, current.forge_review_id);
            assert_eq!(
                remote.lock().unwrap().writes,
                writes + 1,
                "reuse the same pending review and comments; send only its final event"
            );
            reopened.database().close().await;
        }
    }
}
