use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use sled::Db;

use crate::error::Result;

use super::key::{HighestKeys, Key};
use super::{DbTree, TableType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub key: Key,
    pub title: String,
    pub post: Key,
}

#[derive(Clone, Deref, DerefMut)]
pub struct Threads(DbTree<Key, Thread>);

impl Threads {
    pub fn open(db: &Db) -> Result<Self> {
        Ok(Self(DbTree::new(
            db.open_tree("threads")?,
            HighestKeys::open(db)?,
        )))
    }

    pub fn next_key(&self) -> Result<Key> {
        self.1.next(TableType::Threads)
    }
}
