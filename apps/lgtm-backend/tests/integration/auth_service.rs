use crate::support;
use connectrpc::{
    Encodable, Protocol, RequestContext, Response, Router, Server, ServiceRequest, ServiceResult,
    client::{CallOptions, ClientConfig, HttpClient},
};
use lgtm_backend::database::Database;
use lgtm_backend::{
    error::Error,
    proto::lgtm::v1::{GreeterService, GreeterServiceClient, SayHelloRequest, SayHelloResponse},
    services::AuthService,
};
use std::{sync::Arc, time::Duration};

// fixtures --------------------------------------------------------------------
/// Test endpoint: production greeting/health routes keep their existing contract.
struct RequesterEndpoint(AuthService);

impl GreeterService for RequesterEndpoint {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "requester"))]
    async fn say_hello<'a>(
        &'a self,
        context: RequestContext,
        _request: ServiceRequest<'_, SayHelloRequest>,
    ) -> ServiceResult<impl Encodable<SayHelloResponse> + Send + use<'a>> {
        let requester = self
            .0
            .requester(&context)
            .await
            .map_err(|error| connectrpc::ConnectError::invalid_argument(error.to_string()))?;
        Response::ok(SayHelloResponse {
            message: requester.label(),
            ..Default::default()
        })
    }
}

// tests -----------------------------------------------------------------------

#[tokio::test]
async fn should_reject_missing_unknown_and_ambiguous_requester_metadata() {
    for (review, _, _) in support::databases().await {
        let auth = review.auth();
        let actor = auth
            .ensure_human("Alice", "alice@example.com")
            .await
            .unwrap();
        let mut request = RequestContext::new(Default::default());
        assert!(matches!(
            auth.requester(&request).await,
            Err(Error::Invalid(_))
        ));
        for value in ["", " ", "missing-identity"] {
            request
                .headers_mut()
                .insert("x-lgtm-identity", value.parse().unwrap());
            assert!(auth.requester(&request).await.is_err());
        }
        request
            .headers_mut()
            .insert("x-lgtm-identity", actor.id.to_string().parse().unwrap());
        assert_eq!(auth.requester(&request).await.unwrap(), actor);
        request
            .headers_mut()
            .append("x-lgtm-identity", actor.id.to_string().parse().unwrap());
        assert!(matches!(
            auth.requester(&request).await,
            Err(Error::Invalid(_))
        ));
        review.database().close().await;
    }
}

#[tokio::test]
async fn should_resolve_each_requests_supplied_identity_over_grpc_and_grpc_web() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    let auth = AuthService::new(database.clone());
    let alice = auth
        .ensure_human("Alice", "alice@example.com")
        .await
        .unwrap();
    let agent = auth
        .ensure_agent("Agent", "model-one", "test-provider")
        .await
        .unwrap();
    let router = Router::new().add_service(Arc::new(RequesterEndpoint(auth)));
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
    for protocol in [Protocol::Grpc, Protocol::GrpcWeb] {
        let config =
            ClientConfig::new(format!("http://{address}").parse().unwrap()).with_protocol(protocol);
        let transport = if protocol == Protocol::Grpc {
            HttpClient::plaintext_http2_only()
        } else {
            HttpClient::plaintext()
        };
        let client = GreeterServiceClient::new(transport, config);
        for actor in [&alice, &agent, &alice] {
            let options = CallOptions::default()
                .with_timeout(Duration::from_secs(5))
                .with_header("x-lgtm-identity", actor.id.to_string());
            let response = client
                .say_hello_with_options(SayHelloRequest::default(), options)
                .await
                .unwrap();
            assert_eq!(response.into_owned().message, actor.label());
        }
        assert!(
            client
                .say_hello_with_options(
                    SayHelloRequest::default(),
                    CallOptions::default().with_timeout(Duration::from_secs(5))
                )
                .await
                .is_err()
        );
    }
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    database.close().await;
}
