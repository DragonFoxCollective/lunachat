pub use awesome_axum_responses::*;
use futures::future::{JoinAll, join_all};
pub use return_ok::*;
pub use sea_orm::{DatabaseConnection, EntityTrait as _};

pub use crate::entity::*;

pub trait MapAsyncExt: Iterator {
    fn map_async<T, Fut: Future<Output = T>>(self, f: impl Fn(Self::Item) -> Fut) -> JoinAll<Fut>;
}

impl<Iter: Iterator> MapAsyncExt for Iter {
    fn map_async<T, Fut: Future<Output = T>>(self, f: impl Fn(Self::Item) -> Fut) -> JoinAll<Fut> {
        join_all(self.map(f))
    }
}
