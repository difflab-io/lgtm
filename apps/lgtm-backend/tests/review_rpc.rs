//! Public typed API contracts exercised over a real native gRPC server.
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
    let (router, _) = lgtm_backend::router_with_database(database.clone());
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
    for capability in ["notes", "local_review", "export"] {
        assert!(
            capabilities
                .capabilities
                .iter()
                .any(|value| value == capability)
        );
    }
    let invalid_context = reviews
        .ensure_context(EnsureContextRequest {
            key: Some(ContextKey {
                kind: "notes".into(),
                path: temp.path().to_str().unwrap().into(),
                repository_url: "https://github.com/acme/repo".into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(invalid_context.code, ErrorCode::InvalidArgument);
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
    let context = reviews
        .ensure_context(EnsureContextRequest {
            key: Some(ContextKey {
                kind: "notes".into(),
                path: temp.path().to_str().unwrap().into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .context;
    assert!(
        reviews
            .start_session(StartSessionRequest {
                context_id: context.id.clone(),
                ..Default::default()
            })
            .await
            .is_err()
    );
    let invalid_identity = reviews
        .start_session_with_options(
            StartSessionRequest {
                context_id: context.id.clone(),
                ..Default::default()
            },
            CallOptions::default()
                .with_timeout(Duration::from_secs(5))
                .with_header("x-lgtm-identity", "not-a-uuid"),
        )
        .await
        .unwrap_err();
    assert_eq!(invalid_identity.code, ErrorCode::InvalidArgument);
    let unknown_identity = reviews
        .start_session_with_options(
            StartSessionRequest {
                context_id: context.id.clone(),
                ..Default::default()
            },
            CallOptions::default()
                .with_timeout(Duration::from_secs(5))
                .with_header("x-lgtm-identity", "00000000-0000-0000-0000-000000000001"),
        )
        .await
        .unwrap_err();
    assert_eq!(unknown_identity.code, ErrorCode::NotFound);
    let unknown_context = reviews
        .start_session_with_options(
            StartSessionRequest {
                context_id: "00000000-0000-0000-0000-000000000002".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(unknown_context.code, ErrorCode::NotFound);
    let session = reviews
        .start_session_with_options(
            StartSessionRequest {
                context_id: context.id.clone(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap()
        .into_owned()
        .session;
    let first = reviews
        .add_comment_with_options(
            AddCommentRequest {
                session_id: session.id.clone(),
                body: "A general note".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap()
        .into_owned()
        .comment;
    let reply = reviews
        .reply_with_options(
            ReplyRequest {
                session_id: session.id.clone(),
                thread_id: first.thread_id.clone(),
                body: "A reply".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap()
        .into_owned()
        .comment;
    // A root cannot be removed while its reply remains; the failed write is atomic.
    let conflict = reviews
        .delete_comment_with_options(
            DeleteCommentRequest {
                session_id: session.id.clone(),
                comment_id: first.id.clone(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(conflict.code, ErrorCode::FailedPrecondition);
    let result = reviews
        .export(ExportRequest {
            session_id: session.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert!(result.markdown.contains("A general note"));
    assert!(result.markdown.contains("A reply"));
    reviews
        .delete_comment_with_options(
            DeleteCommentRequest {
                session_id: session.id.clone(),
                comment_id: reply.id.clone(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap();
    reviews
        .resolve_thread_with_options(
            ResolveThreadRequest {
                session_id: session.id.clone(),
                thread_id: first.thread_id.clone(),
                resolved: true,
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap();
    let conversation = reviews
        .get_conversation(GetConversationRequest {
            context_id: context.id.clone(),
            include_drafts: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(conversation.comments.len(), 1);
    assert_eq!(conversation.threads[0].state, "resolved");
    let done = reviews
        .complete_session_with_options(
            CompleteSessionRequest {
                context_id: context.id.clone(),
                request_key: "complete-once".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap()
        .into_owned()
        .session;
    assert_eq!(done.state, "completed");
    let changed_retry = reviews
        .complete_session_with_options(
            CompleteSessionRequest {
                context_id: context.id.clone(),
                request_key: "complete-once".into(),
                body: "Changed retry payload".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(changed_retry.code, ErrorCode::FailedPrecondition);
    let immutable_session = reviews
        .add_comment_with_options(
            AddCommentRequest {
                session_id: session.id.clone(),
                body: "Too late".into(),
                ..Default::default()
            },
            options(),
        )
        .await
        .unwrap_err();
    assert_eq!(immutable_session.code, ErrorCode::FailedPrecondition);
    let unchanged = reviews
        .get_conversation(GetConversationRequest {
            context_id: context.id.clone(),
            include_drafts: true,
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
