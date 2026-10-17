use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub context_id: String,
    pub creator_id: String,
    pub state: String,
    pub base_commit: Option<String>,
    pub head_commit: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub completed_by_id: Option<String>,
    pub outcome: Option<String>,
    pub body: Option<String>,
    pub forge_review_id: Option<String>,
    pub submission_status: String,
    pub submission_error: Option<String>,
    pub request_key: Option<String>,
    pub request_hash: Option<String>,

    #[sea_orm(belongs_to, relation_enum = "Creator", from = "creator_id", to = "id")]
    pub creator: BelongsTo<super::identities::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "CompletedBy",
        from = "completed_by_id",
        to = "id"
    )]
    pub completed_by: BelongsTo<Option<super::identities::Entity>>,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    pub context: BelongsTo<super::review_contexts::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
