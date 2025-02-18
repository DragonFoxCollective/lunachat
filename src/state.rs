use std::sync::Arc;

use axum::extract::FromRef;
use derive_more::{Deref, DerefMut};
use sled::Tree;

#[derive(Clone)]
pub struct AppState {
    pub posts: Posts,
    pub sanitizer: Sanitizer,
}

impl FromRef<AppState> for Posts {
    fn from_ref(app_state: &AppState) -> Posts {
        app_state.posts.clone()
    }
}

impl FromRef<AppState> for Sanitizer {
    fn from_ref(app_state: &AppState) -> Sanitizer {
        app_state.sanitizer.clone()
    }
}

#[derive(Clone, Deref, DerefMut)]
pub struct Posts(pub Tree);

#[derive(Clone, Deref, DerefMut)]
pub struct Sanitizer(pub Arc<ammonia::Builder<'static>>);

impl Sanitizer {
    pub fn new(builder: ammonia::Builder<'static>) -> Self {
        Self(Arc::new(builder))
    }
}
