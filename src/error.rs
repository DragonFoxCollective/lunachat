use std::array::TryFromSliceError;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

use crate::state::post::PostKey;
use crate::state::thread::ThreadKey;
use crate::state::user::UserKey;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to parse key: {0}")]
    TryFromSlice(#[from] TryFromSliceError),
    #[error("bincode failed: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("sled failed: {0}")]
    Sled(#[from] sled::Error),
    #[error("askama failed: {0}")]
    Askama(#[from] askama::Error),
    #[error("tokio join failed: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),
    #[error("login failed: {0}")]
    Login(#[from] Box<axum_login::Error<crate::auth::Backend>>),
    #[error("password hash failed: {0}")]
    PasswordHash(#[from] argon2::password_hash::Error),
    #[error("not logged in (should never happen)")]
    NotLoggedIn,
    #[error("tried to deploy a different repo: {0}")]
    WrongRepo(String),
    #[error("io error: {0}")]
    IO(#[from] std::io::Error),
    #[error("post not found: {0}")]
    PostNotFound(PostKey),
    #[error("thread not found: {0}")]
    ThreadNotFound(ThreadKey),
    #[error("user not found: {0}")]
    UserNotFound(UserKey),
    #[error("thread has no posts: {0}")]
    ThreadHasNoPosts(ThreadKey),
    #[error("extension rejected: {0}")]
    ExtensionRejected(#[from] axum::extract::rejection::ExtensionRejection),
    #[error("path rejected: {0}")]
    PathRejected(#[from] axum::extract::rejection::PathRejection),
    #[error("query rejected: {0}")]
    QueryRejected(#[from] axum::extract::rejection::QueryRejection),
    #[error("form rejected: {0}")]
    FormRejected(#[from] axum::extract::rejection::FormRejection),
    #[error("auth not found")]
    AuthNotFound,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        error!(err = ?self, "responding with error");
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
