//! Integration tests for health service.
mod health_service {
    use connectrpc::{
        Protocol, Server,
        client::{CallOptions, ClientConfig, HttpClient},
    };
    use connectrpc_health::{
        HealthClient,
        wire::{HealthCheckRequest, ServingStatus},
    };
    use std::time::Duration;

    // tests ------------------------------------------------------------------

    #[tokio::test]
    async fn should_report_health_over_connect_and_grpc() {
        let (router, health) = lgtm_forge::router();
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
            let config = ClientConfig::new(format!("http://{address}").parse().unwrap())
                .with_protocol(protocol);
            let transport = match protocol {
                Protocol::Grpc => HttpClient::plaintext_http2_only(),
                _ => HttpClient::plaintext(),
            };
            let client = HealthClient::new(transport, config);
            let options = CallOptions::default().with_timeout(Duration::from_secs(5));
            let response = client
                .check_with_options(HealthCheckRequest::default(), options.clone())
                .await
                .unwrap();
            assert_eq!(response.into_owned().status, ServingStatus::Serving);
            let error = client
                .check_with_options(
                    HealthCheckRequest {
                        service: "unknown".into(),
                        ..Default::default()
                    },
                    options.clone(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, connectrpc::ErrorCode::NotFound);
            health.shutdown();
            let response = client
                .check_with_options(HealthCheckRequest::default(), options)
                .await
                .unwrap();
            assert_eq!(response.into_owned().status, ServingStatus::NotServing);
            health
                .set_status("", connectrpc_health::Status::Serving)
                .unwrap();
        }
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
