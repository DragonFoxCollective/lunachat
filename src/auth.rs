use std::collections::HashSet;

use async_trait::async_trait;
use axum_login::{AuthUser, AuthnBackend, AuthzBackend, UserId};
use password_auth::verify_password;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ok_some;
use crate::state::{Key, Users, UsersUsernameMap};

#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    key: Key,
    pub username: String,
    password: String,
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// password hash.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.key)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .finish()
    }
}

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
    users_username_map: UsersUsernameMap,
}

impl Backend {
    pub fn new(users: Users, users_username_map: UsersUsernameMap) -> Self {
        Self {
            users,
            users_username_map,
        }
    }
}

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Permission {
    pub name: String,
}

impl From<&str> for Permission {
    fn from(name: &str) -> Self {
        Permission {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = Error;

    async fn authenticate(&self, creds: Self::Credentials) -> Result<Option<Self::User>> {
        let user: Self::User = ok_some!(self.users_username_map.get(creds.username));

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
        permissions.insert("post".into());
        Ok(permissions)
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;
