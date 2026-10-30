use crate::{database::Database, models::Review};

/// Read the demonstration review through the database boundary.
#[derive(Clone)]
pub struct ReviewService {
    database: Database,
}

impl ReviewService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    // Public ------------------------------------------------------------------

    pub fn get_hello(&self, name: &str) -> Review {
        Review::new(&self.database.get_hello(), name)
    }
}
