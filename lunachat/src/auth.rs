use std::collections::HashSet;

use async_trait::async_trait;
use axum_login::{AuthUser, AuthnBackend, AuthzBackend};
use derive_more::Display;
use password_auth::verify_password;
use return_ok::ok_some;
use serde::Deserialize;

use crate::prelude::*;

impl AuthUser for user::Model {
    type Id = user::Id;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password.as_bytes()
    }
}

#[derive(Clone)]
pub struct Backend {
    db: DatabaseConnection,
}

impl Backend {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[derive(Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Permission {
    Post,
}

#[derive(Debug, Display)]
pub struct AuthError(pub Error);

impl std::error::Error for AuthError {}

impl From<sea_orm::DbErr> for AuthError {
    fn from(err: sea_orm::DbErr) -> Self {
        AuthError(err.into())
    }
}

impl From<tokio::task::JoinError> for AuthError {
    fn from(err: tokio::task::JoinError) -> Self {
        AuthError(err.into())
    }
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = user::Model;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = ok_some!(self.db.find_user_by_username(creds.username).await);

        Ok(tokio::task::spawn_blocking(|| {
            if verify_password(creds.password, &user.password).is_ok() {
                Some(user)
            } else {
                None
            }
        })
        .await?)
    }

    async fn get_user(
        &self,
        user_id: &axum_login::UserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        Ok(Some(ok_some!(self.db.find_user(*user_id).await)))
    }
}

#[async_trait]
impl AuthzBackend for Backend {
    type Permission = Permission;

    async fn get_user_permissions(
        &self,
        _user: &Self::User,
    ) -> Result<HashSet<Self::Permission>, Self::Error> {
        let mut permissions = HashSet::new();
        permissions.insert(Permission::Post);
        Ok(permissions)
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;

// This allows us to extract the "next" field from the query string. We use this
// to redirect after log in.
#[derive(Debug, Deserialize)]
pub struct NextUrl {
    pub next: Option<String>,
}
