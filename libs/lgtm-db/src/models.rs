//! SeaORM row models returned and accepted by repository methods.
//! Fields can be edited in memory. Persist changes through the repository write API.

/// SeaORM persisted model for identities.
pub mod identities {
    pub use super::super::entities::identities::Model;
}

/// SeaORM persisted model for repositories.
pub mod repositories {
    pub use super::super::entities::repositories::Model;
}

/// SeaORM persisted model for workspace paths.
pub mod workspace_paths {
    pub use super::super::entities::workspace_paths::Model;
}

/// SeaORM persisted model for review contexts.
pub mod review_contexts {
    pub use super::super::entities::review_contexts::Model;
}

/// SeaORM persisted model for sessions.
pub mod sessions {
    pub use super::super::entities::sessions::Model;
}

/// SeaORM persisted model for threads.
pub mod threads {
    pub use super::super::entities::threads::Model;
}

/// SeaORM persisted model for comments.
pub mod comments {
    pub use super::super::entities::comments::Model;
}
