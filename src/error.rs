use core::error;
use std::array::TryFromSliceError;
use std::fmt::{self, Display, Formatter};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

#[derive(Debug)]
pub enum Error {
    TryFromSlice(TryFromSliceError),
    Bincode(bincode::Error),
    Sled(sled::Error),
    Askama(askama::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::TryFromSlice(err) => write!(f, "Failed to convert ID to slice: {}", err),
            Error::Bincode(err) => {
                write!(f, "Failed to serialize or deserialize data: {}", err)
            }
            Error::Sled(err) => write!(f, "Failed to interact with the database: {}", err),
            Error::Askama(err) => write!(f, "Failed to render template: {}", err),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::TryFromSlice(err) => Some(err),
            Error::Bincode(err) => Some(err),
            Error::Sled(err) => Some(err),
            Error::Askama(err) => Some(err),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        error!(err = ?self, "responding with error");
        let body = self.to_string();
        (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}

impl From<TryFromSliceError> for Error {
    fn from(err: TryFromSliceError) -> Self {
        Error::TryFromSlice(err)
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Error::Bincode(err)
    }
}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        Error::Sled(err)
    }
}

impl From<askama::Error> for Error {
    fn from(err: askama::Error) -> Self {
        Error::Askama(err)
    }
}
