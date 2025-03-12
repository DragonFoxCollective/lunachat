use askama::Template;
use serde::{Deserialize, Serialize};

use crate::state::post::PostKey;
use crate::state::thread::ThreadKey;
use crate::state::user::User;

#[derive(Template)]
#[template(path = "partial/thread.html.jinja")]
pub struct ThreadTemplate {
    pub key: ThreadKey,
    pub title: String,
    pub body: String,
    pub num_posts: usize,
    pub author: User,
    pub sse: bool,
}

#[derive(Template, Clone, Serialize, Deserialize)]
#[template(path = "partial/post.html.jinja")]
pub struct PostTemplate {
    pub key: PostKey,
    pub author: User,
    pub body: String,
    pub sse: bool,
}
