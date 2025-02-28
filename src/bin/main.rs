use std::collections::HashSet;
use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{Salt, SaltString};
use argon2::{Argon2, PasswordHasher};
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Redirect, Sse};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use axum_htmx::HxBoosted;
use axum_login::tower_sessions::{MemoryStore, SessionManagerLayer};
use axum_login::{AuthManagerLayerBuilder, AuthzBackend, permission_required};
use bincode::Options as _;
use dragon_fox::auth::{AuthSession, Backend, Credentials, NextUrl, Permission};
use dragon_fox::error::{Error, Result};
use dragon_fox::some_or_continue;
use dragon_fox::state::post::{Post, PostSubmission, Posts};
use dragon_fox::state::sanitizer::Sanitizer;
use dragon_fox::state::thread::{Thread, ThreadKey, ThreadSubmission, Threads};
use dragon_fox::state::user::{User, Users};
use dragon_fox::state::{AppState, BINCODE, DbTreeLookup, TableType, Versions};
use dragon_fox::templates::{
    ForumTemplate, HtmlTemplate, IndexTemplate, LoginTemplate, PostTemplate, ThreadIndexTemplate,
    ThreadTemplate,
};
use futures::{Stream, stream};
use serde::{Deserialize, Serialize};
use sled::Subscriber;
use tokio::process::Command;
use tower_http::services::ServeDir;
use tracing::{debug, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("dragon_fox=trace")
        .init();

    // DB
    let db = sled::open("db")?;
    let posts = Posts::open(&db)?;
    let users = Users::open(&db)?;
    let threads = Threads::open(&db)?;

    // Versioning
    {
        let versions = Versions::open(&db)?;
        let mut modified = false;
        if versions.get(TableType::Posts)?.is_none() {
            versions.insert(TableType::Posts, 1)?;
            modified = true;
        }
        if versions.get(TableType::Users)?.is_none() {
            versions.insert(TableType::Users, 1)?;
            modified = true;
        }
        if versions.get(TableType::HighestKeys)?.is_none() {
            versions.insert(TableType::HighestKeys, 1)?;
            modified = true;
        }
        if versions.get(TableType::Threads)?.is_none() {
            versions.insert(TableType::Threads, 1)?;
            modified = true;
        }
        if modified {
            versions.flush().await?;
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
        sanitizer,
        threads,
    };

    let app = Router::new()
        .route("/forum", post(thread_post))
        .route("/forum/thread/{thread_key}", post(post_post))
        .route_layer(permission_required!(
            Backend,
            login_url = "/login",
            Permission::Post
        ))
        .route("/", get(index))
        .route("/forum", get(forum))
        .route("/forum/thread/{thread_key}", get(thread))
        .route("/sse/threads", get(sse_threads))
        .route("/sse/posts/{thread_key}", get(sse_posts))
        .route("/login", get(login))
        .route("/login", post(login_post))
        .route("/logout", get(logout_post))
        .route("/register", post(register_post))
        .route("/deploy", post(deploy_post))
        .layer(auth_layer)
        .with_state(state)
        .nest_service("/static", ServeDir::new("static"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index() -> Result<impl IntoResponse> {
    Ok(HtmlTemplate(IndexTemplate))
}

async fn forum(
    auth: AuthSession,
    State(threads): State<Threads>,
    State(posts): State<Posts>,
    State(users): State<Users>,
) -> Result<impl IntoResponse> {
    let threads = threads
        .iter()
        .values()
        .map(|thread| {
            let thread = thread?;
            let post = posts
                .get(thread.post)?
                .ok_or(Error::PostNotFound(thread.post))?;
            let num_posts = posts
                .iter()
                .values()
                .filter_map(|post| post.ok())
                .filter(|post| post.thread == thread.key)
                .count();
            let author = users
                .get(post.author)?
                .ok_or(Error::UserNotFound(post.author))?;
            let template = ThreadIndexTemplate {
                key: thread.key,
                title: thread.title,
                body: post.body,
                num_posts,
                author: author.username,
                sse: false,
            };
            Ok(template.render()?)
        })
        .collect::<Result<Vec<String>>>()?
        .join("\n");
    Ok(HtmlTemplate(ForumTemplate {
        username: auth.user.as_ref().map(|user| user.username.clone()),
        threads,
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, Permission::Post).await?,
            None => false,
        },
    }))
}

async fn thread_post(
    auth: AuthSession,
    State(threads): State<Threads>,
    State(posts): State<Posts>,
    State(sanitizer): State<Sanitizer>,
    Form(thread): Form<ThreadSubmission>,
) -> Result<impl IntoResponse> {
    debug!("Thread created!");

    let thread_key = threads.next_key()?;
    let post_key = posts.next_key()?;

    let post = Post {
        key: post_key,
        author: auth.user.ok_or(Error::NotLoggedIn)?.key,
        body: sanitizer.clean(&thread.body).to_string(),
        parent: None,
        children: vec![],
        thread: thread_key,
    };
    posts.insert(post_key, post.clone())?;
    posts.flush().await?;

    let thread = Thread {
        key: thread_key,
        title: sanitizer.clean(&thread.title).to_string(),
        post: post_key,
    };
    threads.insert(thread_key, thread.clone())?;
    threads.flush().await?;

    Ok(Redirect::to(&format!("/forum/thread/{}", thread_key)).into_response())
}

async fn thread(
    auth: AuthSession,
    State(threads): State<Threads>,
    State(posts): State<Posts>,
    State(users): State<Users>,
    Path(thread_key): Path<ThreadKey>,
) -> Result<impl IntoResponse> {
    let thread = threads
        .get(thread_key)?
        .ok_or(Error::ThreadNotFound(thread_key))?;
    let posts = {
        let mut posts_visited = HashSet::new();
        let mut posts_visited_in_order = vec![];
        let mut posts_to_visit = vec![thread.post];
        while let Some(post_key) = posts_to_visit.pop() {
            if posts_visited.contains(&post_key) {
                continue;
            }
            posts_visited.insert(post_key);
            posts_visited_in_order.push(post_key);
            let post = posts.get(post_key)?.ok_or(Error::PostNotFound(post_key))?;
            posts_to_visit.extend(post.children.iter().copied());
        }
        posts_visited_in_order
            .iter()
            .map(|key| match posts.get(*key) {
                Ok(Some(post)) => Ok(post),
                Ok(None) => Err(Error::PostNotFound(*key)),
                Err(e) => Err(e),
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|post| {
                let author = users
                    .get(post.author)?
                    .ok_or(Error::UserNotFound(post.author))?;
                let template = PostTemplate {
                    key: post.key,
                    body: post.body,
                    author: author.username,
                    avatar: author.avatar,
                    sse: false,
                };
                Ok(template.render()?)
            })
            .collect::<Result<Vec<String>>>()?
            .join("\n")
    };
    Ok(HtmlTemplate(ThreadTemplate {
        key: thread_key,
        username: auth.user.as_ref().map(|user| user.username.clone()),
        posts,
        can_post: match auth.user {
            Some(user) => auth.backend.has_perm(&user, Permission::Post).await?,
            None => false,
        },
    }))
}

async fn post_post(
    auth: AuthSession,
    State(posts): State<Posts>,
    State(sanitizer): State<Sanitizer>,
    HxBoosted(boosted): HxBoosted,
    Path(thread_key): Path<ThreadKey>,
    Form(post): Form<PostSubmission>,
) -> Result<impl IntoResponse> {
    debug!("Post created!");

    let key = posts.next_key()?;
    let parent_key = posts
        .iter()
        .values()
        .filter_map(|post| post.ok())
        .filter(|post| post.thread == thread_key)
        .last()
        .ok_or(Error::ThreadHasNoPosts(thread_key))?
        .key;
    let thread_key = posts
        .get(parent_key)?
        .ok_or(Error::PostNotFound(parent_key))?
        .thread;

    let post = Post {
        key,
        author: auth.user.ok_or(Error::NotLoggedIn)?.key,
        body: sanitizer.clean(&post.body).to_string(),
        parent: Some(parent_key),
        children: vec![],
        thread: thread_key,
    };
    posts.insert(key, post.clone())?;

    let mut parent = posts
        .get(parent_key)?
        .ok_or(Error::PostNotFound(parent_key))?;
    parent.children.push(key);
    posts.insert(parent_key, parent)?;

    posts.flush().await?;

    if boosted {
        Ok(().into_response()) // Handled by SSE
    } else {
        Ok(Redirect::to("/forum").into_response())
    }
}

async fn sse_threads(
    State(threads): State<Threads>,
    State(posts): State<Posts>,
    State(users): State<Users>,
) -> Sse<impl Stream<Item = Result<Event>>> {
    debug!("SSE connection established");

    async fn get_valid_single(
        mut sub: &mut Subscriber,
        posts: &Posts,
        users: &Users,
    ) -> Result<Event> {
        loop {
            let event = some_or_continue!((&mut sub).await);
            let thread = match event {
                sled::Event::Insert { value, .. } => value,
                sled::Event::Remove { .. } => continue,
            };
            let thread: Thread = BINCODE.deserialize(&thread)?;
            let root_post = posts
                .get(thread.post)?
                .ok_or(Error::PostNotFound(thread.post))?;
            let num_posts = posts
                .iter()
                .values()
                .filter_map(|post| post.ok())
                .filter(|post| post.thread == thread.key)
                .count();
            let author = users
                .get(root_post.author)?
                .ok_or(Error::UserNotFound(root_post.author))?;
            let template = ThreadIndexTemplate {
                key: thread.key,
                title: thread.title,
                body: root_post.body,
                num_posts,
                author: author.username,
                sse: true,
            };
            let data = template.render()?;
            let event = Event::default().data(data);
            return Ok(event);
        }
    }

    let sub = threads.watch();
    let stream = stream::unfold((sub, posts, users), async move |(mut sub, posts, users)| {
        Some((
            get_valid_single(&mut sub, &posts, &users).await,
            (sub, posts, users),
        ))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-text"),
    )
}

async fn sse_posts(
    State(posts): State<Posts>,
    State(users): State<Users>,
    Path(thread_key): Path<ThreadKey>,
) -> Sse<impl Stream<Item = Result<Event>>> {
    debug!("SSE connection established");

    async fn get_valid_single(
        mut sub: &mut Subscriber,
        users: &Users,
        thread_key: ThreadKey,
    ) -> Result<Event> {
        loop {
            let event = some_or_continue!((&mut sub).await);
            let post = match event {
                sled::Event::Insert { value, .. } => value,
                sled::Event::Remove { .. } => continue,
            };
            let post: Post = BINCODE.deserialize(&post)?;
            if post.thread != thread_key {
                continue;
            }
            let author = users
                .get(post.author)?
                .ok_or(Error::UserNotFound(post.author))?;
            let template = PostTemplate {
                key: post.key,
                body: post.body,
                author: author.username,
                avatar: author.avatar,
                sse: true,
            };
            let data = template.render()?;
            let event = Event::default().data(data);
            return Ok(event);
        }
    }

    let sub = posts.watch();
    let stream = stream::unfold(
        (sub, users, thread_key),
        async move |(mut sub, users, thread_key)| {
            Some((
                get_valid_single(&mut sub, &users, thread_key).await,
                (sub, users, thread_key),
            ))
        },
    );

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
            .into_response());
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

    let key = users.next_key()?;

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
/// No idea if this *actually* works
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
