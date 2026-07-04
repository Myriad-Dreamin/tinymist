//! Task are stateless actors that staring computing tasks.
//! [`SyncTaskFactory`] can hold *mutable* configuration but the mutations don't
//! blocking the computation, i.e. the mutations are non-blocking.

#[cfg(feature = "export")]
mod export;
#[cfg(feature = "export")]
pub use export::*;
#[cfg(feature = "export")]
pub mod export2;
mod format;
pub use format::*;
#[cfg(feature = "trace")]
mod user_action;
#[cfg(feature = "trace")]
pub use user_action::*;

use std::sync::Arc;

use reflexo::TakeAs;

/// Please uses this if you believe all mutations are fast
#[derive(Clone, Default)]
pub struct SyncTaskFactory<T>(Arc<std::sync::RwLock<Arc<T>>>);

impl<T> SyncTaskFactory<T> {
    pub fn new(config: T) -> Self {
        Self(Arc::new(std::sync::RwLock::new(Arc::new(config))))
    }
}

impl<T: Clone> SyncTaskFactory<T> {
    fn mutate(&self, f: impl FnOnce(&mut T)) {
        let mut w = self.0.write().unwrap();
        let mut config = w.clone().take();
        f(&mut config);
        *w = Arc::new(config);
    }

    pub fn task(&self) -> Arc<T> {
        self.0.read().unwrap().clone()
    }
}
