use askama::Template;
use awesome_axum_responses::*;
use axum::extract::FromRequestParts;
use axum::http::Uri;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{RequestPartsExt, Router};
use axum_htmx::HxBoosted;
use axum_login::{AuthzBackend as _, permission_required};
use itertools::Itertools;
use lunachat::auth::{AuthSession, Backend, Permission};
use lunachat::prelude::*;
use lunachat::templates::partial::{PostSse, ThreadSse};
use lunachat::templates::{
    ForumGet, LoginGet, LoginPost, LogoutPost, PostPost, RegisterPost, ThreadGet, ThreadPost,
    UserGet,
};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("debug,lunachat=trace,main=trace,sqlx=warn")
        .init();

    let app = Router::new()
        .route("/thread", post(thread_post))
        .route("/thread/{thread_key}", post(post_post))
        .route_layer(permission_required!(
            Backend,
            login_url = "/login",
            Permission::Post
        ))
        .route("/", get(forum))
        .route("/sse", get(forum_sse))
        .route("/thread/{thread_key}", get(thread))
        .route("/thread/{thread_key}/sse", get(thread_sse))
        .route("/user/{user_key}", get(user))
        .route("/login", get(login))
        .route("/login", post(login_post))
        .route("/logout", get(logout_post))
        .route("/register", post(register_post))
        .nest_service("/static", ServeDir::new("static"));
    let app = lunachat::apply_middleware(app).await?;

    let listener = tokio::net::TcpListener::bind("0.0.0.0:80").await?;
    tracing::info!("Lunachat started!");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn forum(
    logged_in: LoggedIn,
    auth: AuthSession,
    forum: ForumGet,
) -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(ForumTemplate {
        logged_in,
        threads: forum
            .threads
            .iter()
            .cloned()
            .map(|template| PartialThreadTemplate {
                thread: template.thread,
                post: template.post,
                author: template.author,
                sse: false,
            })
            .join("\n"),
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, Permission::Post).await?,
            None => false,
        },
    }))
}

async fn forum_sse(sse: ThreadSse) -> impl IntoResponse {
    sse.into_sse(|template| {
        Ok(PartialThreadTemplate {
            thread: template.thread,
            post: template.post,
            author: template.author,
            sse: true,
        }
        .render()?)
    })
}

pub async fn thread_post(thread: ThreadPost) -> impl IntoResponse {
    tracing::debug!("Thread created!");

    Redirect::to(&format!("/thread/{}", thread.0))
}

async fn thread(
    logged_in: LoggedIn,
    auth: AuthSession,
    thread: ThreadGet,
) -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(ThreadTemplate {
        logged_in,
        thread: thread.thread,
        posts: thread
            .posts
            .iter()
            .cloned()
            .map(|template| PartialPostTemplate {
                post: template.post,
                author: template.author,
                sse: false,
            })
            .join("\n"),
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, Permission::Post).await?,
            None => false,
        },
    }))
}

async fn thread_sse(sse: PostSse) -> impl IntoResponse {
    sse.into_sse(|template| {
        Ok(PartialPostTemplate {
            post: template.post,
            author: template.author,
            sse: true,
        }
        .render()?)
    })
}

pub async fn post_post(HxBoosted(boosted): HxBoosted, post: PostPost) -> impl IntoResponse {
    tracing::debug!("Post created!");

    if boosted {
        ().into_response() // Handled by SSE
    } else {
        Redirect::to(&format!("/thread/{}", post.1)).into_response()
    }
}

async fn user(logged_in: LoggedIn, user: UserGet) -> impl IntoResponse {
    HtmlTemplate(UserTemplate {
        logged_in,
        user: user.user,
    })
}

async fn login(login: LoginGet) -> impl IntoResponse {
    HtmlTemplate(LoginTemplate {
        login_error: login.error,
        next: login.next,
    })
}

async fn login_post(login: LoginPost) -> impl IntoResponse {
    match login {
        LoginPost::Success { user, next } => {
            tracing::debug!("Logged in user: {:?}", user);
            Redirect::to(next.as_ref().map_or("/", |v| v)).into_response()
        }
        LoginPost::Failure { error, next } => HtmlTemplate(LoginTemplate {
            login_error: Some(error),
            next,
        })
        .into_response(),
    }
}

pub async fn logout_post(_logout: LogoutPost) -> impl IntoResponse {
    Redirect::to("/").into_response()
}

async fn register_post(register: RegisterPost) -> impl IntoResponse {
    match register {
        RegisterPost::Success { user, next } => {
            tracing::debug!("Registered user: {:?}", user);
            Redirect::to(next.as_ref().map_or("/", |v| v)).into_response()
        }
        RegisterPost::Failure { error, next } => HtmlTemplate(LoginTemplate {
            login_error: Some(error),
            next,
        })
        .into_response(),
    }
}

enum LoggedIn {
    Yes {
        user: user::Model,
    },
    No {
        url: String,
        login_error: Option<String>,
    },
}

impl<S> FromRequestParts<S> for LoggedIn
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let auth = parts
            .extract::<AuthSession>()
            .await
            .map_err(|_| anyhow!("Auth not found"))?;
        if let Some(user) = auth.user {
            Ok(LoggedIn::Yes { user })
        } else {
            Ok(LoggedIn::No {
                url: parts.extract::<Uri>().await?.to_string(),
                login_error: None,
            })
        }
    }
}

#[derive(Template)]
#[template(path = "forum.html.jinja")]
struct ForumTemplate {
    logged_in: LoggedIn,
    threads: String,
    can_post: bool,
}

#[derive(Template)]
#[template(path = "thread.html.jinja")]
struct ThreadTemplate {
    logged_in: LoggedIn,
    thread: thread::Model,
    posts: String,
    can_post: bool,
}

#[derive(Template)]
#[template(path = "login.html.jinja")]
struct LoginTemplate {
    login_error: Option<String>,
    next: Option<String>,
}

#[derive(Template)]
#[template(path = "user.html.jinja")]
struct UserTemplate {
    logged_in: LoggedIn,
    user: user::Model,
}

#[derive(Template)]
#[template(path = "partial/thread.html.jinja")]
struct PartialThreadTemplate {
    thread: thread::Model,
    post: post::Model,
    author: user::Model,
    sse: bool,
}

#[derive(Template)]
#[template(path = "partial/post.html.jinja")]
struct PartialPostTemplate {
    post: post::Model,
    author: user::Model,
    sse: bool,
}
