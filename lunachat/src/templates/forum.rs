use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::{Extension, RequestPartsExt as _};
use sea_orm::DatabaseConnection;

use super::partial;
use crate::prelude::*;

pub struct ForumGet {
    pub threads: Vec<partial::PartialThreadGet>,
}

impl<S> FromRequestParts<S> for ForumGet
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(db) = parts.extract::<Extension<DatabaseConnection>>().await?;

        let threads = thread::Entity::find()
            .all(&db)
            .await?
            .into_iter()
            .map_async(async |thread| {
                let root = db.get_root_post_of(thread.id).await?;
                let author = db.get_user(root.author_id).await?;
                Ok(partial::PartialThreadGet {
                    id: thread.id,
                    title: thread.title,
                    body: root.body,
                    author,
                    sse: false,
                })
            })
            .await
            .into_iter()
            .collect::<Result<Vec<partial::PartialThreadGet>>>()?;
        Ok(ForumGet { threads })
    }
}
