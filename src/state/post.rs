use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use sled::Db;

use crate::error::Result;

use super::key::{HighestKeys, Key};
use super::{DbTree, TableType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub key: Key,
    pub body: String,
    pub author: Key,
    pub parent: Option<Key>,
    pub children: Vec<Key>,
}

#[derive(Clone, Deref, DerefMut)]
pub struct Posts(DbTree<Key, Post>);

impl Posts {
    pub fn open(db: &Db) -> Result<Self> {
        Ok(Self(DbTree::new(
            db.open_tree("posts")?,
            HighestKeys::open(db)?,
        )))
    }

    pub fn next_key(&self) -> Result<Key> {
        self.1.next(TableType::Posts)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PostSubmission {
    pub body: String,
}
