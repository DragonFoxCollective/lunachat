use std::fmt::Display;

use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use sled::Db;

use crate::error::Result;

use super::key::{HighestKeys, Key};
use super::post::PostKey;
use super::{DbTree, TableType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub key: ThreadKey,
    pub title: String,
    pub post: PostKey,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadKey(Key);

impl Display for ThreadKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Deref, DerefMut)]
pub struct Threads(DbTree<ThreadKey, Thread>);

impl Threads {
    pub fn open(db: &Db) -> Result<Self> {
        Ok(Self(DbTree::new(
            db.open_tree("threads")?,
            HighestKeys::open(db)?,
        )))
    }

    pub fn next_key(&self) -> Result<ThreadKey> {
        self.1.next(TableType::Threads).map(ThreadKey)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ThreadSubmission {
    pub title: String,
    pub body: String,
}
