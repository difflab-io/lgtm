//! Domain service contract with the dummy database package.
use lgtm_backend::{database::Database, services::ReviewService};

// tests -----------------------------------------------------------------------

#[test]
fn should_build_review_from_dummy_database_greeting() {
    let reviews = ReviewService::new(Database);
    assert_eq!(reviews.get_hello("Reviewer").message, "Hello, Reviewer!");
    assert_eq!(reviews.get_hello("  ").message, "Hello, World!");
}
