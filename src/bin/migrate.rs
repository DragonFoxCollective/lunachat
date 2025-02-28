use bincode::Options as _;
use dragon_fox::error::{Error, Result};
use dragon_fox::option_ok;
use dragon_fox::state::post::{Post, PostKey};
use dragon_fox::state::thread::{Thread, Threads};
use dragon_fox::state::user::{User, UserKey};
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
        3,
        (1, Post1, Post2, migrate_post1),
        (2, Post2, Post, migrate_post2)
    );

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User1 {
    pub key: UserKey,
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
    pub key: PostKey,
    pub body: String,
    pub author: UserKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post2 {
    pub key: PostKey,
    pub body: String,
    pub author: UserKey,
    pub parent: Option<PostKey>,
    pub children: Vec<PostKey>,
}

fn migrate_post1(db: &Db, post: Post1) -> Result<Post2> {
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
    Ok(Post2 {
        key: post.key,
        body: post.body,
        author: post.author,
        parent,
        children: vec![child].into_iter().flatten().collect(),
    })
}

fn migrate_post2(db: &Db, post: Post2) -> Result<Post> {
    let posts = db.open_tree("posts")?;
    let mut root_key = post.key;
    let mut parent_key = post.parent;
    while let Some(parent) = parent_key {
        let parent = posts
            .get(BINCODE.serialize(&parent)?)?
            .ok_or(Error::PostNotFound(parent))?;
        let parent: Post2 = BINCODE.deserialize(&parent)?;
        root_key = parent.key;
        parent_key = parent.parent;
    }

    let threads = Threads::open(db)?;
    let thread = threads
        .iter()
        .values()
        .find_map(|thread| {
            let thread = option_ok!(thread);
            if thread.post == root_key {
                Some(Ok::<Thread, Error>(thread))
            } else {
                None
            }
        })
        .unwrap()?;

    Ok(Post {
        key: post.key,
        body: post.body,
        author: post.author,
        parent: post.parent,
        children: post.children,
        thread: thread.key,
    })
}
