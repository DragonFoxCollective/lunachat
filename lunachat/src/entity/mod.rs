use std::collections::{HashMap, HashSet};

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DbErr, IntoActiveModel, ModelTrait, QueryFilter};

use crate::prelude::*;

pub mod post;
pub mod thread;
pub mod user;

#[derive(Clone)]
pub enum BroadcastEvent<T> {
    Create(T),
    Update(T),
    Delete,
}

pub trait DatabaseConnectionExt {
    fn get_user(&self, id: user::Id) -> impl Future<Output = Result<user::Model>>;
    fn find_user(&self, id: user::Id) -> impl Future<Output = Result<Option<user::Model>, DbErr>>;
    fn get_user_by_username(
        &self,
        username: impl Into<String>,
    ) -> impl Future<Output = Result<user::Model>>;
    fn find_user_by_username(
        &self,
        username: impl Into<String>,
    ) -> impl Future<Output = Result<Option<user::Model>, DbErr>>;
    fn insert_user(&self, user: user::NewModel) -> impl Future<Output = Result<user::Model>>;

    fn get_post(&self, id: post::Id) -> impl Future<Output = Result<post::Model>>;
    fn get_root_post_of(&self, thread_id: thread::Id) -> impl Future<Output = Result<post::Model>>;
    fn insert_post(&self, post: post::NewModel) -> impl Future<Output = Result<post::Model>>;

    fn get_thread(&self, id: thread::Id) -> impl Future<Output = Result<thread::Model>>;
    fn get_thread_and_posts(
        &self,
        id: thread::Id,
    ) -> impl Future<
        Output = Result<(
            thread::Model,
            Vec<post::Model>,
            HashMap<user::Id, user::Model>,
        )>,
    >;
    fn insert_thread(
        &self,
        thread: thread::NewModel,
    ) -> impl Future<Output = Result<(thread::Model, post::Model)>>;
}

impl DatabaseConnectionExt for DatabaseConnection {
    async fn get_user(&self, id: user::Id) -> Result<user::Model> {
        Ok(user::Entity::find_by_id(id)
            .one(self)
            .await?
            .ok_or(anyhow!("User {id} not found"))?)
    }

    async fn find_user(&self, id: user::Id) -> Result<Option<user::Model>, DbErr> {
        user::Entity::find_by_id(id).one(self).await
    }

    async fn get_user_by_username(&self, username: impl Into<String>) -> Result<user::Model> {
        let username = username.into();
        Ok(user::Entity::find_by_username(username.clone())
            .one(self)
            .await?
            .ok_or(anyhow!("User with username {username} not found"))?)
    }

    async fn insert_user(&self, user: user::NewModel) -> Result<user::Model> {
        Ok(user.into_active_model().insert(self).await?)
    }

    async fn find_user_by_username(
        &self,
        username: impl Into<String>,
    ) -> Result<Option<user::Model>, DbErr> {
        user::Entity::find_by_username(username).one(self).await
    }

    async fn get_post(&self, id: post::Id) -> Result<post::Model> {
        Ok(post::Entity::find_by_id(id)
            .one(self)
            .await?
            .ok_or(anyhow!("Post {id} not found"))?)
    }

    async fn get_root_post_of(&self, thread_id: thread::Id) -> Result<post::Model> {
        Ok(post::Entity::find()
            .filter(post::Column::ThreadId.eq(thread_id))
            .filter(post::Column::ParentId.is_null())
            .one(self)
            .await?
            .ok_or(anyhow!("Thread {thread_id} has no root post"))?)
    }

    async fn insert_post(&self, post: post::NewModel) -> Result<post::Model> {
        Ok(post
            .into_active_model()
            .into_active_model()
            .insert(self)
            .await?)
    }

    async fn get_thread(&self, id: thread::Id) -> Result<thread::Model> {
        Ok(thread::Entity::find_by_id(id)
            .one(self)
            .await?
            .ok_or(anyhow!("Thread {id} not found"))?)
    }

    async fn get_thread_and_posts(
        &self,
        id: thread::Id,
    ) -> Result<(
        thread::Model,
        Vec<post::Model>,
        HashMap<user::Id, user::Model>,
    )> {
        let thread = thread::Entity::find_by_id(id)
            .one(self)
            .await?
            .ok_or(anyhow!("Thread {id} not found"))?;
        let posts = thread.find_related(post::Entity).all(self).await?;
        let authors = posts
            .iter()
            .map(|post| post.author_id)
            .collect::<HashSet<_>>();
        let authors = user::Entity::find()
            .filter(user::Column::Id.is_in(authors))
            .all(self)
            .await?
            .into_iter()
            .map(|author| (author.id, author))
            .collect::<HashMap<_, _>>();
        Ok((thread, posts, authors))
    }

    async fn insert_thread(
        &self,
        thread: thread::NewModel,
    ) -> Result<(thread::Model, post::Model)> {
        let thread::NewModel {
            title,
            body,
            author_id,
        } = thread;
        let thread = thread::ActiveModel {
            id: NotSet,
            title: Set(title),
        }
        .insert(self)
        .await?;
        let post = post::NewModel {
            body,
            author_id,
            thread_id: thread.id,
            parent_id: None,
        }
        .into_active_model()
        .insert(self)
        .await?;
        Ok((thread, post))
    }
}
