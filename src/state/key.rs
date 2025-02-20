use std::fmt::Display;

use bincode::Options as _;
use serde::{Deserialize, Serialize};
use sled::{IVec, Tree};

use crate::error::{Error, Result};

use super::{TableType, BINCODE};

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

#[derive(Clone)]
pub struct HighestKeys(Tree);

impl HighestKeys {
    pub fn new(tree: Tree) -> Self {
        Self(tree)
    }

    pub fn next(&self, table: TableType) -> Result<Key> {
        let table: IVec = BINCODE.serialize(&table)?.into();
        let key = self.0.fetch_and_update(table, |key| {
            let key = key.map_or(0u64, |key| BINCODE.deserialize(key).unwrap()) + 1;
            let key: IVec = BINCODE.serialize(&key).unwrap().into();
            Some(key)
        })?;
        let key = key
            .map(|key| BINCODE.deserialize(&key))
            .unwrap_or(Ok(0u64))?;
        Ok(Key(key))
    }
}
