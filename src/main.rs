use std::time::{Duration, Instant};

use askama::Template;
use axum::response::sse::Event;
use axum::response::Sse;
use axum::routing::{get, post};
use axum::Router;
use error::Error;
use futures::{stream, Stream};
use sled::{IVec, Subscriber, Tree};
use templates::{HtmlTemplate, IndexTemplate, PostTemplate};
use tower_http::services::ServeDir;
use tracing::debug;

mod error;
mod templates;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("lunachat=trace")
        .init();

    let db = sled::open("db").unwrap();
    let posts = db.open_tree("posts").unwrap();
    let posts_clicked = posts.clone();
    let posts_sse = posts.clone();

    let app = Router::new()
        .route("/", get(move || root(posts.clone())))
        .route("/clicked", post(move || clicked(posts_clicked.clone())))
        .route(
            "/sse/posts",
            get(move || sse_handler(posts_sse.watch_prefix([]))),
        )
        .nest_service("/public", ServeDir::new("public"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root(posts: Tree) -> Result<HtmlTemplate<IndexTemplate>, Error> {
    let posts = posts
        .into_iter()
        .values()
        .map(|post| Ok(bincode::deserialize::<PostTemplate>(&post?)?.render()?))
        .collect::<Result<Vec<String>, Error>>()?
        .join("\n");
    Ok(HtmlTemplate(IndexTemplate { posts }))
}

async fn clicked(posts: Tree) -> Result<(), Error> {
    debug!("Button clicked");

    let last_post = posts.last().unwrap();
    let last_id = match last_post {
        Some((id, _)) => {
            let id_array: [u8; 8] = (*id).try_into()?;
            u64::from_be_bytes(id_array)
        }
        None => 0,
    };
    let this_id = last_id + 1;

    posts.insert(
        this_id.to_be_bytes(),
        bincode::serialize(&PostTemplate {
            author: "Bepisman".to_string(),
            profile_picture: "/public/checker.png".to_string(),
            body: format!("You clicked the button! {:?}", Instant::now()),
        })?,
    )?;

    posts.flush_async().await?;

    Ok(())
}

async fn sse_handler(sub: Subscriber) -> Sse<impl Stream<Item = Result<Event, Error>>> {
    debug!("SSE connection established");

    let stream = stream::unfold(sub, move |mut sub| async {
        let result = extract_template(&mut sub).await?;
        Some((result, sub))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-text"),
    )
}

async fn extract_template(sub: &mut Subscriber) -> Option<Result<Event, Error>> {
    let template = loop {
        match (&mut *sub).await? {
            sled::Event::Insert { value, .. } => {
                break value;
            }
            sled::Event::Remove { .. } => {}
        }
    };
    Some(extract_template_inner(template).await)
}

async fn extract_template_inner(template: IVec) -> Result<Event, Error> {
    let template: PostTemplate = bincode::deserialize(&template)?;
    let data = template.render()?;
    let event = Event::default().data(data);
    Ok(event)
}
