use axum::{Extension, Router};
use axum_login::AuthManagerLayerBuilder;
use axum_login::tower_sessions::{MemoryStore, SessionManagerLayer};

use crate::auth::Backend;
use crate::error::Result;
use crate::state::post::Posts;
use crate::state::sanitizer::Sanitizer;
use crate::state::thread::Threads;
use crate::state::user::Users;
use crate::state::{DbTreeLookup, TableType, Versions};

pub mod auth;
pub mod error;
pub mod state;
pub mod templates;
pub mod utils;
pub mod versioning;

pub async fn apply_middleware(router: Router) -> Result<Router> {
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

    let router = router
        .layer(auth_layer)
        .layer(Extension(posts))
        .layer(Extension(users))
        .layer(Extension(threads))
        .layer(Extension(sanitizer));

    Ok(router)
}
