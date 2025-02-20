use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{Salt, SaltString};
use argon2::{Argon2, PasswordHasher};
use askama::Template;
use axum::extract::{Query, State};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Redirect, Sse};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use axum_htmx::HxBoosted;
use axum_login::tower_sessions::{MemoryStore, SessionManagerLayer};
use axum_login::{permission_required, AuthManagerLayerBuilder, AuthzBackend};
use bincode::Options as _;
use futures::{stream, Stream};
use lunachat::auth::{AuthSession, Backend, Credentials, NextUrl, Permission};
use lunachat::error::{Error, Result};
use lunachat::state::key::HighestKeys;
use lunachat::state::post::{Post, PostSubmission, Posts};
use lunachat::state::sanitizer::Sanitizer;
use lunachat::state::user::{User, Users};
use lunachat::state::{AppState, DbTreeLookup, TableType, Versions, BINCODE};
use lunachat::templates::{
    ForumTemplate, HtmlTemplate, IndexTemplate, LoginTemplate, PostTemplate,
};
use lunachat::{option_ok, some_ok};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tower_http::services::ServeDir;
use tracing::{debug, warn};

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

    // Versioning
    {
        let versions = Versions::new(db.open_tree("versions").unwrap());
        let mut modified = false;
        if versions.get(TableType::Posts).unwrap().is_none() {
            versions.insert(TableType::Posts, 1).unwrap();
            modified = true;
        }
        if versions.get(TableType::Users).unwrap().is_none() {
            versions.insert(TableType::Users, 1).unwrap();
            modified = true;
        }
        if versions.get(TableType::HighestKeys).unwrap().is_none() {
            versions.insert(TableType::HighestKeys, 1).unwrap();
            modified = true;
        }
        if modified {
            versions.flush().await.unwrap();
        }
    }

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
        .route("/forum", post(forum_post))
        .route_layer(permission_required!(
            Backend,
            login_url = "/login",
            Permission::Post
        ))
        .route("/", get(index))
        .route("/forum", get(forum))
        .route("/sse/posts", get(sse_handler))
        .route("/login", get(login))
        .route("/login", post(login_post))
        .route("/logout", get(logout_post))
        .route("/register", post(register_post))
        .route("/deploy", post(deploy_post))
        .layer(auth_layer)
        .with_state(state)
        .nest_service("/static", ServeDir::new("static"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(IndexTemplate))
}

async fn forum(
    auth: AuthSession,
    State(posts): State<Posts>,
    State(users): State<Users>,
) -> Result<impl IntoResponse> {
    let posts = posts
        .iter()
        .values()
        .map(|post| {
            let post = post?;
            let author = users.get(post.author)?.unwrap_or_default();
            let template = PostTemplate {
                body: post.body,
                author: author.username,
                avatar: author.avatar,
            };
            Ok(template.render()?)
        })
        .collect::<Result<Vec<String>>>()?
        .join("\n");
    Ok(HtmlTemplate(ForumTemplate {
        username: auth.user.as_ref().map(|user| user.username.clone()),
        posts,
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, Permission::Post).await?,
            None => false,
        },
    }))
}

async fn forum_post(
    auth: AuthSession,
    State(posts): State<Posts>,
    State(highest_keys): State<HighestKeys>,
    State(sanitizer): State<Sanitizer>,
    HxBoosted(boosted): HxBoosted,
    Form(post): Form<PostSubmission>,
) -> Result<impl IntoResponse> {
    debug!("Post created!");

    let key = highest_keys.next(TableType::Posts)?;

    let post = Post {
        key,
        author: auth.user.ok_or(Error::NotLoggedIn)?.key,
        body: sanitizer.clean(&post.body).to_string(),
    };

    posts.insert(key, post.clone())?;
    posts.flush().await?;

    if boosted {
        Ok(().into_response()) // Handled by SSE
    } else {
        Ok(Redirect::to("/forum").into_response())
    }
}

async fn sse_handler(
    State(posts): State<Posts>,
    State(users): State<Users>,
) -> Sse<impl Stream<Item = Result<Event>>> {
    debug!("SSE connection established");

    let sub = posts.watch();
    let stream = stream::unfold((sub, users), move |(mut sub, users)| async {
        (&mut sub)
            .await
            .and_then(|event| {
                let post = match event {
                    sled::Event::Insert { value, .. } => Some(value),
                    sled::Event::Remove { .. } => None,
                }?;
                let post: Post = option_ok!(BINCODE.deserialize(&post));
                let author = some_ok!(users.get(post.author).transpose());
                let template = PostTemplate {
                    body: post.body,
                    author: author.username,
                    avatar: author.avatar,
                };
                let data = option_ok!(template.render());
                let event = Event::default().data(data);
                Some(Ok(event))
            })
            .map(|event| (event, (sub, users)))
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

    Ok(Redirect::to(creds.next.as_ref().map_or("/forum", |v| v)).into_response())
}

async fn logout_post(mut auth: AuthSession) -> Result<impl IntoResponse> {
    auth.logout().await.map_err(Box::new)?;
    Ok(Redirect::to("/forum").into_response())
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

    let salt_string = SaltString::generate(&mut OsRng);
    let salt: Salt = salt_string.as_salt();
    let password: String = Argon2::default()
        .hash_password(creds.password.as_bytes(), salt)?
        .to_string();

    let user = User {
        key,
        username: creds.username.clone(),
        password,
        avatar: None,
    };
    users.insert(key, user.clone())?;
    users.flush().await?;

    auth.login(&user).await.map_err(Box::new)?;

    debug!("Registered user: {:?}", user);

    Ok(Redirect::to(creds.next.as_ref().map_or("/forum", |v| v)).into_response())
}

#[derive(Serialize, Deserialize)]
struct Deploy {
    repository: DeployRepo,
}
#[derive(Serialize, Deserialize)]
struct DeployRepo {
    name: String,
}
async fn deploy_post(Json(deploy): Json<Deploy>) -> Result<impl IntoResponse> {
    warn!("Deploying {}", deploy.repository.name);
    let dir = match deploy.repository.name.as_ref() {
        "dragon-fox.com" => "/var/www/dragon-fox.com",
        _ => return Err(Error::WrongRepo(deploy.repository.name)),
    };
    debug!(
        "{:?}",
        Command::new("eval").arg("`ssh-agent`").output().await?
    );
    debug!("{:?}", Command::new("cd").arg(dir).output().await?);
    let pull_output = Command::new("git").arg("pull").output().await?;
    debug!("{:?}", pull_output);
    debug!(
        "{:?}",
        Command::new("kill").arg("$SSH_AGENT_PID").output().await?
    );

    if is_sub(pull_output.stdout.as_ref(), b"Already up to date.") {
        return Ok("Already up to date");
    }

    debug!(
        "{:?}",
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(dir)
            .output()
            .await?
    );
    debug!(
        "{:?}",
        Command::new("systemctl")
            .arg("restart")
            .arg("dragon-fox.service")
            .output()
            .await?
    );
    Ok("Deployed")
}

fn is_sub<T: PartialEq>(haystack: &[T], needle: &[T]) -> bool {
    haystack.windows(needle.len()).any(|c| c == needle)
}
