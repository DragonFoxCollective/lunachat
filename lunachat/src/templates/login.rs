use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{Salt, SaltString};
use argon2::{Argon2, PasswordHasher};
use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use axum::{Extension, Form, RequestExt as _, RequestPartsExt as _};

use crate::auth::{AuthSession, Credentials, NextUrl};
use crate::prelude::*;

pub struct LoginGet {
    pub error: Option<String>,
    pub next: Option<String>,
}

impl<S> FromRequestParts<S> for LoginGet
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let Query(NextUrl { next }) = parts.extract::<Query<NextUrl>>().await?;

        Ok(LoginGet { error: None, next })
    }
}

pub enum LoginPost {
    Success {
        user: user::Model,
        next: Option<String>,
    },
    Failure {
        error: String,
        next: Option<String>,
    },
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
            .map_err(|_| anyhow!("Auth not found"))?;
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
    Success {
        user: user::Model,
        next: Option<String>,
    },
    Failure {
        error: String,
        next: Option<String>,
    },
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
            .map_err(|_| anyhow!("Auth not found"))?;
        let Extension(db) = req.extract_parts::<Extension<DatabaseConnection>>().await?;
        let Form(creds) = req.extract::<Form<Credentials>, _>().await?;

        if db.find_user_by_username(&creds.username).await?.is_some() {
            return Ok(RegisterPost::Failure {
                error: "Username already taken".into(),
                next: None,
            });
        }

        let salt_string = SaltString::generate(&mut OsRng);
        let salt: Salt = salt_string.as_salt();
        let password: String = Argon2::default()
            .hash_password(creds.password.as_bytes(), salt)?
            .to_string();

        let user = user::ActiveModel::builder()
            .set_username(creds.username.clone())
            .set_password(password)
            .insert(&db)
            .await?
            .into();

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
            .map_err(|_| anyhow!("Auth not found"))?;

        auth.logout().await.map_err(Box::new)?;

        Ok(LogoutPost)
    }
}
