//! Dummy data source for the hello-review skeleton; no files or connections are opened.

/// Stateless placeholder for the future persistence boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct Database;

impl Database {
    // Public ------------------------------------------------------------------

    /// Return the greeting shared by the demonstration clients.
    pub fn get_hello(&self) -> String {
        "Hello".into()
    }
}
