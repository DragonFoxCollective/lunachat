use std::collections::HashSet;

use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::request::Parts;
use axum::{Extension, Form, RequestExt as _, RequestPartsExt as _};

use crate::auth::AuthSession;
use crate::error::{Error, Result};
use crate::state::DbTreeLookup as _;
use crate::state::post::{Post, PostKey, PostSubmission, Posts};
use crate::state::sanitizer::Sanitizer;
use crate::state::thread::{Thread, ThreadKey, ThreadSubmission, Threads};
use crate::state::user::Users;

use super::partial;

pub struct ThreadGet {
    pub key: ThreadKey,
    pub posts: Vec<partial::PartialPostGet>,
}

impl<S> FromRequestParts<S> for ThreadGet
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(threads) = parts.extract::<Extension<Threads>>().await?;
        let Extension(posts) = parts.extract::<Extension<Posts>>().await?;
        let Extension(users) = parts.extract::<Extension<Users>>().await?;
        let Path(thread_key) = parts.extract::<Path<ThreadKey>>().await?;

        let thread = threads
            .get(thread_key)?
            .ok_or(Error::ThreadNotFound(thread_key))?;
        let posts = {
            let mut posts_visited = HashSet::new();
            let mut posts_visited_in_order = vec![];
            let mut posts_to_visit = vec![thread.post];
            while let Some(post_key) = posts_to_visit.pop() {
                if posts_visited.contains(&post_key) {
                    continue;
                }
                posts_visited.insert(post_key);
                posts_visited_in_order.push(post_key);
                let post = posts.get(post_key)?.ok_or(Error::PostNotFound(post_key))?;
                posts_to_visit.extend(post.children.iter().copied());
            }
            posts_visited_in_order
                .iter()
                .map(|key| match posts.get(*key) {
                    Ok(Some(post)) => Ok(post),
                    Ok(None) => Err(Error::PostNotFound(*key)),
                    Err(e) => Err(e),
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(|post| {
                    let author = users
                        .get(post.author)?
                        .ok_or(Error::UserNotFound(post.author))?;
                    let template = partial::PartialPostGet {
                        key: post.key,
                        body: post.body,
                        author,
                        sse: false,
                    };
                    Ok(template)
                })
                .collect::<Result<Vec<partial::PartialPostGet>>>()?
        };
        Ok(ThreadGet {
            key: thread_key,
            posts,
        })
    }
}

pub struct ThreadPost(pub ThreadKey);

impl<S> FromRequest<S> for ThreadPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| Error::AuthNotFound)?;
        let Extension(threads) = req.extract_parts::<Extension<Threads>>().await?;
        let Extension(posts) = req.extract_parts::<Extension<Posts>>().await?;
        let Extension(sanitizer) = req.extract_parts::<Extension<Sanitizer>>().await?;
        let Form(thread) = req.extract::<Form<ThreadSubmission>, _>().await?;

        let thread_key = threads.next_key()?;
        let post_key = posts.next_key()?;

        let post = Post {
            key: post_key,
            author: auth.user.ok_or(Error::NotLoggedIn)?.key,
            body: sanitizer.clean(&thread.body).to_string(),
            parent: None,
            children: vec![],
            thread: thread_key,
        };
        posts.insert(post_key, post.clone())?;
        posts.flush().await?;

        let thread = Thread {
            key: thread_key,
            title: sanitizer.clean(&thread.title).to_string(),
            post: post_key,
        };
        threads.insert(thread_key, thread.clone())?;
        threads.flush().await?;

        Ok(ThreadPost(thread_key))
    }
}

pub struct PostPost(pub PostKey, pub ThreadKey);

impl<S> FromRequest<S> for PostPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| Error::AuthNotFound)?;
        let Extension(posts) = req.extract_parts::<Extension<Posts>>().await?;
        let Extension(sanitizer) = req.extract_parts::<Extension<Sanitizer>>().await?;
        let Path(thread_key) = req.extract_parts::<Path<ThreadKey>>().await?;
        let Form(post) = req.extract::<Form<PostSubmission>, _>().await?;

        let key = posts.next_key()?;
        let parent_key = posts
            .iter()
            .values()
            .filter_map(|post| post.ok())
            .filter(|post| post.thread == thread_key)
            .last()
            .ok_or(Error::ThreadHasNoPosts(thread_key))?
            .key;
        let thread_key = posts
            .get(parent_key)?
            .ok_or(Error::PostNotFound(parent_key))?
            .thread;

        let post = Post {
            key,
            author: auth.user.ok_or(Error::NotLoggedIn)?.key,
            body: sanitizer.clean(&post.body).to_string(),
            parent: Some(parent_key),
            children: vec![],
            thread: thread_key,
        };
        posts.insert(key, post.clone())?;

        let mut parent = posts
            .get(parent_key)?
            .ok_or(Error::PostNotFound(parent_key))?;
        parent.children.push(key);
        posts.insert(parent_key, parent)?;

        posts.flush().await?;

        Ok(PostPost(key, thread_key))
    }
}
