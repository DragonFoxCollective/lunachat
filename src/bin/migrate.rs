use bincode::Options as _;
use dragon_fox::error::Result;
use dragon_fox::state::key::Key;
use dragon_fox::state::post::Post;
use dragon_fox::state::thread::{Thread, Threads};
use dragon_fox::state::user::User;
use dragon_fox::state::{DbTreeLookup, TableType, Versions, BINCODE};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sled::Db;

macro_rules! migrate_up_to {
	($db:ident, $table:expr, $table_key:expr, $max_version:expr, $(($from_version:expr, $from:ty, $to:ty, $migrate:ident)),*) => {
		let versions = Versions::open(&$db)?;

		if let Some(mut version) = versions.get($table)? {
			if version < $max_version {
				let tree = $db.open_tree($table_key)?;
				$(
					if version == $from_version {
						println!("Migrating {} from version {} to {} using {}", stringify!($table), $from_version, $from_version + 1, stringify!($migrate));
						for item in tree.iter() {
							let (key, value) = item?;
							let value: $from = BINCODE.deserialize(&value)?;
							let new_value: $to = $migrate(&$db, value.clone())?;
							println!("Migrated {:?} to {:?}", value, new_value);
							tree.insert(key, BINCODE.serialize(&new_value)?)?;
						}
						version += 1;
					}
				)*
				tree.flush_async().await?;
				versions.insert($table, version)?;
				versions.flush().await?;
			}
		} else {
			// Assume there are no entries?
			versions.insert($table, $max_version)?;
			versions.flush().await?;
		}
	};
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = sled::open("db")?;

    // Doesn't handle username -> key mapping
    migrate_up_to!(
        db,
        TableType::Users,
        "users",
        2,
        (1, User1, User, migrate_user1)
    );

    migrate_up_to!(
        db,
        TableType::Posts,
        "posts",
        2,
        (1, Post1, Post, migrate_post1)
    );

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User1 {
    pub key: Key,
    pub username: String,
    password: String,
}

fn migrate_user1(_db: &Db, user: User1) -> Result<User> {
    Ok(User {
        key: user.key,
        username: user.username,
        password: user.password,
        avatar: None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post1 {
    pub key: Key,
    pub body: String,
    pub author: Key,
}

fn migrate_post1(db: &Db, post: Post1) -> Result<Post> {
    let posts = db.open_tree("posts")?;
    let (parent, child) = posts
        .iter()
        .keys()
        .map(|key| BINCODE.deserialize(&key.unwrap()).unwrap())
        .tuple_windows()
        .filter_map(|(prev, this, next)| {
            if this == post.key {
                Some((Some(prev), Some(next)))
            } else {
                None
            }
        })
        .next()
        .unwrap_or((None, None));

    if parent.is_none() {
        let threads = Threads::open(db)?;
        let thread_key = threads.next_key()?;
        threads.insert(
            thread_key,
            Thread {
                key: thread_key,
                title: "".to_string(),
                post: post.key,
            },
        )?;
    }

    // Whoops this actually forgets to set parent.children and child.parent
    Ok(Post {
        key: post.key,
        body: post.body,
        author: post.author,
        parent,
        children: vec![child].into_iter().flatten().collect(),
    })
}
