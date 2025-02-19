use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{Salt, SaltString};
use argon2::{Argon2, PasswordHasher};
use askama::Template;
use auth::{AuthSession, Backend, Credentials, NextUrl, Permission};
use axum::extract::{Query, State};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Redirect, Sse};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_htmx::HxBoosted;
use axum_login::tower_sessions::{MemoryStore, SessionManagerLayer};
use axum_login::{permission_required, AuthManagerLayerBuilder, AuthzBackend};
use error::Result;
use futures::{stream, Stream, StreamExt};
use state::{AppState, DbTreeLookup, HighestKeys, Posts, Sanitizer, TableType, Users};
use templates::{HtmlTemplate, IndexTemplate, LoginTemplate, PostTemplate};
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
    let users = Users::new(
        db.open_tree("usernames").unwrap(),
        db.open_tree("users").unwrap(),
    );
    let highest_keys = HighestKeys::new(db.open_tree("highest_keys").unwrap());

    // Session layer
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);

    // Auth service
    let backend = Backend::new(users.clone());
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    // Sanitizer
    let mut builder = ammonia::Builder::new();
    builder.add_generic_attributes(["style"]);
    let sanitizer = Sanitizer::new(builder);

    // State
    let state = AppState {
        posts,
        users,
        highest_keys,
        sanitizer,
    };

    let app = Router::new()
        .route("/", post(index_post))
        .route_layer(permission_required!(
            Backend,
            login_url = "/login",
            Permission::Post
        ))
        .route("/", get(index))
        .route("/sse/posts", get(sse_handler))
        .route("/login", get(login))
        .route("/login", post(login_post))
        .route("/logout", get(logout_post))
        .route("/register", post(register_post))
        .layer(auth_layer)
        .with_state(state)
        .nest_service("/public", ServeDir::new("public"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index(auth: AuthSession, State(posts): State<Posts>) -> Result<impl IntoResponse> {
    let posts = posts
        .iter()
        .values()
        .map(|post| Ok(post?.render()?))
        .collect::<Result<Vec<String>>>()?
        .join("\n");
    Ok(HtmlTemplate(IndexTemplate {
        username: auth.user.as_ref().map(|user| user.username.clone()),
        posts,
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, Permission::Post).await?,
            None => false,
        },
    }))
}

async fn index_post(
    State(posts): State<Posts>,
    State(highest_keys): State<HighestKeys>,
    State(sanitizer): State<Sanitizer>,
    HxBoosted(boosted): HxBoosted,
    Form(mut post): Form<PostTemplate>,
) -> Result<impl IntoResponse> {
    debug!("Post created!");

    let key = highest_keys.next(TableType::Posts)?;

    post.body = sanitizer.clean(&post.body).to_string();

    posts.insert(key, post.clone())?;
    posts.flush().await?;

    if boosted {
        Ok(().into_response()) // Handled by SSE
    } else {
        Ok(Redirect::to("/").into_response())
    }
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

async fn login(Query(NextUrl { next }): Query<NextUrl>) -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(LoginTemplate { error: None, next }))
}

async fn login_post(
    mut auth: AuthSession,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse> {
    let user = match auth.authenticate(creds.clone()).await.map_err(Box::new)? {
        Some(user) => user,
        None => {
            return Ok(HtmlTemplate(LoginTemplate {
                error: Some("Username or password incorrect".into()),
                next: creds.next,
            })
            .into_response())
        }
    };

    auth.login(&user).await.map_err(Box::new)?;

    debug!("Logged in user: {:?}", user);

    Ok(Redirect::to(creds.next.as_ref().map_or("/", |v| v)).into_response())
}

async fn logout_post(mut auth: AuthSession) -> Result<impl IntoResponse> {
    auth.logout().await.map_err(Box::new)?;
    Ok(Redirect::to("/").into_response())
}

async fn register_post(
    State(users): State<Users>,
    State(highest_keys): State<HighestKeys>,
    mut auth: AuthSession,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse> {
    if users.get_by_username(&creds.username)?.is_some() {
        return Ok(HtmlTemplate(LoginTemplate {
            error: Some("Username already taken".into()),
            next: None,
        })
        .into_response());
    }

    let key = highest_keys.next(TableType::Users)?;

    let argon2 = Argon2::default();
    let salt_string = SaltString::generate(&mut OsRng);
    let salt: Salt = salt_string.as_salt();
    let password: String = argon2
        .hash_password(creds.password.as_bytes(), salt)?
        .to_string();

    let user = auth::User::new(key, creds.username.clone(), password);
    users.insert(key, user.clone())?;
    users.flush().await?;

    auth.login(&user).await.map_err(Box::new)?;

    debug!("Registered user: {:?}", user);

    Ok(Redirect::to(creds.next.as_ref().map_or("/", |v| v)).into_response())
}
