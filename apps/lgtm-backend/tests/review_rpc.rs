//! Public typed contracts exercised over a real native gRPC server.
use connectrpc::{
    ErrorCode, Protocol, Server,
    client::{CallOptions, ClientConfig, HttpClient},
};
use lgtm_backend::{database::Database, proto::lgtm::v1::*};
use std::time::Duration;

// tests -----------------------------------------------------------------------
#[tokio::test]
async fn should_validate_requests_and_preserve_local_conversation_through_rpc() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::connect("sqlite::memory:").await.unwrap();
    let api = std::sync::Arc::new(lgtm_backend::rpc::Api::with_snapshots(
        database.clone(),
        lgtm_backend::services::SnapshotService::new(temp.path().join("snapshots")),
    ));
    let router = connectrpc::Router::new()
        .add_service::<lgtm_backend::rpc::Api, IdentityServiceRegisterMarker>(api.clone())
        .add_service::<lgtm_backend::rpc::Api, ReviewServiceRegisterMarker>(api.clone())
        .add_service::<lgtm_backend::rpc::Api, DiffServiceRegisterMarker>(api);
    let bound = Server::bind("127.0.0.1:0").await.unwrap();
    let address = bound.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        bound
            .serve_with_graceful_shutdown(router, async {
                stopped.await.ok();
            })
            .await
    });
    let config = ClientConfig::new(format!("http://{address}").parse().unwrap())
        .with_protocol(Protocol::Grpc);
    let identities = IdentityServiceClient::new(HttpClient::plaintext_http2_only(), config.clone());
    let reviews = ReviewServiceClient::new(HttpClient::plaintext_http2_only(), config);
    let capabilities = identities
        .get_capabilities(GetCapabilitiesRequest::default())
        .await
        .unwrap()
        .into_owned();
    assert_eq!(capabilities.api_version, "1");
    for capability in [
        "local_review",
        "export",
        "review_targets",
        "feedback_scopes",
        "review_snapshots",
        "diff_service",
    ] {
        assert!(
            capabilities
                .capabilities
                .iter()
                .any(|value| value == capability)
        );
    }
    let who = identities
        .ensure_identity(EnsureIdentityRequest {
            name: "Alice".into(),
            email: "alice@example.com".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .actor;
    let options = || {
        CallOptions::default()
            .with_timeout(Duration::from_secs(5))
            .with_header("x-lgtm-identity", who.id.clone())
    };
    let start = StartReviewRequest {
        target: Some(ReviewTarget {
            kind: Some(review_target::Kind::Directory(Box::new(DirectoryTarget {
                path: temp.path().to_string_lossy().into_owned(),
                ..Default::default()
            }))),
            ..Default::default()
        })
        .into(),
        request_key: "native-start".into(),
        ..Default::default()
    };
    assert!(
        reviews.start_review(start.clone()).await.is_err(),
        "a target does not bypass requester identity"
    );
    assert_eq!(
        reviews
            .start_review_with_options(
                start.clone(),
                CallOptions::default().with_header("x-lgtm-identity", "not-a-uuid")
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        reviews
            .start_review_with_options(
                start.clone(),
                CallOptions::default()
                    .with_header("x-lgtm-identity", "00000000-0000-0000-0000-000000000001")
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    assert_eq!(
        reviews
            .start_review_with_options(
                StartReviewRequest {
                    request_key: "missing-target".into(),
                    ..Default::default()
                },
                options()
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        reviews
            .get_review(GetReviewRequest {
                review_id: "00000000-0000-0000-0000-000000000002".into(),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let review = reviews
        .start_review_with_options(start, options())
        .await
        .unwrap()
        .into_owned()
        .review;
    let scope = ReviewScope {
        kind: Some(review_scope::Kind::ReviewId(review.id.clone())),
        ..Default::default()
    };
    let read = || GetConversationRequest {
        scope: Some(scope.clone()).into(),
        include_drafts: Some(true),
        ..Default::default()
    };
    // Old clients must receive an actionable error without saving a partial comment.
    for kind in ["file", "line"] {
        for snapshot_id in ["", "not-a-uuid"] {
            let error = reviews
                .add_comment_with_options(
                    AddCommentRequest {
                        scope: Some(scope.clone()).into(),
                        anchor: Some(Anchor {
                            kind: kind.into(),
                            snapshot_id: snapshot_id.into(),
                            path: "review.txt".into(),
                            side: if kind == "line" {
                                "new".into()
                            } else {
                                String::new()
                            },
                            start: if kind == "line" { 1 } else { 0 },
                            end: if kind == "line" { 1 } else { 0 },
                            ..Default::default()
                        })
                        .into(),
                        body: "Keep my unsent feedback".into(),
                        expected_version: review.version.clone(),
                        request_key: format!("invalid-snapshot-{kind}-{snapshot_id}"),
                        ..Default::default()
                    },
                    options(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidArgument);
            assert!(error.to_string().contains("anchor.snapshotId"), "{error}");
            if snapshot_id.is_empty() {
                assert!(error.to_string().contains("restart your client"), "{error}");
            }
        }
    }
    assert!(
        reviews
            .get_conversation(read())
            .await
            .unwrap()
            .into_owned()
            .comments
            .is_empty()
    );

    let first = reviews
        .add_comment_with_options(
            AddCommentRequest {
                scope: Some(scope.clone()).into(),
                body: "A general note".into(),
                expected_version: review.version.clone(),
                request_key: "root".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap()
        .into_owned()
        .comment;
    let after_root = reviews.get_conversation(read()).await.unwrap().into_owned();
    let reply = reviews
        .reply_with_options(
            ReplyRequest {
                scope: Some(scope.clone()).into(),
                thread_id: first.thread_id.clone(),
                body: "A reply".into(),
                expected_version: after_root.threads[0].version.clone(),
                request_key: "reply".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap()
        .into_owned()
        .comment;
    // A stale root token cannot bypass an intervening accepted reply. The rejected write is atomic.
    let conflict = reviews
        .delete_comment_with_options(
            DeleteCommentRequest {
                scope: Some(scope.clone()).into(),
                comment_id: first.id.clone(),
                expected_version: first.version.clone(),
                request_key: "stale-delete".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(conflict.code, ErrorCode::FailedPrecondition);
    let result = reviews
        .export(ExportRequest {
            scope: Some(scope.clone()).into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert!(result.markdown.contains("A general note") && result.markdown.contains("A reply"));
    assert!(result.markdown.contains("1. **[COMMENT]**") && !result.markdown.contains(&review.id));
    let current = reviews.get_conversation(read()).await.unwrap().into_owned();
    let reply_version = current
        .comments
        .iter()
        .find(|comment| comment.id == reply.id)
        .unwrap()
        .version
        .clone();
    reviews
        .delete_comment_with_options(
            DeleteCommentRequest {
                scope: Some(scope.clone()).into(),
                comment_id: reply.id.clone(),
                expected_version: reply_version,
                request_key: "delete-reply".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap();
    let current = reviews.get_conversation(read()).await.unwrap().into_owned();
    reviews
        .resolve_thread_with_options(
            ResolveThreadRequest {
                scope: Some(scope.clone()).into(),
                thread_id: first.thread_id.clone(),
                resolved: true,
                expected_version: current.threads[0].version.clone(),
                request_key: "resolve".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap();
    let conversation = reviews.get_conversation(read()).await.unwrap().into_owned();
    assert_eq!(conversation.comments.len(), 1);
    assert_eq!(conversation.threads[0].state, "resolved");
    let completion = CompleteReviewRequest {
        review_id: review.id.clone(),
        expected_version: conversation.version.clone(),
        request_key: "complete-once".into(),
        ..Default::default()
    };
    let done = reviews
        .complete_review_with_options(completion.clone(), options())
        .await
        .unwrap()
        .into_owned()
        .review;
    assert_eq!(done.state, "completed");
    let changed_retry = reviews
        .complete_review_with_options(
            CompleteReviewRequest {
                body: "Changed retry payload".into(),
                ..completion
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(changed_retry.code, ErrorCode::FailedPrecondition);
    let immutable = reviews
        .add_comment_with_options(
            AddCommentRequest {
                scope: Some(scope).into(),
                body: "Too late".into(),
                expected_version: done.version.clone(),
                request_key: "completed-write".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(immutable.code, ErrorCode::FailedPrecondition);
    let unchanged = reviews
        .get_conversation(GetConversationRequest {
            scope: Some(ReviewScope {
                kind: Some(review_scope::Kind::ReviewId(review.id.clone())),
                ..Default::default()
            })
            .into(),
            include_drafts: Some(true),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(unchanged.comments.len(), 1);
    assert_eq!(unchanged.comments[0].body, "A general note");
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
    database.close().await;
}
