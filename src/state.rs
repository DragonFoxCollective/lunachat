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
    pub users_username_map: UsersUsernameMap,
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

impl FromRef<AppState> for UsersUsernameMap {
    fn from_ref(app_state: &AppState) -> UsersUsernameMap {
        app_state.users_username_map.clone()
    }
}

impl FromRef<AppState> for Sanitizer {
    fn from_ref(app_state: &AppState) -> Sanitizer {
        app_state.sanitizer.clone()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key(u64);

impl Key {
    pub fn incremented(&self) -> Self {
        Self(self.0 + 1)
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Key {
    fn from(id: u64) -> Self {
        Self(id)
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

#[derive(Clone)]
pub struct DbTree<Item>(Tree, PhantomData<Item>);

impl<Item> DbTree<Item>
where
    Item: for<'a> Deserialize<'a> + Serialize,
{
    pub fn new(tree: Tree) -> Self {
        Self(tree, PhantomData)
    }

    pub fn get(&self, key: Key) -> Result<Option<Item>> {
        let item = ok_some!(self.0.get::<IVec>(key.into()));
        Ok(Some(bincode::deserialize(&item)?))
    }

    pub fn iter(&self) -> DbTreeIter<Item> {
        DbTreeIter(self.0.iter(), PhantomData)
    }

    pub fn watch(&self) -> sled::Subscriber {
        self.0.watch_prefix([])
    }

    pub fn insert(&self, key: Key, value: Item) -> Result<()> {
        self.0
            .insert(IVec::from(key), bincode::serialize(&value)?)?;
        Ok(())
    }

    pub async fn flush_async(&self) -> Result<()> {
        self.0.flush_async().await?;
        Ok(())
    }

    pub fn last(&self) -> Result<Option<(Key, Item)>> {
        let (key, value) = ok_some!(self.0.last());
        Ok(Some((key.try_into()?, bincode::deserialize(&value)?)))
    }
}

impl<Item> IntoIterator for &'_ DbTree<Item>
where
    Item: for<'a> Deserialize<'a> + Serialize,
{
    type Item = Result<(Key, Item)>;
    type IntoIter = DbTreeIter<Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct DbTreeIter<Item>(sled::Iter, PhantomData<Item>);

impl<Item> DbTreeIter<Item>
where
    Item: for<'a> Deserialize<'a>,
{
    pub fn keys(self) -> impl DoubleEndedIterator<Item = Result<Key>> {
        self.map(|r| r.map(|(k, _v)| k))
    }

    pub fn values(self) -> impl DoubleEndedIterator<Item = Result<Item>> {
        self.map(|r| r.map(|(_k, v)| v))
    }
}

impl<Item> Iterator for DbTreeIter<Item>
where
    Item: for<'a> Deserialize<'a>,
{
    type Item = Result<(Key, Item)>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = some_ok!(self.0.next());
        Some(Ok((
            key.try_into().unwrap(),
            bincode::deserialize(&value).unwrap(),
        )))
    }
}

impl<Item> DoubleEndedIterator for DbTreeIter<Item>
where
    Item: for<'a> Deserialize<'a>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let (key, value) = some_ok!(self.0.next_back());
        Some(Ok((
            key.try_into().unwrap(),
            bincode::deserialize(&value).unwrap(),
        )))
    }
}

pub type Posts = DbTree<PostTemplate>;
pub type Users = DbTree<User>;

#[derive(Clone)]
pub struct UsersUsernameMap {
    usernames: Tree,
    users: Tree,
}

impl UsersUsernameMap {
    pub fn new(usernames: Tree, users: Tree) -> Self {
        Self { usernames, users }
    }

    pub fn get(&self, username: &String) -> Result<Option<User>> {
        let id = ok_some!(self.usernames.get(username));
        let user_data = ok_some!(self.users.get(id));
        let user = bincode::deserialize(&user_data)?;
        Ok(Some(user))
    }

    pub fn insert(&self, user: User) -> Result<()> {
        self.usernames.insert(user.username.clone(), user.key)?;
        let user_data = bincode::serialize(&user)?;
        self.users.insert(IVec::from(user.key), user_data.clone())?;
        Ok(())
    }

    pub async fn flush_async(&self) -> Result<()> {
        self.usernames.flush_async().await?;
        self.users.flush_async().await?;
        Ok(())
    }
}

#[derive(Clone, Deref, DerefMut)]
pub struct Sanitizer(pub Arc<ammonia::Builder<'static>>);

impl Sanitizer {
    pub fn new(builder: ammonia::Builder<'static>) -> Self {
        Self(Arc::new(builder))
    }
}
