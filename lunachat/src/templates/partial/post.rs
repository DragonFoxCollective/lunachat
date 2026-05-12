use std::time::Duration;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Sse};
use axum::{Extension, RequestPartsExt as _};
use futures::stream;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;

use crate::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct PartialPostGet {
    pub post: post::Model,
    pub author: user::Model,
}

pub struct PostSse {
    db: DatabaseConnection,
    thread_id: thread::Id,
}

impl PostSse {
    pub fn into_sse(
        self,
        mapper: impl Fn(PartialPostGet) -> Result<String> + Send + Sync + 'static,
    ) -> impl IntoResponse {
        async fn get_valid_single(
            sub: &mut Receiver<BroadcastEvent<post::Model>>,
            db: &DatabaseConnection,
            thread_id: thread::Id,
            mapper: impl Fn(PartialPostGet) -> Result<String>,
        ) -> Result<Event> {
            loop {
                let post = match sub.recv().await? {
                    BroadcastEvent::Create(value) => value,
                    BroadcastEvent::Update(value) => value,
                    BroadcastEvent::Delete => continue,
                };
                if post.thread_id != thread_id {
                    continue;
                }
                let author = db.get_user(post.author_id).await?;
                let template = PartialPostGet { post, author };
                let data = mapper(template)?;
                let event = Event::default().data(data);
                return Ok(event);
            }
        }

        let Self { db, thread_id } = self;
        let sub = post::BROADCAST.subscribe();
        let stream = stream::unfold(
            (sub, db, thread_id, mapper),
            async move |(mut sub, db, thread_id, mapper)| {
                Some((
                    get_valid_single(&mut sub, &db, thread_id, &mapper).await,
                    (sub, db, thread_id, mapper),
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
        let Extension(db) = parts.extract::<Extension<DatabaseConnection>>().await?;
        let Path(thread_id) = parts.extract::<Path<thread::Id>>().await?;

        Ok(PostSse { db, thread_id })
    }
}
