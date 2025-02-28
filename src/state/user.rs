use std::fmt::Display;

use async_trait::async_trait;
use bincode::Options as _;
use serde::{Deserialize, Serialize};
use sled::{Db, IVec, Tree};

use crate::error::Result;
use crate::ok_some;

use super::key::{HighestKeys, Key};
use super::{DbTreeLookup, TableType, BINCODE};

#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    pub key: UserKey,
    pub username: String,
    pub password: String,
    pub avatar: Option<String>,
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// password hash.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.key)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .field("avatar", &self.avatar)
            .finish()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserKey(Key);

impl Display for UserKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone)]
pub struct Users {
    usernames: Tree,
    users: Tree,
    highest_keys: HighestKeys,
}

impl Users {
    pub fn new(usernames: Tree, users: Tree, highest_keys: HighestKeys) -> Self {
        Self {
            usernames,
            users,
            highest_keys,
        }
    }

    pub fn open(db: &Db) -> Result<Self> {
        Ok(Self::new(
            db.open_tree("usernames")?,
            db.open_tree("users")?,
            HighestKeys::open(db)?,
        ))
    }

    pub fn next_key(&self) -> Result<UserKey> {
        self.highest_keys.next(TableType::Users).map(UserKey)
    }

    pub fn get_by_username(&self, username: &String) -> Result<Option<User>> {
        let username: IVec = username.as_bytes().into();
        let key = ok_some!(self.usernames.get(username));
        let user = ok_some!(self.users.get(key));
        Ok(Some(BINCODE.deserialize(&user)?))
    }
}

#[async_trait]
impl DbTreeLookup<UserKey, User> for Users {
    fn tree(&self) -> &Tree {
        &self.users
    }

    fn insert(&self, key: UserKey, value: User) -> Result<()> {
        let key: IVec = BINCODE.serialize(&key)?.into();
        let username: IVec = value.username.as_bytes().into();
        let value: IVec = BINCODE.serialize(&value)?.into();
        self.users.insert(key.clone(), value)?;
        self.usernames.insert(username, key)?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        self.users.flush_async().await?;
        self.usernames.flush_async().await?;
        Ok(())
    }
}
