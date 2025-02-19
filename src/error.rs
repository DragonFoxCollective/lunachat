use std::array::TryFromSliceError;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

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
    #[error("tokio failed: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        error!(err = ?self, "responding with error");
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
