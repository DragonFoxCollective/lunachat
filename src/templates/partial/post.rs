use std::time::Duration;

use axum::extract::{FromRequestParts, Path};
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
use crate::state::post::{Post, PostKey, Posts};
use crate::state::thread::ThreadKey;
use crate::state::user::{User, Users};
use crate::state::{BINCODE, DbTreeLookup as _};

#[derive(Clone, Serialize, Deserialize)]
pub struct PostTemplate {
    pub key: PostKey,
    pub author: User,
    pub body: String,
    pub sse: bool,
}

pub struct PostSse {
    posts: Posts,
    users: Users,
    thread_key: ThreadKey,
}

impl PostSse {
    pub fn into_sse(
        self,
        mapper: impl Fn(PostTemplate) -> Result<String> + Send + Sync + 'static,
    ) -> impl IntoResponse {
        async fn get_valid_single(
            mut sub: &mut Subscriber,
            users: &Users,
            thread_key: &ThreadKey,
            mapper: impl Fn(PostTemplate) -> Result<String>,
        ) -> Result<Event> {
            loop {
                let event = some_or_continue!((&mut sub).await);
                let post = match event {
                    sled::Event::Insert { value, .. } => value,
                    sled::Event::Remove { .. } => continue,
                };
                let post: Post = BINCODE.deserialize(&post)?;
                if post.thread != *thread_key {
                    continue;
                }
                let author = users
                    .get(post.author)?
                    .ok_or(Error::UserNotFound(post.author))?;
                let template = PostTemplate {
                    key: post.key,
                    body: post.body,
                    author,
                    sse: true,
                };
                let data = mapper(template)?;
                let event = Event::default().data(data);
                return Ok(event);
            }
        }

        let Self {
            posts,
            users,
            thread_key,
        } = self;
        let sub = posts.watch();
        let stream = stream::unfold(
            (sub, users, thread_key, mapper),
            async move |(mut sub, users, thread_key, mapper)| {
                Some((
                    get_valid_single(&mut sub, &users, &thread_key, &mapper).await,
                    (sub, users, thread_key, mapper),
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

impl<S> FromRequestParts<S> for PostSse
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Extension(posts) = parts.extract::<Extension<Posts>>().await?;
        let Extension(users) = parts.extract::<Extension<Users>>().await?;
        let Path(thread_key) = parts.extract::<Path<ThreadKey>>().await?;

        Ok(PostSse {
            posts,
            users,
            thread_key,
        })
    }
}
