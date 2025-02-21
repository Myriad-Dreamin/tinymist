//! Task are stateless actors that staring computing tasks.
//! [`SyncTaskFactory`] can hold *mutable* configuration but the mutations don't
//! blocking the computation, i.e. the mutations are non-blocking.

mod export;
pub use export::*;
pub mod export2;
mod format;
pub use format::*;
mod user_action;
pub use user_action::*;

use std::{ops::DerefMut, pin::Pin, sync::Arc};

use futures::Future;
use parking_lot::Mutex;
use rayon::Scope;
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

type FoldFuture = Pin<Box<dyn Future<Output = Option<()>> + Send>>;

#[derive(Default)]
struct FoldingState {
    running: bool,
    task: Option<(usize, FoldFuture)>,
}

#[derive(Clone, Default)]
struct FutureFolder {
    state: Arc<Mutex<FoldingState>>,
}

impl FutureFolder {
    async fn compute<'scope, OP, R: Send + 'static>(op: OP) -> anyhow::Result<R>
    where
        OP: FnOnce(&Scope<'scope>) -> R + Send + 'static,
    {
        tokio::task::spawn_blocking(move || -> R { rayon::in_place_scope(op) })
            .await
            .map_err(|e| anyhow::anyhow!("compute error: {e:?}"))
    }

    #[must_use]
    fn spawn(
        &self,
        revision: usize,
        fut: impl FnOnce() -> FoldFuture,
    ) -> Option<impl Future<Output = ()> + Send + 'static> {
        let mut state = self.state.lock();
        let state = state.deref_mut();

        match &mut state.task {
            Some((prev_revision, prev)) => {
                if *prev_revision < revision {
                    *prev = fut();
                    *prev_revision = revision;
                }

                return None;
            }
            next_update => {
                *next_update = Some((revision, fut()));
            }
        }

        if state.running {
            return None;
        }

        state.running = true;

        let state = self.state.clone();
        Some(async move {
            loop {
                let fut = {
                    let mut state = state.lock();
                    let Some((_, fut)) = state.task.take() else {
                        state.running = false;
                        return;
                    };
                    fut
                };
                fut.await;
            }
        })
    }
}
