use std::fmt::Display;

use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use sled::Db;

use crate::error::Result;

use super::key::{HighestKeys, Key};
use super::thread::ThreadKey;
use super::user::UserKey;
use super::{DbTree, TableType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub key: PostKey,
    pub body: String,
    pub author: UserKey,
    pub parent: Option<PostKey>,
    pub children: Vec<PostKey>,
    pub thread: ThreadKey,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostKey(Key);

impl Display for PostKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Deref, DerefMut)]
pub struct Posts(DbTree<PostKey, Post>);

impl Posts {
    pub fn open(db: &Db) -> Result<Self> {
        Ok(Self(DbTree::new(
            db.open_tree("posts")?,
            HighestKeys::open(db)?,
        )))
    }

    pub fn next_key(&self) -> Result<PostKey> {
        self.1.next(TableType::Posts).map(PostKey)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PostSubmission {
    pub body: String,
}
