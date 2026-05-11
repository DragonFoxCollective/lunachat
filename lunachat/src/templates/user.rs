use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::{Extension, RequestPartsExt as _};

use crate::prelude::*;

pub struct UserGet {
    pub user: user::Model,
}

impl<S> FromRequestParts<S> for UserGet
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(db) = parts.extract::<Extension<DatabaseConnection>>().await?;
        let Path(user_id) = parts.extract::<Path<user::Id>>().await?;

        let user = db.get_user(user_id).await?;
        Ok(UserGet { user })
    }
}
