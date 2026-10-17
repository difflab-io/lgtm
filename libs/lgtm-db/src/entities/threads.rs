use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "threads")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub context_id: String,
    pub origin_session_id: Option<String>,
    pub author_id: String,
    pub anchor_kind: String,
    pub file_path: Option<String>,
    pub side: Option<String>,
    pub reviewed_commit: Option<String>,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub state: String,
    pub resolved_by_id: Option<String>,
    pub resolved_at: Option<String>,
    pub updated_by_id: Option<String>,
    pub updated_at: String,
    pub forge_thread_id: Option<String>,
    pub outdated: bool,

    #[sea_orm(belongs_to, relation_enum = "Author", from = "author_id", to = "id")]
    pub author: BelongsTo<super::identities::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "ResolvedBy",
        from = "resolved_by_id",
        to = "id"
    )]
    pub resolved_by: BelongsTo<Option<super::identities::Entity>>,

    #[sea_orm(
        belongs_to,
        relation_enum = "UpdatedBy",
        from = "updated_by_id",
        to = "id"
    )]
    pub updated_by: BelongsTo<Option<super::identities::Entity>>,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    pub context: BelongsTo<super::review_contexts::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "OriginSession",
        from = "origin_session_id",
        to = "id"
    )]
    pub origin_session: BelongsTo<Option<super::sessions::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
