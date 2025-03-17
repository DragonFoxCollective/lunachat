use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::{Extension, RequestPartsExt as _};

use crate::error::{Error, Result};
use crate::state::DbTreeLookup as _;
use crate::state::post::Posts;
use crate::state::thread::Threads;
use crate::state::user::Users;

use super::partial;

pub struct ForumTemplate {
    pub threads: Vec<partial::ThreadTemplate>,
}

impl<S> FromRequestParts<S> for ForumTemplate
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(threads) = parts.extract::<Extension<Threads>>().await?;
        let Extension(posts) = parts.extract::<Extension<Posts>>().await?;
        let Extension(users) = parts.extract::<Extension<Users>>().await?;

        let threads = threads
            .iter()
            .values()
            .map(|thread| {
                let thread = thread?;
                let post = posts
                    .get(thread.post)?
                    .ok_or(Error::PostNotFound(thread.post))?;
                let author = users
                    .get(post.author)?
                    .ok_or(Error::UserNotFound(post.author))?;
                let template = partial::ThreadTemplate {
                    key: thread.key,
                    title: thread.title,
                    body: post.body,
                    author,
                    sse: false,
                };
                Ok(template)
            })
            .collect::<Result<Vec<partial::ThreadTemplate>>>()?;
        Ok(ForumTemplate { threads })
    }
}
