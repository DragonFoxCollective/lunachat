use bincode::Options as _;
use dragon_fox::state::key::Key;
use dragon_fox::state::user::User;
use dragon_fox::state::{DbTreeLookup, TableType, Versions, BINCODE};
use serde::{Deserialize, Serialize};

macro_rules! migrate_up_to {
	($db:ident, $table:expr, $max_version:expr, $(($from_version:expr, $from:ty, $to:ty, $migrate:ident)),*) => {
		let versions = Versions::new($db.open_tree("versions").unwrap());

		if let Some(mut version) = versions.get($table).unwrap() {
			if version < $max_version {
				let tree = $db.open_tree(stringify!($table)).unwrap();
				$(
					if version == $from_version {
						for (key, mut value) in tree.iter().map(|x| x.unwrap()) {
							value = BINCODE.serialize(&$migrate(BINCODE.deserialize(&value).unwrap())).unwrap().into();
							tree.insert(key, value).unwrap();
						}
						version += 1;
					}
				)*
				tree.flush().unwrap();
				versions.insert($table, version).unwrap();
				versions.flush().await.unwrap();
			}
		} else {
			// Assume there are no entries?
			versions.insert($table, $max_version).unwrap();
			versions.flush().await.unwrap();
		}
	};
}

#[tokio::main]
async fn main() {
    let db = sled::open("db").unwrap();

    // Doesn't handle username -> key mapping
    migrate_up_to!(db, TableType::Users, 2, (1, User1, User, migrate_user1));
}

#[derive(Clone, Serialize, Deserialize)]
struct User1 {
    pub key: Key,
    pub username: String,
    password: String,
}

fn migrate_user1(user: User1) -> User {
    User {
        key: user.key,
        username: user.username,
        password: user.password,
        avatar: None,
    }
}
