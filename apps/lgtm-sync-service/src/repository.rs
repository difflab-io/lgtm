use crate::entities::{accounts, receipts, reviews};
use anyhow::Result;
use lgtm_db::sync::{Change, Pull, Push};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Schema, Set, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use uuid::Uuid;

/// Separate cloud persistence; no filesystem or forge behavior lives here.
#[derive(Clone)]
pub struct Repository {
    connection: DatabaseConnection,
}

impl Repository {
    pub async fn connect(url: &str) -> Result<Self> {
        let connection = sea_orm::Database::connect(url).await?;
        let schema = Schema::new(connection.get_database_backend());
        // POC-only schema initialization. Production needs a migration job.
        for mut table in [
            schema.create_table_from_entity(accounts::Entity),
            schema.create_table_from_entity(reviews::Entity),
            schema.create_table_from_entity(receipts::Entity),
        ] {
            table.if_not_exists();
            connection.execute(&table).await?;
        }
        Ok(Self { connection })
    }

    // Public ------------------------------------------------------------------

    pub async fn push(&self, owner_id: Uuid, push: Push) -> Result<Option<Change>> {
        push.snapshot.validate()?;
        let tx = self.connection.begin().await?;
        accounts::Entity::insert(accounts::ActiveModel {
            id: Set(owner_id),
            cursor: Set(0),
        })
        .on_conflict(
            OnConflict::column(accounts::Column::Id)
                .do_nothing()
                .to_owned(),
        )
        .try_insert()
        .exec(&tx)
        .await?;
        // Serialize acceptance per account so cursor order is also commit order.
        accounts::Entity::update_many()
            .col_expr(
                accounts::Column::Cursor,
                Expr::col(accounts::Column::Cursor),
            )
            .filter(accounts::Column::Id.eq(owner_id))
            .exec(&tx)
            .await?;
        let request = serde_json::to_value(&push)?;
        if let Some(receipt) = receipts::Entity::find_by_id((owner_id, push.operation_id))
            .one(&tx)
            .await?
        {
            if receipt.request != request {
                return Ok(None);
            }
            return Ok(Some(Change {
                version: receipt.version,
                snapshot: push.snapshot,
            }));
        }
        let id = push.snapshot.context.id;
        let previous = reviews::Entity::find_by_id((owner_id, id)).one(&tx).await?;
        if previous.as_ref().map_or(0, |r| r.version) != push.expected_version {
            return Ok(None);
        }
        let version = accounts::Entity::find_by_id(owner_id)
            .one(&tx)
            .await?
            .unwrap()
            .cursor
            + 1;
        accounts::ActiveModel {
            id: Set(owner_id),
            cursor: Set(version),
        }
        .update(&tx)
        .await?;
        reviews::Entity::insert(reviews::ActiveModel {
            owner_id: Set(owner_id),
            context_id: Set(id),
            version: Set(version),
            snapshot: Set(serde_json::to_value(&push.snapshot)?),
        })
        .on_conflict(
            OnConflict::columns([reviews::Column::OwnerId, reviews::Column::ContextId])
                .update_columns([reviews::Column::Version, reviews::Column::Snapshot])
                .to_owned(),
        )
        .exec(&tx)
        .await?;
        receipts::Entity::insert(receipts::ActiveModel {
            owner_id: Set(owner_id),
            operation_id: Set(push.operation_id),
            request: Set(request),
            version: Set(version),
        })
        .exec(&tx)
        .await?;
        tx.commit().await?;
        Ok(Some(Change {
            version,
            snapshot: push.snapshot,
        }))
    }

    /// Fetch current snapshots through a fixed cursor boundary, scoped to the authenticated account.
    pub async fn get_changes(&self, owner_id: Uuid, cursor: i64) -> Result<Pull> {
        let tx = self.connection.begin().await?;
        let end = accounts::Entity::find_by_id(owner_id)
            .one(&tx)
            .await?
            .map_or(0, |r| r.cursor);
        let rows = reviews::Entity::find()
            .filter(reviews::Column::OwnerId.eq(owner_id))
            .filter(reviews::Column::Version.gt(cursor))
            .filter(reviews::Column::Version.lte(end))
            .order_by_asc(reviews::Column::Version)
            .all(&tx)
            .await?;
        let changes = rows
            .into_iter()
            .map(|r| {
                Ok(Change {
                    version: r.version,
                    snapshot: serde_json::from_value(r.snapshot)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(Pull {
            cursor: end,
            changes,
        })
    }
}
