//! Private cloud storage: account cursor, current review snapshot and retry receipt.
pub mod accounts {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "sync_accounts")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: uuid::Uuid,
        pub cursor: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod reviews {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "sync_documents")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub owner_id: uuid::Uuid,
        #[sea_orm(primary_key, auto_increment = false)]
        pub context_id: uuid::Uuid,
        pub version: i64,
        pub snapshot: Json,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod receipts {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "sync_receipts")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub owner_id: uuid::Uuid,
        #[sea_orm(primary_key, auto_increment = false)]
        pub operation_id: uuid::Uuid,
        pub request: Json,
        pub version: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}
