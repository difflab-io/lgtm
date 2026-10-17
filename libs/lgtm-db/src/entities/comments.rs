use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "comments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub context_id: String,
    pub thread_id: String,
    pub origin_session_id: Option<String>,
    pub author_id: String,
    pub body: String,
    pub position: i64,
    pub visibility: String,
    pub created_at: String,
    pub updated_at: String,
    pub forge_comment_id: Option<String>,
    pub forge_object_kind: Option<String>,
    pub forge_review_id: Option<String>,
    pub forge_body: Option<String>,
    pub forge_updated_at: Option<String>,
    pub posting_account_id: Option<String>,
    pub posting_state: String,
    pub posting_error: Option<String>,
    pub posting_request_key: Option<String>,
    pub payload_hash: Option<String>,
    pub remote_presence: String,

    #[sea_orm(belongs_to, relation_enum = "Author", from = "author_id", to = "id")]
    pub author: BelongsTo<super::identities::Entity>,

    #[sea_orm(belongs_to, relation_enum = "Context", from = "context_id", to = "id")]
    pub context: BelongsTo<super::review_contexts::Entity>,

    #[sea_orm(belongs_to, relation_enum = "Thread", from = "thread_id", to = "id")]
    pub thread: BelongsTo<super::threads::Entity>,

    #[sea_orm(
        belongs_to,
        relation_enum = "OriginSession",
        from = "origin_session_id",
        to = "id"
    )]
    pub origin_session: BelongsTo<Option<super::sessions::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
