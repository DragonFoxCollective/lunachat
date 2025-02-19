use std::fmt::Display;
use std::marker::PhantomData;
use std::sync::Arc;

use axum::extract::FromRef;
use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use sled::{IVec, Tree};

use crate::auth::User;
use crate::error::{Error, Result};
use crate::templates::PostTemplate;
use crate::{ok_some, some_ok};

#[derive(Clone)]
pub struct AppState {
    pub posts: Posts,
    pub users: Users,
    pub highest_keys: HighestKeys,
    pub sanitizer: Sanitizer,
}

impl FromRef<AppState> for Posts {
    fn from_ref(app_state: &AppState) -> Posts {
        app_state.posts.clone()
    }
}

impl FromRef<AppState> for Users {
    fn from_ref(app_state: &AppState) -> Users {
        app_state.users.clone()
    }
}

impl FromRef<AppState> for HighestKeys {
    fn from_ref(app_state: &AppState) -> HighestKeys {
        app_state.highest_keys.clone()
    }
}

impl FromRef<AppState> for Sanitizer {
    fn from_ref(app_state: &AppState) -> Sanitizer {
        app_state.sanitizer.clone()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key(u64);

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<IVec> for Key {
    type Error = Error;

    fn try_from(value: IVec) -> Result<Self> {
        let bytes: [u8; 8] = value.as_ref().try_into()?;
        Ok(Self(u64::from_be_bytes(bytes)))
    }
}

impl From<Key> for IVec {
    fn from(key: Key) -> Self {
        key.0.to_be_bytes().as_ref().into()
    }
}

pub trait DbTreeLookup<Key, Value>
where
    Key: for<'a> Deserialize<'a> + Serialize,
    Value: for<'a> Deserialize<'a> + Serialize,
{
    fn tree(&self) -> &Tree;

    fn get(&self, key: Key) -> Result<Option<Value>> {
        let key: IVec = bincode::serialize(&key)?.into();
        let item = ok_some!(self.tree().get(key));
        Ok(Some(bincode::deserialize(&item)?))
    }

    fn iter(&self) -> DbTreeIter<Key, Value> {
        DbTreeIter(self.tree().iter(), PhantomData, PhantomData)
    }

    fn watch(&self) -> sled::Subscriber {
        self.tree().watch_prefix([])
    }

    fn insert(&self, key: Key, value: Value) -> Result<()> {
        let key: IVec = bincode::serialize(&key)?.into();
        self.tree().insert(key, bincode::serialize(&value)?)?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        self.tree().flush_async().await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct DbTree<Key, Value>(Tree, PhantomData<Key>, PhantomData<Value>);

impl<Key, Value> DbTree<Key, Value> {
    pub fn new(tree: Tree) -> Self {
        Self(tree, PhantomData, PhantomData)
    }
}

impl<Key, Value> DbTreeLookup<Key, Value> for DbTree<Key, Value>
where
    Key: for<'a> Deserialize<'a> + Serialize,
    Value: for<'a> Deserialize<'a> + Serialize,
{
    fn tree(&self) -> &Tree {
        &self.0
    }
}

pub struct DbTreeIter<Key, Value>(sled::Iter, PhantomData<Key>, PhantomData<Value>);

impl<Key, Value> DbTreeIter<Key, Value>
where
    Key: for<'a> Deserialize<'a>,
    Value: for<'a> Deserialize<'a>,
{
    #[allow(dead_code)]
    pub fn keys(self) -> impl DoubleEndedIterator<Item = Result<Key>> {
        self.map(|r| r.map(|(k, _v)| k))
    }

    #[allow(dead_code)]
    pub fn values(self) -> impl DoubleEndedIterator<Item = Result<Value>> {
        self.map(|r| r.map(|(_k, v)| v))
    }
}

impl<Key, Value> Iterator for DbTreeIter<Key, Value>
where
    Key: for<'a> Deserialize<'a>,
    Value: for<'a> Deserialize<'a>,
{
    type Item = Result<(Key, Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = some_ok!(self.0.next());
        Some(Ok((
            bincode::deserialize(&key).unwrap(),
            bincode::deserialize(&value).unwrap(),
        )))
    }
}

impl<Key, Value> DoubleEndedIterator for DbTreeIter<Key, Value>
where
    Key: for<'a> Deserialize<'a>,
    Value: for<'a> Deserialize<'a>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let (key, value) = some_ok!(self.0.next_back());
        Some(Ok((
            bincode::deserialize(&key).unwrap(),
            bincode::deserialize(&value).unwrap(),
        )))
    }
}

pub type Posts = DbTree<Key, PostTemplate>;

#[derive(Clone)]
pub struct Users {
    usernames: Tree,
    users: Tree,
}

impl Users {
    pub fn new(usernames: Tree, users: Tree) -> Self {
        Self { usernames, users }
    }

    pub fn get_by_username(&self, username: &String) -> Result<Option<User>> {
        let username: IVec = username.as_bytes().into();
        let key = ok_some!(self.usernames.get(username));
        let user = ok_some!(self.users.get(key));
        Ok(Some(bincode::deserialize(&user)?))
    }
}

impl DbTreeLookup<Key, User> for Users {
    fn tree(&self) -> &Tree {
        &self.users
    }

    fn insert(&self, key: Key, value: User) -> Result<()> {
        let key: IVec = bincode::serialize(&key)?.into();
        let username: IVec = value.username.as_bytes().into();
        let value: IVec = bincode::serialize(&value)?.into();
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

#[derive(Clone)]
pub struct HighestKeys(Tree);

impl HighestKeys {
    pub fn new(tree: Tree) -> Self {
        Self(tree)
    }

    pub fn next(&self, table: TableType) -> Result<Key> {
        let table: IVec = bincode::serialize(&table)?.into();
        let key = self.0.fetch_and_update(table, |key| {
            let key = key.map_or(0u64, |key| bincode::deserialize(key).unwrap()) + 1;
            let key: IVec = bincode::serialize(&key).unwrap().into();
            Some(key)
        })?;
        let key = key
            .map(|key| bincode::deserialize(&key))
            .unwrap_or(Ok(0u64))?;
        Ok(Key(key))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TableType {
    Posts,
    Users,
}

#[derive(Clone, Deref, DerefMut)]
pub struct Sanitizer(pub Arc<ammonia::Builder<'static>>);

impl Sanitizer {
    pub fn new(builder: ammonia::Builder<'static>) -> Self {
        Self(Arc::new(builder))
    }
}
