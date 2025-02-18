use std::convert::Infallible;
use std::time::{Duration, Instant};

use askama::Template;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Sse};
use axum::routing::{get, post};
use axum::Router;
use futures::{stream, Stream};
use templates::{HtmlTemplate, IndexTemplate, PostTemplate};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tower_http::services::ServeDir;
use tracing::{debug, error};

mod templates;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("lunachat=trace")
        .init();

    let (send, _) = broadcast::channel(2);
    let send_sub = send.clone();

    let app = Router::new()
        .route("/", get(root))
        .route("/clicked", post(move || clicked(send.clone())))
        .route("/sse/posts", get(move || sse_handler(send_sub.subscribe())))
        .nest_service("/public", ServeDir::new("public"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> impl IntoResponse {
    HtmlTemplate(IndexTemplate)
}

async fn clicked(send: Sender<PostTemplate>) -> impl IntoResponse {
    debug!("Button clicked");

    if let Err(err) = send.send(PostTemplate {
        author: "Bepisman".to_string(),
        profile_picture: "/public/checker.png".to_string(),
        body: format!("You clicked the button! {:?}", Instant::now()),
    }) {
        error!("Failed to broadcast message. Error: {}", err);
    }
}

async fn sse_handler(
    recv: Receiver<PostTemplate>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    debug!("SSE connection established");

    let stream = stream::unfold(recv, move |mut recv| async {
        match recv.recv().await {
            Ok(template) => match template.render() {
                Ok(template) => Some((Ok(Event::default().data(template)), recv)),
                Err(err) => {
                    error!("Failed to render template. Error: {}", err);
                    None
                }
            },
            Err(err) => {
                error!("Failed to receive message. Error: {}", err);
                None
            }
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-text"),
    )
}
