pub use forum::ForumGet;
pub use login::{LoginGet, LoginPost, LogoutPost, RegisterPost};
pub use thread::{PostPost, ThreadGet, ThreadPost};
pub use user::UserGet;

mod forum;
mod login;
pub mod partial;
mod thread;
mod user;
