//! Desktop review mode service tracking whether the workspace diff viewer operates in `Local` or `ForgeBacked` mode.

use std::sync::RwLock;

use crate::models::ReviewMode;

/// Manages the active [`ReviewMode`] (`Local` vs. `ForgeBacked`) for the desktop diff viewer.
#[derive(Default)]
pub struct ReviewService {
    mode: RwLock<ReviewMode>,
}

impl ReviewService {
    /// Creates a review service initialized to `initial_mode` (or [`ReviewMode::Local`] when `None`).
    pub fn new(initial_mode: Option<ReviewMode>) -> Self {
        Self {
            mode: RwLock::new(initial_mode.unwrap_or_default()),
        }
    }

    // Public ------------------------------------------------------------------

    /// Returns the currently active [`ReviewMode`].
    pub fn get_mode(&self) -> ReviewMode {
        self.mode.read().map(|guard| *guard).unwrap_or_default()
    }

    /// Updates the active [`ReviewMode`] and returns the newly applied mode.
    pub fn set_mode(&self, mode: ReviewMode) -> ReviewMode {
        if let Ok(mut guard) = self.mode.write() {
            *guard = mode;
        }
        mode
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_a_review_service {
        use super::*;

        mod when_initializing_and_switching_modes {
            use super::*;

            #[test]
            fn should_default_to_local_and_persist_updated_review_mode() {
                // Arrange
                let service = ReviewService::new(None);

                // Act
                let initial = service.get_mode();
                let updated = service.set_mode(ReviewMode::ForgeBacked);

                // Assert
                assert_eq!(initial, ReviewMode::Local);
                assert_eq!(updated, ReviewMode::ForgeBacked);
                assert_eq!(service.get_mode(), ReviewMode::ForgeBacked);
            }
        }
    }
}
