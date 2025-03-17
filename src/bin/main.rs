use askama::Template;
use axum::Router;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum_htmx::HxBoosted;
use axum_login::{AuthzBackend as _, permission_required};
use itertools::Itertools;
use lunachat::auth::{AuthSession, Backend, Permission};
use lunachat::error::Result;
use lunachat::state::post::PostKey;
use lunachat::state::thread::ThreadKey;
use lunachat::state::user::User;
use lunachat::templates::partial::{PostSse, ThreadSse};
use lunachat::templates::{
    HtmlTemplate, LoginPost, LogoutPost, PostPost, RegisterPost, ThreadPost,
};
use tower_http::services::ServeDir;
use tracing::debug;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("lunachat=trace")
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

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8002").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn forum(
    auth: AuthSession,
    template: lunachat::templates::ForumTemplate,
) -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(ForumTemplate {
        logged_in_user: auth.user.clone(),
        threads: template
            .threads
            .iter()
            .cloned()
            .map(|template| PartialThreadTemplate {
                key: template.key,
                title: template.title,
                body: template.body,
                author: template.author,
                sse: template.sse,
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
            key: template.key,
            title: template.title,
            body: template.body,
            author: template.author,
            sse: template.sse,
        }
        .render()?)
    })
}

pub async fn thread_post(template: ThreadPost) -> impl IntoResponse {
    debug!("Thread created!");

    Redirect::to(&format!("/thread/{}", template.0))
}

async fn thread(
    auth: AuthSession,
    template: lunachat::templates::ThreadTemplate,
) -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(ThreadTemplate {
        logged_in_user: auth.user.clone(),
        key: template.key,
        posts: template
            .posts
            .iter()
            .cloned()
            .map(|template| PartialPostTemplate {
                key: template.key,
                author: template.author,
                body: template.body,
                sse: template.sse,
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
            key: template.key,
            author: template.author,
            body: template.body,
            sse: template.sse,
        }
        .render()?)
    })
}

pub async fn post_post(HxBoosted(boosted): HxBoosted, template: PostPost) -> impl IntoResponse {
    debug!("Post created!");

    if boosted {
        ().into_response() // Handled by SSE
    } else {
        Redirect::to(&format!("/thread/{}", template.0)).into_response()
    }
}

async fn user(auth: AuthSession, template: lunachat::templates::UserTemplate) -> impl IntoResponse {
    HtmlTemplate(UserTemplate {
        logged_in_user: auth.user.clone(),
        user: template.user,
    })
}

async fn login(template: lunachat::templates::LoginTemplate) -> impl IntoResponse {
    HtmlTemplate(LoginTemplate {
        error: template.error,
        next: template.next,
    })
}

async fn login_post(template: LoginPost) -> impl IntoResponse {
    match template {
        LoginPost::Success { user, next } => {
            debug!("Logged in user: {:?}", user);
            Redirect::to(next.as_ref().map_or("/", |v| v)).into_response()
        }
        LoginPost::Failure { error, next } => HtmlTemplate(LoginTemplate {
            error: Some(error),
            next,
        })
        .into_response(),
    }
}

pub async fn logout_post(_template: LogoutPost) -> impl IntoResponse {
    Redirect::to("/").into_response()
}

async fn register_post(template: RegisterPost) -> impl IntoResponse {
    match template {
        RegisterPost::Success { user, next } => {
            debug!("Registered user: {:?}", user);
            Redirect::to(next.as_ref().map_or("/", |v| v)).into_response()
        }
        RegisterPost::Failure { error, next } => HtmlTemplate(LoginTemplate {
            error: Some(error),
            next,
        })
        .into_response(),
    }
}

#[derive(Template)]
#[template(path = "forum.html.jinja")]
pub struct ForumTemplate {
    pub logged_in_user: Option<User>,
    pub threads: String,
    pub can_post: bool,
}

#[derive(Template)]
#[template(path = "thread.html.jinja")]
pub struct ThreadTemplate {
    pub logged_in_user: Option<User>,
    pub key: ThreadKey,
    pub posts: String,
    pub can_post: bool,
}

#[derive(Template)]
#[template(path = "login.html.jinja")]
pub struct LoginTemplate {
    pub error: Option<String>,
    pub next: Option<String>,
}

#[derive(Template)]
#[template(path = "user.html.jinja")]
pub struct UserTemplate {
    pub logged_in_user: Option<User>,
    pub user: User,
}

#[derive(Template)]
#[template(path = "partial/thread.html.jinja")]
pub struct PartialThreadTemplate {
    pub key: ThreadKey,
    pub title: String,
    pub body: String,
    pub author: User,
    pub sse: bool,
}

#[derive(Template)]
#[template(path = "partial/post.html.jinja")]
pub struct PartialPostTemplate {
    pub key: PostKey,
    pub author: User,
    pub body: String,
    pub sse: bool,
}
