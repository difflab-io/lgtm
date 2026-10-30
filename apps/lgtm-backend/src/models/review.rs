use crate::proto::lgtm::v1::HelloReviewResponse;

/// Demonstration review data; it does not identify a persisted review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub message: String,
}

impl Review {
    /// Create a greeting for a normalized display name.
    pub fn new(greeting: &str, name: &str) -> Self {
        let name = name.trim();
        let name = if name.is_empty() { "World" } else { name };
        Self {
            message: format!("{greeting}, {name}!"),
        }
    }
}

impl From<Review> for HelloReviewResponse {
    fn from(review: Review) -> Self {
        Self {
            message: review.message,
            ..Default::default()
        }
    }
}
