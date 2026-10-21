//! Loopback-only HTTP fixture exercising the real Octocrab transport.
use serde_json::Value;
use std::{net::TcpListener, sync::Arc, thread::JoinHandle};

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Value,
    pub headers: String,
}
pub struct Fixture {
    pub endpoint: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}
impl Fixture {
    pub fn new(handler: impl Fn(Request) -> (u16, Value) + Send + Sync + 'static) -> Self {
        Self::with_headers(move |request| {
            let (status, body) = handler(request);
            (status, body, vec![])
        })
    }

    pub fn with_headers(
        handler: impl Fn(Request) -> (u16, Value, Vec<(&'static str, String)>) + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let handler = Arc::new(handler);
        let worker = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    let app = axum::Router::new().fallback(
                        move |request: axum::http::Request<axum::body::Body>| {
                            let handler = handler.clone();
                            async move {
                                let (parts, body) = request.into_parts();
                                let bytes =
                                    axum::body::to_bytes(body, 4 * 1024 * 1024).await.unwrap();
                                let body = if bytes.is_empty() {
                                    Value::Null
                                } else {
                                    serde_json::from_slice(&bytes).unwrap()
                                };
                                let (status, body, headers) = handler(Request {
                                    method: parts.method.to_string(),
                                    path: parts.uri.to_string(),
                                    body,
                                    headers: format!("{:?}", parts.headers),
                                });
                                let mut response = axum::http::Response::builder()
                                    .status(status)
                                    .header("content-type", "application/json");
                                for (name, value) in headers {
                                    response = response.header(name, value);
                                }
                                response
                                    .body(axum::body::Body::from(if status == 204 {
                                        String::new()
                                    } else {
                                        body.to_string()
                                    }))
                                    .unwrap()
                            }
                        },
                    );
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = stopped.await;
                        })
                        .await
                        .unwrap();
                });
        });
        Self {
            endpoint,
            stop: Some(stop),
            worker: Some(worker),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
