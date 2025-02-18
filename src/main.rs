use std::time::Duration;

use askama::Template;
use auth::{AuthSession, Backend};
use axum::extract::State;
use axum::response::sse::Event;
use axum::response::Sse;
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_login::tower_sessions::{MemoryStore, SessionManagerLayer};
use axum_login::{permission_required, AuthManagerLayerBuilder, AuthzBackend};
use error::Result;
use futures::{stream, Stream, StreamExt};
use state::{AppState, Posts, Sanitizer, Users, UsersUsernameMap};
use templates::{HtmlTemplate, IndexTemplate, PostTemplate};
use tower_http::services::ServeDir;
use tracing::debug;

mod auth;
mod error;
mod state;
mod templates;
mod utils;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("lunachat=trace")
        .init();

    // DB
    let db = sled::open("db").unwrap();
    let posts = Posts::new(db.open_tree("posts").unwrap());
    let users = Users::new(db.open_tree("users").unwrap());
    let users_username_map = UsersUsernameMap::new(
        db.open_tree("usernames").unwrap(),
        db.open_tree("users").unwrap(),
    );

    // Session layer
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);

    // Auth service
    let backend = Backend::new(users.clone(), users_username_map.clone());
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    // Sanitizer
    let mut builder = ammonia::Builder::new();
    builder.add_generic_attributes(["style"]);
    let sanitizer = Sanitizer::new(builder);

    // State
    let state = AppState {
        posts,
        users,
        users_username_map,
        sanitizer,
    };

    let app = Router::new()
        .route("/create-post", post(create_post))
        .route_layer(permission_required!(Backend, login_url = "/login", "post"))
        .route("/", get(root))
        .route("/sse/posts", get(sse_handler))
        .layer(auth_layer)
        .with_state(state)
        .nest_service("/public", ServeDir::new("public"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root(
    auth: AuthSession,
    State(posts): State<Posts>,
) -> Result<HtmlTemplate<IndexTemplate>> {
    let posts = posts
        .into_iter()
        .values()
        .map(|post| Ok(post?.render()?))
        .collect::<Result<Vec<String>>>()?
        .join("\n");
    Ok(HtmlTemplate(IndexTemplate {
        posts,
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, "post".into()).await?,
            None => false,
        },
    }))
}

async fn create_post(
    State(posts): State<Posts>,
    State(sanitizer): State<Sanitizer>,
    Form(mut post): Form<PostTemplate>,
) -> Result<()> {
    debug!("Post created!");

    let id = posts
        .last()?
        .map(|(key, _)| key.incremented())
        .unwrap_or_default();

    post.body = sanitizer.clean(&post.body).to_string();

    posts.insert(id, post)?;
    posts.flush_async().await?;

    Ok(())
}

async fn sse_handler(State(posts): State<Posts>) -> Sse<impl Stream<Item = Result<Event>>> {
    debug!("SSE connection established");

    let sub = posts.watch();
    let stream = stream::unfold(sub, move |mut sub| async {
        (&mut sub).await.map(|event| (event, sub))
    })
    .filter_map(|event| async {
        let template = match event {
            sled::Event::Insert { value, .. } => Some(value),
            sled::Event::Remove { .. } => None,
        }?;
        let template: PostTemplate = option_ok!(bincode::deserialize(&template));
        let data = option_ok!(template.render());
        let event = Event::default().data(data);
        Some(Ok(event))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-text"),
    )
}
