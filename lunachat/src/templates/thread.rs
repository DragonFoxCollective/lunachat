use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::request::Parts;
use axum::{Extension, Form, RequestExt as _, RequestPartsExt as _};
use serde::{Deserialize, Serialize};

use super::partial;
use crate::auth::AuthSession;
use crate::prelude::*;
use crate::sanitizer::Sanitizer;

pub struct ThreadGet {
    pub id: thread::Id,
    pub posts: Vec<partial::PartialPostGet>,
}

impl<S> FromRequestParts<S> for ThreadGet
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(db) = parts.extract::<Extension<DatabaseConnection>>().await?;
        let Path(thread_id) = parts.extract::<Path<thread::Id>>().await?;

        let (_thread, posts, authors) = db.get_thread_and_posts(thread_id).await?;
        let posts = posts
            .into_iter()
            .map(|post| partial::PartialPostGet {
                id: post.id,
                author: authors[&post.author_id].clone(),
                body: post.body,
                sse: false,
            })
            .collect::<Vec<_>>();

        Ok(ThreadGet {
            id: thread_id,
            posts,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ThreadSubmission {
    pub title: String,
    pub body: String,
}

pub struct ThreadPost(pub thread::Id);

impl<S> FromRequest<S> for ThreadPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| anyhow!("Auth not found"))?;
        let Extension(db) = req.extract_parts::<Extension<DatabaseConnection>>().await?;
        let Extension(sanitizer) = req.extract_parts::<Extension<Sanitizer>>().await?;
        let Form(thread) = req.extract::<Form<ThreadSubmission>, _>().await?;

        let thread = thread::ActiveModel::builder()
            .set_title(sanitizer.clean(&thread.title))
            .add_post(
                post::ActiveModel::builder()
                    .set_author_id(auth.user.ok_or(anyhow!("Not logged in"))?.id)
                    .set_body(sanitizer.clean(&thread.body)),
            )
            .insert(&db)
            .await?;

        Ok(ThreadPost(thread.id))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PostSubmission {
    pub parent_id: post::Id,
    pub body: String,
}

pub struct PostPost(pub post::Id, pub thread::Id);

impl<S> FromRequest<S> for PostPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| anyhow!("Auth not found"))?;
        let Extension(db) = req.extract_parts::<Extension<DatabaseConnection>>().await?;
        let Extension(sanitizer) = req.extract_parts::<Extension<Sanitizer>>().await?;
        let Path(thread_id) = req.extract_parts::<Path<thread::Id>>().await?;
        let Form(post) = req.extract::<Form<PostSubmission>, _>().await?;

        let post = post::ActiveModel::builder()
            .set_author_id(auth.user.ok_or(anyhow!("Not logged in"))?.id)
            .set_body(sanitizer.clean(&post.body))
            .set_parent_id(Some(post.parent_id))
            .set_thread_id(thread_id)
            .insert(&db)
            .await?;

        Ok(PostPost(post.id, thread_id))
    }
}
