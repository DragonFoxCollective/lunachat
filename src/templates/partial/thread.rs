use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Sse};
use axum::{Extension, RequestPartsExt as _};
use bincode::Options as _;
use futures::stream;
use serde::{Deserialize, Serialize};
use sled::Subscriber;

use crate::error::{Error, Result};
use crate::some_or_continue;
use crate::state::post::Posts;
use crate::state::thread::{Thread, ThreadKey, Threads};
use crate::state::user::{User, Users};
use crate::state::{BINCODE, DbTreeLookup as _};

#[derive(Clone, Serialize, Deserialize)]
pub struct ThreadTemplate {
    pub key: ThreadKey,
    pub title: String,
    pub body: String,
    pub author: User,
    pub sse: bool,
}

pub struct ThreadSse {
    threads: Threads,
    posts: Posts,
    users: Users,
}

impl ThreadSse {
    pub fn into_sse(
        self,
        mapper: impl Fn(ThreadTemplate) -> Result<String> + Send + Sync + 'static,
    ) -> impl IntoResponse {
        async fn get_valid_single(
            mut sub: &mut Subscriber,
            posts: &Posts,
            users: &Users,
            mapper: impl Fn(ThreadTemplate) -> Result<String>,
        ) -> Result<Event> {
            loop {
                let event = some_or_continue!((&mut sub).await);
                let thread = match event {
                    sled::Event::Insert { value, .. } => value,
                    sled::Event::Remove { .. } => continue,
                };
                let thread: Thread = BINCODE.deserialize(&thread)?;
                let root_post = posts
                    .get(thread.post)?
                    .ok_or(Error::PostNotFound(thread.post))?;
                let author = users
                    .get(root_post.author)?
                    .ok_or(Error::UserNotFound(root_post.author))?;
                let template = ThreadTemplate {
                    key: thread.key,
                    title: thread.title,
                    body: root_post.body,
                    author,
                    sse: true,
                };
                let data = mapper(template)?;
                let event = Event::default().data(data);
                return Ok(event);
            }
        }

        let Self {
            threads,
            posts,
            users,
        } = self;
        let sub = threads.watch();
        let stream = stream::unfold(
            (sub, posts, users, mapper),
            async move |(mut sub, posts, users, mapper)| {
                Some((
                    get_valid_single(&mut sub, &posts, &users, &mapper).await,
                    (sub, posts, users, mapper),
                ))
            },
        );

        Sse::new(stream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(1))
                .text("keep-alive-text"),
        )
    }
}

impl<S> FromRequestParts<S> for ThreadSse
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(threads) = parts.extract::<Extension<Threads>>().await?;
        let Extension(posts) = parts.extract::<Extension<Posts>>().await?;
        let Extension(users) = parts.extract::<Extension<Users>>().await?;

        Ok(ThreadSse {
            threads,
            posts,
            users,
        })
    }
}
