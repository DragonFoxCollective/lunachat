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
#[sea_orm(table_name = "thread")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Id,
    pub title: String,
    #[sea_orm(
        has_many,
        relation_enum = "Posts",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    pub posts: HasMany<super::post::Entity>,
}

pub struct NewModel {
    pub title: String,
    pub body: String,
    pub author_id: user::Id,
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
