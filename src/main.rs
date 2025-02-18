use std::time::Duration;

use askama::Template;
use axum::extract::State;
use axum::response::sse::Event;
use axum::response::Sse;
use axum::routing::{get, post};
use axum::{Form, Router};
use error::Error;
use futures::{stream, Stream};
use sled::{IVec, Subscriber};
use state::{AppState, Posts, Sanitizer};
use templates::{HtmlTemplate, IndexTemplate, PostTemplate};
use tower_http::services::ServeDir;
use tracing::debug;

mod error;
mod state;
mod templates;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("lunachat=trace")
        .init();

    let db = sled::open("db").unwrap();
    let posts = Posts(db.open_tree("posts").unwrap());

    let mut builder = ammonia::Builder::new();
    builder.add_generic_attributes(["style"]);
    let sanitizer = Sanitizer::new(builder);

    let state = AppState { posts, sanitizer };

    let app = Router::new()
        .route("/", get(root))
        .route("/create-post", post(create_post))
        .route("/sse/posts", get(sse_handler))
        .with_state(state)
        .nest_service("/public", ServeDir::new("public"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root(State(posts): State<Posts>) -> Result<HtmlTemplate<IndexTemplate>, Error> {
    let posts = posts
        .into_iter()
        .values()
        .map(|post| Ok(bincode::deserialize::<PostTemplate>(&post?)?.render()?))
        .collect::<Result<Vec<String>, Error>>()?
        .join("\n");
    Ok(HtmlTemplate(IndexTemplate { posts }))
}

async fn create_post(
    State(posts): State<Posts>,
    State(sanitizer): State<Sanitizer>,
    Form(mut post): Form<PostTemplate>,
) -> Result<(), Error> {
    debug!("Post created!");

    let last_post = posts.last().unwrap();
    let last_id = match last_post {
        Some((id, _)) => {
            let id_array: [u8; 8] = (*id).try_into()?;
            u64::from_be_bytes(id_array)
        }
        None => 0,
    };
    let this_id = last_id + 1;

    post.body = sanitizer.clean(&post.body).to_string();

    posts.insert(this_id.to_be_bytes(), bincode::serialize(&post)?)?;
    posts.flush_async().await?;

    Ok(())
}

async fn sse_handler(State(posts): State<Posts>) -> Sse<impl Stream<Item = Result<Event, Error>>> {
    debug!("SSE connection established");

    let sub = posts.watch_prefix([]);
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
