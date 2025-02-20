use serde::{Deserialize, Serialize};

use super::key::Key;
use super::DbTree;

#[derive(Clone, Serialize, Deserialize)]
pub struct Post {
    pub key: Key,
    pub body: String,
    pub author: Key,
}

pub type Posts = DbTree<Key, Post>;

#[derive(Clone, Serialize, Deserialize)]
pub struct PostSubmission {
    pub body: String,
}
