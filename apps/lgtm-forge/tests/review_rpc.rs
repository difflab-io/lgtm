//! Integration tests for `ReviewService` over Connect and gRPC transports.

use std::{process::Command, sync::Arc};

use connectrpc::{
    Protocol, Server,
    client::{ClientConfig, HttpClient},
};
use lgtm_forge::{
    database::Database,
    forge::GitHubForge,
    proto::lgtm::v1::{CreateReviewRequest, ListReviewsRequest, ReviewServiceClient},
    snapshots::SnapshotStore,
};

// fixtures --------------------------------------------------------------------

fn init_temp_git_repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    for args in [
        &["init", "-q", "-b", "feature/rpc-test"][..],
        &["config", "user.name", "Ada Lovelace"],
        &["config", "user.email", "ada@example.com"],
    ] {
        let status = Command::new("git")
            .current_dir(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    std::fs::write(
        repo.join("src/lib.rs"),
        "pub fn answer() -> i32 {\n    42\n}\n",
    )
    .unwrap();
    for args in [&["add", "."][..], &["commit", "-qm", "Initial commit"]] {
        let status = Command::new("git")
            .current_dir(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    temp
}

// tests -----------------------------------------------------------------------

mod given_running_review_service_router {
    use super::*;

    mod when_calling_review_service_over_connect_and_grpc {
        use super::*;

        #[tokio::test]
        async fn should_create_and_list_local_reviews_over_connect_and_grpc() {
            // Arrange
            let repo = init_temp_git_repo();
            let snapshots_root = tempfile::tempdir().unwrap();
            let git_dir = repo.path().display().to_string();
            let db = Database::connect("sqlite::memory:").await.unwrap();
            let (router, _) = lgtm_forge::router_with_dependencies(
                db,
                SnapshotStore::new(snapshots_root.path().to_path_buf()),
                Arc::new(GitHubForge::new()),
            );
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

            // Act & Assert
            let connect_client = ReviewServiceClient::new(
                HttpClient::plaintext(),
                ClientConfig::new(format!("http://{address}").parse().unwrap())
                    .with_protocol(Protocol::Connect),
            );
            let created = connect_client
                .create_review(CreateReviewRequest {
                    git_dir: git_dir.clone(),
                    local: true,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_owned();
            assert_eq!(created.review.as_option().unwrap().number, 1);

            let grpc_client = ReviewServiceClient::new(
                HttpClient::plaintext_http2_only(),
                ClientConfig::new(format!("http://{address}").parse().unwrap())
                    .with_protocol(Protocol::Grpc),
            );
            let listed = grpc_client
                .list_reviews(ListReviewsRequest {
                    git_dir,
                    local: true,
                    all: false,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_owned();
            assert_eq!(listed.reviews.len(), 1);
            assert_eq!(listed.reviews[0].name, "R1-feature-rpc-test");

            stop.send(()).unwrap();
            server.await.unwrap().unwrap();
        }
    }
}
