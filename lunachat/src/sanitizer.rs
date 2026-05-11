use std::sync::Arc;

use derive_more::{Deref, DerefMut};

#[derive(Clone, Deref, DerefMut)]
pub struct Sanitizer(pub Arc<ammonia::Builder<'static>>);

impl Sanitizer {
    pub fn new(builder: ammonia::Builder<'static>) -> Self {
        Self(Arc::new(builder))
    }
}
