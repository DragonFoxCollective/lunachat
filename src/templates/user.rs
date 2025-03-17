use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::{Extension, RequestPartsExt as _};

use crate::error::{Error, Result};
use crate::state::DbTreeLookup as _;
use crate::state::user::{User, UserKey, Users};

pub struct UserTemplate {
    pub user: User,
}

impl<S> FromRequestParts<S> for UserTemplate
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(users) = parts.extract::<Extension<Users>>().await?;
        let Path(user_key) = parts.extract::<Path<UserKey>>().await?;

        let user = users.get(user_key)?.ok_or(Error::UserNotFound(user_key))?;
        Ok(UserTemplate { user })
    }
}
