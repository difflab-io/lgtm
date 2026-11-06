use super::{Push, Snapshot};
use crate::{
    entities::*,
    error::{Result, invalid},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, Set, TransactionTrait, sea_query::OnConflict,
};
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};
use uuid::Uuid;

/// Sync operations share the caller's transaction with review writes when needed.
#[derive(Clone)]
pub struct SyncRepository<C> {
    connection: Arc<C>,
}

impl<C: ConnectionTrait + TransactionTrait> SyncRepository<C> {
    pub(crate) fn new(connection: Arc<C>) -> Self {
        Self { connection }
    }

    // Public ------------------------------------------------------------------

    /// Permanently bind this POC data directory to its first authenticated account.
    pub async fn bind_account(&self, owner_id: Uuid) -> Result<()> {
        sync_account::Entity::insert(sync_account::ActiveModel {
            id: Set(1),
            owner_id: Set(owner_id),
            cursor: Set(0),
        })
        .on_conflict(
            OnConflict::column(sync_account::Column::Id)
                .do_nothing()
                .to_owned(),
        )
        .try_insert()
        .exec(self.connection.as_ref())
        .await?;
        if sync_account::Entity::find_by_id(1)
            .one(self.connection.as_ref())
            .await?
            .unwrap()
            .owner_id
            != owner_id
        {
            return Err(invalid(
                "Local database belongs to another sync account; use a separate LGTM_HOME",
            ));
        }
        Ok(())
    }

    pub async fn get_cursor(&self) -> Result<i64> {
        Ok(sync_account::Entity::find_by_id(1)
            .one(self.connection.as_ref())
            .await?
            .ok_or_else(|| invalid("Sync account is not bound"))?
            .cursor)
    }

    pub async fn advance_cursor(&self, cursor: i64) -> Result<()> {
        sync_account::Entity::update(sync_account::ActiveModel {
            id: Set(1),
            cursor: Set(cursor),
            ..Default::default()
        })
        .exec(self.connection.as_ref())
        .await?;
        Ok(())
    }

    pub async fn get_context_ids(&self) -> Result<Vec<Uuid>> {
        Ok(review_contexts::Entity::find()
            .order_by_asc(review_contexts::Column::Id)
            .all(self.connection.as_ref())
            .await?
            .into_iter()
            .map(|r| r.id)
            .collect())
    }

    /// Get a consistent snapshot inside a caller-owned transaction.
    pub async fn get_snapshot(&self, id: Uuid) -> Result<Snapshot> {
        let conn = self.connection.as_ref();
        let context = review_contexts::Entity::find_by_id(id)
            .one(conn)
            .await?
            .ok_or_else(|| invalid("Context does not exist"))?;
        let sessions = sessions::Entity::find()
            .filter(sessions::Column::ContextId.eq(id))
            .order_by_asc(sessions::Column::Id)
            .all(conn)
            .await?;
        let threads = threads::Entity::find()
            .filter(threads::Column::ContextId.eq(id))
            .order_by_asc(threads::Column::Id)
            .all(conn)
            .await?;
        let comments = comments::Entity::find()
            .filter(comments::Column::ContextId.eq(id))
            .order_by_asc(comments::Column::Id)
            .all(conn)
            .await?;
        let file_reviews = file_reviews::Entity::find()
            .filter(file_reviews::Column::ContextId.eq(id))
            .order_by_asc(file_reviews::Column::ActorId)
            .order_by_asc(file_reviews::Column::Path)
            .all(conn)
            .await?;
        let mut ids = BTreeSet::new();
        for row in &sessions {
            ids.insert(row.creator_id);
            ids.extend(row.completed_by_id);
        }
        for row in &threads {
            ids.insert(row.author_id);
            ids.extend(row.resolved_by_id);
            ids.extend(row.updated_by_id);
        }
        for row in &comments {
            ids.insert(row.author_id);
            ids.extend(row.owner_id);
        }
        for row in &file_reviews {
            ids.insert(row.actor_id);
        }
        let identities = identities::Entity::find()
            .filter(identities::Column::Id.is_in(ids))
            .order_by_asc(identities::Column::Id)
            .all(conn)
            .await?;
        let mut snapshot = Snapshot {
            context,
            identities,
            sessions,
            threads,
            comments,
            file_reviews,
        };
        snapshot.remove_device_paths();
        Ok(snapshot)
    }

    /// Persist the exact upload before HTTP. Existing pending uploads are retried verbatim.
    pub async fn enqueue(&self, snapshot: Snapshot) -> Result<Option<Push>> {
        let id = snapshot.context.id;
        let state = self.get_state(id).await?;
        if let Some(pending) = state.as_ref().and_then(|r| r.pending.clone()) {
            return Ok(Some(
                serde_json::from_value(pending).map_err(|e| invalid(e.to_string()))?,
            ));
        }
        let value = serde_json::to_value(&snapshot).map_err(|e| invalid(e.to_string()))?;
        if state.as_ref().and_then(|r| r.baseline.as_ref()) == Some(&value) {
            return Ok(None);
        }
        let push = Push {
            operation_id: Uuid::new_v4(),
            expected_version: state.as_ref().map_or(0, |r| r.version),
            snapshot,
        };
        let pending = serde_json::to_value(&push).map_err(|e| invalid(e.to_string()))?;
        sync_reviews::Entity::insert(sync_reviews::ActiveModel {
            context_id: Set(id),
            version: Set(push.expected_version),
            baseline: Set(None),
            pending: Set(Some(pending)),
        })
        .on_conflict(
            OnConflict::column(sync_reviews::Column::ContextId)
                .update_column(sync_reviews::Column::Pending)
                .to_owned(),
        )
        .exec(self.connection.as_ref())
        .await?;
        Ok(Some(push))
    }

