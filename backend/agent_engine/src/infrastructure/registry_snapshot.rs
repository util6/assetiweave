use arc_swap::ArcSwap;
use std::sync::Arc;

#[derive(Debug)]
pub struct RegistrySnapshot<T> {
    inner: ArcSwap<T>,
}

impl<T> RegistrySnapshot<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: ArcSwap::from_pointee(value),
        }
    }

    pub fn from_arc(value: Arc<T>) -> Self {
        Self {
            inner: ArcSwap::from(value),
        }
    }

    pub fn load(&self) -> Arc<T> {
        self.inner.load_full()
    }

    pub fn replace(&self, value: T) {
        self.inner.store(Arc::new(value));
    }
}
