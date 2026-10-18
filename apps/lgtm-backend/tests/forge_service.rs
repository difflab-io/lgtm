//! Full submission orchestration with real databases and a dummy HTTP forge.
mod common;
use common::github_http::{Fixture, Request};
use lgtm_backend::{
    forge::Github,
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
    writes: usize,
    lose_response: bool,
    head_changed: bool,
    event_failure: Option<u16>,
    missing_comment: bool,
}
fn fixture(state: Arc<Mutex<Remote>>) -> Fixture {
    Fixture::new(move |r: Request| {
        let mut state = state.lock().unwrap();
        let path = r.path.split('?').next().unwrap();
        if r.method == "GET" {
            return match path {
                "/user" => (200, json!({"id":1})),
                "/repos/acme/repo/pulls/1" => (
                    200,
                    json!({"number":1,"title":"PR","head":{"sha":if state.head_changed{"changed"}else{"head"}},"base":{"sha":"base"}}),
                ),
                "/repos/acme/repo/pulls/1/files" => {
                    (200, json!([{"filename":"file.txt","status":"modified"}]))
                }
                "/repos/acme/repo/contents/file.txt" => (
                    200,
                    json!({"encoding":"base64","size":4,"content":if r.path.contains("ref=head") {"bmV3Cg=="} else {"b2xkCg=="}}),
                ),
                "/repos/acme/repo/pulls/1/reviews" => {
                    (200, json!(state.review.iter().collect::<Vec<_>>()))
                }
                "/repos/acme/repo/issues/1/comments" => (200, json!(state.comments)),
                "/repos/acme/repo/pulls/1/comments"
                | "/repos/acme/repo/pulls/1/reviews/5/comments" => (200, json!([])),
                _ => panic!("Unexpected GET {}", r.path),
            };
        }
        if path == "/graphql" {
            assert!(r.body["query"].as_str().unwrap().contains("reviewThreads"));
            return (
                200,
                json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}}),
            );
        }
        state.writes += 1;
        match path {
            "/repos/acme/repo/pulls/1/reviews" => {
                assert!(r.body.get("event").is_none());
                let review = json!({"id":5,"node_id":"PRR_fixture","state":"PENDING","body":r.body["body"],"commit_id":r.body["commit_id"],"user":{"id":1}});
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
                Github::for_test(&fixture.endpoint).unwrap(),
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
                )
                .await
                .unwrap();
            let (root, files) = forge.get_workspace(&context.id).await.unwrap();
            assert_eq!(root, "https://github.com/acme/repo");
            assert_eq!(files[0].0.content, "new\n");
            assert_eq!(files[0].1, "old\n");
            let comment = reviews
                .add_comment(&session.id, &actor, "Check assumptions", Anchor::General)
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
            Github::for_test(&fixture.endpoint).unwrap(),
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
            Github::for_test(&fixture.endpoint).unwrap(),
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
            )
            .await
            .unwrap();
        let first = reviews
            .add_comment(&session.id, &actor, "First draft", Anchor::General)
            .await
            .unwrap();
        let second = reviews
            .add_comment(&session.id, &actor, "Second draft", Anchor::General)
            .await
            .unwrap();
        reviews
            .edit_comment(&session.id, &second.id, &actor, "Changed second draft")
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
                Github::for_test(&fixture.endpoint).unwrap(),
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
                )
                .await
                .unwrap();
            reviews
                .add_comment(&session.id, &actor, "Frozen draft", Anchor::General)
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
                    "failed"
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