    pub async fn acknowledge(&self, push: &Push, version: i64) -> Result<()> {
        self.save_baseline(push.snapshot.context.id, version, &push.snapshot)
            .await
    }

    /// Refuse to overwrite local edits; a conflict leaves both versions recoverable.
    pub async fn accept(&self, snapshot: &Snapshot, version: i64) -> Result<()> {
        let id = snapshot.context.id;
        let state = self.get_state(id).await?;
        if state.as_ref().is_some_and(|r| r.version >= version) {
            return Ok(());
        }
        if review_contexts::Entity::find_by_id(id)
            .one(self.connection.as_ref())
            .await?
            .is_some()
        {
            let current = serde_json::to_value(self.get_snapshot(id).await?)
                .map_err(|e| invalid(e.to_string()))?;
            if state.as_ref().and_then(|r| r.baseline.as_ref()) != Some(&current) {
                return Err(invalid(format!(
                    "Sync conflict for {id}; local changes retained"
                )));
            }
        }
        self.apply_snapshot(snapshot).await?;
        self.save_baseline(id, version, snapshot).await
    }

    /// Replace a review inside the caller-owned transaction. Device associations remain local.
    pub async fn apply_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        snapshot.validate()?;
        let conn = self.connection.as_ref();
        let id = snapshot.context.id;
        let local_context = review_contexts::Entity::find_by_id(id).one(conn).await?;
        let local_threads = threads::Entity::find()
            .filter(threads::Column::ContextId.eq(id))
            .all(conn)
            .await?;
        for row in &snapshot.identities {
            identities::Entity::insert(row.clone().into_active_model().reset_all())
                .on_conflict(
                    OnConflict::column(identities::Column::Id)
                        .do_nothing()
                        .to_owned(),
                )
                .try_insert()
                .exec(conn)
                .await?;
        }
        let mut context = snapshot.context.clone();
        if let Some(local) = local_context {
            context.notes_path = local.notes_path;
            context.git_dir = local.git_dir;
            context.current_notes_path = local.current_notes_path;
            context.into_active_model().reset_all().update(conn).await?;
        } else {
            review_contexts::Entity::insert(context.into_active_model().reset_all())
                .exec(conn)
                .await?;
        }
        // Delete children first so absent records represent real deletions.
        comments::Entity::delete_many()
            .filter(comments::Column::ContextId.eq(id))
            .exec(conn)
            .await?;
        threads::Entity::delete_many()
            .filter(threads::Column::ContextId.eq(id))
            .exec(conn)
            .await?;
        sessions::Entity::delete_many()
            .filter(sessions::Column::ContextId.eq(id))
            .exec(conn)
            .await?;
        file_reviews::Entity::delete_many()
            .filter(file_reviews::Column::ContextId.eq(id))
            .exec(conn)
            .await?;
        for row in &snapshot.sessions {
            sessions::Entity::insert(row.clone().into_active_model().reset_all())
                .exec(conn)
                .await?;
        }
        for row in &snapshot.threads {
            let mut row = row.clone();
            if let Some(local) = local_threads.iter().find(|r| r.id == row.id) {
                row.source_path = local.source_path.clone();
                if local.file_path.as_ref().is_some_and(|p| p.starts_with('/')) {
                    row.file_path = local.file_path.clone();
                }
                if local
                    .current_path
                    .as_ref()
                    .is_some_and(|p| p.starts_with('/'))
                {
                    row.current_path = local.current_path.clone();
                }
            }
            threads::Entity::insert(row.into_active_model().reset_all())
                .exec(conn)
                .await?;
        }
        for row in &snapshot.comments {
            comments::Entity::insert(row.clone().into_active_model().reset_all())
                .exec(conn)
                .await?;
        }
        for row in &snapshot.file_reviews {
            file_reviews::Entity::insert(row.clone().into_active_model().reset_all())
                .exec(conn)
                .await?;
        }
        Ok(())
    }

    // Private -----------------------------------------------------------------

    async fn get_state(&self, id: Uuid) -> Result<Option<sync_reviews::Model>> {
        Ok(sync_reviews::Entity::find_by_id(id)
            .one(self.connection.as_ref())
            .await?)
    }

    async fn save_baseline(&self, id: Uuid, version: i64, snapshot: &Snapshot) -> Result<()> {
        let baseline: Value = serde_json::to_value(snapshot).map_err(|e| invalid(e.to_string()))?;
        sync_reviews::Entity::insert(sync_reviews::ActiveModel {
            context_id: Set(id),
            version: Set(version),
            baseline: Set(Some(baseline)),
            pending: Set(None),
        })
        .on_conflict(
            OnConflict::column(sync_reviews::Column::ContextId)
                .update_columns([
                    sync_reviews::Column::Version,
                    sync_reviews::Column::Baseline,
                    sync_reviews::Column::Pending,
                ])
                .to_owned(),
        )
        .exec(self.connection.as_ref())
        .await?;
        Ok(())
    }
}
