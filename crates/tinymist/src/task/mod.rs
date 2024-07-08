mod export;
pub use export::*;

use std::{ops::DerefMut, pin::Pin, sync::Arc};

use futures::Future;
use parking_lot::Mutex;
use reflexo::TakeAs;

/// Please uses this if you believe all mutations are fast
#[derive(Clone, Default)]
struct SyncTaskFactory<T>(Arc<std::sync::RwLock<Arc<T>>>);

impl<T: Clone> SyncTaskFactory<T> {
    fn mutate(&self, f: impl FnOnce(&mut T)) {
        let mut w = self.0.write().unwrap();
        let mut data = w.clone().take();
        f(&mut data);
        *w = Arc::new(data);
    }

    fn task(&self) -> Arc<T> {
        self.0.read().unwrap().clone()
    }
}

type FoldFuture = Pin<Box<dyn Future<Output = Option<()>> + Send + Sync>>;

#[derive(Clone, Default)]
struct FutureFolder {
    next: Arc<Mutex<Option<(usize, FoldFuture)>>>,
}

impl FutureFolder {
    fn spawn(&self, revision: usize, fut: FoldFuture) {
        let mut next_update = self.next.lock();
        let next_update = next_update.deref_mut();

        match next_update {
            Some((prev_revision, prev)) => {
                if *prev_revision < revision {
                    *prev = fut;
                    *prev_revision = revision;
                }

                return;
            }
            next_update => {
                *next_update = Some((revision, fut));
            }
        }

        let next = self.next.clone();
        tokio::spawn(async move {
            loop {
                let Some((_, fut)) = next.lock().take() else {
                    return;
                };
                fut.await;
            }
        });
    }
}
