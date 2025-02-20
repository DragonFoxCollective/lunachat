use std::collections::HashSet;

use async_trait::async_trait;
use axum_login::{AuthUser, AuthnBackend, AuthzBackend, UserId};
use password_auth::verify_password;
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::ok_some;
use crate::state::key::Key;
use crate::state::user::{User, Users};
use crate::state::DbTreeLookup as _;

impl AuthUser for User {
    type Id = Key;

    fn id(&self) -> Self::Id {
        self.key
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password.as_bytes()
    }
}

#[derive(Clone)]
pub struct Backend {
    users: Users,
}

impl Backend {
    pub fn new(users: Users) -> Self {
        Self { users }
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

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = Error;

    async fn authenticate(&self, creds: Self::Credentials) -> Result<Option<Self::User>> {
        let user: Self::User = ok_some!(self.users.get_by_username(&creds.username));

        Ok(tokio::task::spawn_blocking(|| {
            if verify_password(creds.password, &user.password).is_ok() {
                Some(user)
            } else {
                None
            }
        })
        .await?)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>> {
        Ok(Some(ok_some!(self.users.get(*user_id))))
    }
}

#[async_trait]
impl AuthzBackend for Backend {
    type Permission = Permission;

    async fn get_user_permissions(&self, _user: &Self::User) -> Result<HashSet<Self::Permission>> {
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
