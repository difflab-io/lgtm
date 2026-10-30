//! Public hello-review contract through generated clients.
use connectrpc::{
    Protocol, Server,
    client::{ClientConfig, HttpClient},
};
use lgtm_backend::proto::lgtm::v1::{HelloReviewRequest, ReviewServiceClient};

// tests -----------------------------------------------------------------------

#[tokio::test]
async fn should_return_normalized_review_greeting_over_connect_and_grpc() {
    // Arrange
    let (router, _) = lgtm_backend::router();
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
    for protocol in [Protocol::Connect, Protocol::Grpc] {
        let config =
            ClientConfig::new(format!("http://{address}").parse().unwrap()).with_protocol(protocol);
        let transport = match protocol {
            Protocol::Grpc => HttpClient::plaintext_http2_only(),
            _ => HttpClient::plaintext(),
        };
        let client = ReviewServiceClient::new(transport, config);
        for (name, expected) in [
            ("Rust", "Hello, Rust!"),
            ("", "Hello, World!"),
            ("  Zoë  ", "Hello, Zoë!"),
        ] {
            // Act
            let response = client
                .hello_review(HelloReviewRequest {
                    name: name.into(),
                    ..Default::default()
                })
                .await
                .unwrap();
            // Assert
            assert_eq!(response.into_owned().message, expected);
        }
    }
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}
