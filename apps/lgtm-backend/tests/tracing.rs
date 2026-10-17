use connectrpc::RequestContext;
use lgtm_backend::{
    database::Database,
    models::{CommitContext, ContextKey},
    services::{AuthService, ReviewService},
};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
use tracing::{Instrument, instrument::WithSubscriber};

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn shared_services_inherit_each_callers_span_across_awaits() {
    let database = Database::connect("sqlite::memory:").await.unwrap();
    let auth = AuthService::new(database.clone());
    let actor = auth
        .ensure_human("Alice", "alice@example.com")
        .await
        .unwrap();
    let review = ReviewService::new(database.clone(), auth.clone());
    let first = review
        .ensure_context(
            ContextKey::Notes {
                path: "/first".into(),
            },
            None,
        )
        .await
        .unwrap();
    let second = review
        .ensure_context(
            ContextKey::Notes {
                path: "/second".into(),
            },
            None,
        )
        .await
        .unwrap();
    let mut request = RequestContext::new(Default::default());
    request
        .headers_mut()
        .insert("x-lgtm-identity", actor.id.to_string().parse().unwrap());

    let output = Capture(Arc::new(Mutex::new(Vec::new())));
    let writer = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_writer(move || writer.clone())
        .finish();

    async {
        let call = |context| {
            let auth = &auth;
            let review = &review;
            let request = &request;
            async move {
                tokio::task::yield_now().await;
                let actor = auth.requester(request).await.unwrap();
                tokio::task::yield_now().await;
                review
                    .start_session(context, &actor, CommitContext::default())
                    .await
                    .unwrap();
            }
        };
        tokio::join!(
            call(&first).instrument(tracing::info_span!("request", request_id = "first")),
            call(&second).instrument(tracing::info_span!("request", request_id = "second")),
        );
    }
    .with_subscriber(subscriber)
    .await;

    let logs = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
    for message in [
        "Resolving requester identity",
        "Review action: start_session",
    ] {
        let events: Vec<_> = logs.lines().filter(|line| line.contains(message)).collect();
        assert_eq!(events.len(), 2, "{logs}");
        for request in ["first", "second"] {
            let event = events
                .iter()
                .find(|line| line.contains(&format!("request_id=\"{request}\"")))
                .unwrap_or_else(|| panic!("Missing {request} span on {message}: {logs}"));
            let other = if request == "first" {
                "second"
            } else {
                "first"
            };
            assert!(
                !event.contains(&format!("request_id=\"{other}\"")),
                "{event}"
            );
        }
    }
    database.close().await;
}
