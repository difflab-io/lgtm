use connectrpc::{
    Protocol, Server,
    client::{CallOptions, ClientConfig, HttpClient},
};
use lgtm_backend::proto::lgtm::v1::{GreeterServiceClient, SayHelloRequest};
use std::time::Duration;

#[tokio::test]
async fn hello_works_over_grpc_and_grpc_web() {
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
    for protocol in [Protocol::Grpc, Protocol::GrpcWeb] {
        let config =
            ClientConfig::new(format!("http://{address}").parse().unwrap()).with_protocol(protocol);
        let transport = if protocol == Protocol::Grpc {
            HttpClient::plaintext_http2_only()
        } else {
            HttpClient::plaintext()
        };
        let client = GreeterServiceClient::new(transport, config);
        for (name, expected) in [
            ("Rust", "Hello, Rust!"),
            ("", "Hello, World!"),
            ("  Zoë  ", "Hello, Zoë!"),
        ] {
            let reply = client
                .say_hello_with_options(
                    SayHelloRequest {
                        name: name.into(),
                        ..Default::default()
                    },
                    CallOptions::default().with_timeout(Duration::from_secs(5)),
                )
                .await
                .unwrap();
            assert_eq!(reply.into_owned().message, expected);
        }
    }
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
