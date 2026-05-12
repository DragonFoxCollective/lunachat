use async_trait::async_trait;
use derive_more::Display;
use lazy_static::lazy_static;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{Sender, channel};

use crate::prelude::*;

lazy_static! {
    pub static ref BROADCAST: Sender<BroadcastEvent<Model>> = channel(16).0;
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "post")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Id,
    pub body: String,
    pub created_at: DateTimeUtc,
    pub author_id: user::Id,
    #[sea_orm(
        belongs_to,
        relation_enum = "Author",
        relation_reverse = "Posts",
        from = "author_id",
        to = "id"
    )]
    pub author: HasOne<user::Entity>,
    pub thread_id: thread::Id,
    #[sea_orm(belongs_to, relation_reverse = "Posts", from = "thread_id", to = "id")]
    pub thread: HasOne<thread::Entity>,
    pub parent_id: Option<Id>,
    #[sea_orm(
        self_ref,
        relation_enum = "Parent",
        relation_reverse = "Children",
        from = "parent_id",
        to = "id"
    )]
    pub parent: HasOne<Entity>,
    #[sea_orm(self_ref, relation_enum = "Children", relation_reverse = "Parent")]
    pub children: HasMany<Entity>,
}

#[derive(DeriveIntoActiveModel)]
#[sea_orm(set(created_at = "chrono::Utc::now()"))]
pub struct NewModel {
    pub body: String,
    pub author_id: user::Id,
    pub thread_id: thread::Id,
    pub parent_id: Option<Id>,
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn after_save<C>(model: Model, _db: &C, insert: bool) -> Result<Model, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            let _ = BROADCAST.send(BroadcastEvent::Create(model.clone()));
        } else {
            let _ = BROADCAST.send(BroadcastEvent::Update(model.clone()));
        }
        Ok(model)
    }

    async fn after_delete<C>(self, _db: &C) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let _ = BROADCAST.send(BroadcastEvent::Delete);
        Ok(self)
    }
}

#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, Hash, DeriveValueType, Serialize, Deserialize,
)]
pub struct Id(i64);
