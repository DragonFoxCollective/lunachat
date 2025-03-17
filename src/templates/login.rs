use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{Salt, SaltString};
use argon2::{Argon2, PasswordHasher};
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use axum::{Extension, Form, RequestExt as _, RequestPartsExt as _};

use crate::auth::{AuthSession, Credentials, NextUrl};
use crate::error::{Error, Result};
use crate::state::DbTreeLookup as _;
use crate::state::user::{User, Users};

pub struct LoginTemplate {
    pub error: Option<String>,
    pub next: Option<String>,
}

impl<S> FromRequestParts<S> for LoginTemplate
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Query(NextUrl { next }) = parts.extract::<Query<NextUrl>>().await?;

        Ok(LoginTemplate { error: None, next })
    }
}

pub enum LoginPost {
    Success { user: User, next: Option<String> },
    Failure { error: String, next: Option<String> },
}

impl<S> FromRequest<S> for LoginPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let mut auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| Error::AuthNotFound)?;
        let Form(creds) = req.extract::<Form<Credentials>, _>().await?;

        let user = match auth.authenticate(creds.clone()).await.map_err(Box::new)? {
            Some(user) => user,
            None => {
                return Ok(LoginPost::Failure {
                    error: "Username or password incorrect".into(),
                    next: creds.next,
                });
            }
        };

        auth.login(&user).await.map_err(Box::new)?;

        Ok(LoginPost::Success {
            user,
            next: creds.next,
        })
    }
}

pub enum RegisterPost {
    Success { user: User, next: Option<String> },
    Failure { error: String, next: Option<String> },
}

impl<S> FromRequest<S> for RegisterPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let mut auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| Error::AuthNotFound)?;
        let Extension(users) = req.extract_parts::<Extension<Users>>().await?;
        let Form(creds) = req.extract::<Form<Credentials>, _>().await?;

        if users.get_by_username(&creds.username)?.is_some() {
            return Ok(RegisterPost::Failure {
                error: "Username already taken".into(),
                next: None,
            });
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

        Ok(RegisterPost::Success {
            user,
            next: creds.next,
        })
    }
}

pub struct LogoutPost;

impl<S> FromRequest<S> for LogoutPost
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request(mut req: Request, _state: &S) -> Result<Self> {
        let mut auth = req
            .extract_parts::<AuthSession>()
            .await
            .map_err(|_| Error::AuthNotFound)?;

        auth.logout().await.map_err(Box::new)?;

        Ok(LogoutPost)
    }
}
