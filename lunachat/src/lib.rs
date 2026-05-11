use std::env;

use axum::{Extension, Router};
use axum_login::AuthManagerLayerBuilder;
use axum_login::tower_sessions::{MemoryStore, SessionManagerLayer};
use sea_orm::Database;

use crate::auth::Backend;
use crate::prelude::*;
use crate::sanitizer::Sanitizer;

pub mod auth;
pub mod entity;
pub mod prelude;
pub mod sanitizer;
pub mod templates;

pub async fn apply_middleware(router: Router) -> Result<Router> {
    // DB
    let database_url = env::var("DATABASE_URL")?;
    tracing::debug!("Connecting to database at {database_url}");
    let db: DatabaseConnection = Database::connect(database_url).await?;
    tracing::debug!("Connected to database");
    db.get_schema_registry("lunachat::entity::*")
        .sync(&db)
        .await?;

    // Session layer
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store);

    // Auth service
    let backend = Backend::new(db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    // Sanitizer
    let mut builder = ammonia::Builder::new();
    builder.add_generic_attributes(["style"]);
    let sanitizer = Sanitizer::new(builder);

    let router = router
        .layer(auth_layer)
        .layer(Extension(sanitizer))
        .layer(Extension(db));

    Ok(router)
}
