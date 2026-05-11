use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Sse};
use axum::{Extension, RequestPartsExt as _};
use futures::stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;

use crate::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct PartialThreadGet {
    pub id: thread::Id,
    pub title: String,
    pub body: String,
    pub author: user::Model,
    pub sse: bool,
}

pub struct ThreadSse {
    db: DatabaseConnection,
}

impl ThreadSse {
    pub fn into_sse(
        self,
        mapper: impl Fn(PartialThreadGet) -> Result<String> + Send + Sync + 'static,
    ) -> impl IntoResponse {
        async fn get_valid_single(
            sub: &mut Receiver<BroadcastEvent<thread::Model>>,
            db: &DatabaseConnection,
            mapper: impl Fn(PartialThreadGet) -> Result<String>,
        ) -> Result<Event> {
            loop {
                let thread = match sub.recv().await? {
                    BroadcastEvent::Create(value) => value,
                    BroadcastEvent::Update(value) => value,
                    BroadcastEvent::Delete => continue,
                };
                let post = db.get_root_post_of(thread.id).await?;
                let author = db.get_user(post.author_id).await?;
                let template = PartialThreadGet {
                    id: thread.id,
                    title: thread.title,
                    body: post.body,
                    author,
                    sse: true,
                };
                let data = mapper(template)?;
                let event = Event::default().data(data);
                return Ok(event);
            }
        }

        let Self { db } = self;
        let sub = thread::BROADCAST.subscribe();
        let stream = stream::unfold((sub, db, mapper), async move |(mut sub, db, mapper)| {
            Some((
                get_valid_single(&mut sub, &db, &mapper).await,
                (sub, db, mapper),
            ))
        });

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
        let Extension(db) = parts.extract::<Extension<DatabaseConnection>>().await?;

        Ok(ThreadSse { db })
    }
}
