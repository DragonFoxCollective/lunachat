use derive_more::Display;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Id,
    #[sea_orm(unique)]
    pub username: String,
    pub password: String,
    pub avatar: Option<String>,
    #[sea_orm(has_many, relation_enum = "Posts", relation_reverse = "Author")]
    pub posts: HasMany<post::Entity>,
}

#[derive(DeriveIntoActiveModel)]
pub struct NewModel {
    pub username: String,
    pub password: String,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(
    Copy, Clone, Debug, Display, Eq, PartialEq, Hash, DeriveValueType, Serialize, Deserialize,
)]
pub struct Id(i64);
