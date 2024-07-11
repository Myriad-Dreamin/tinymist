mod export;
pub use export::*;
mod format;
pub use format::*;
mod user_action;
pub use user_action::*;

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
    fn spawn(&self, revision: usize, fut: FoldFuture) {
        let mut state = self.state.lock();
        let state = state.deref_mut();

        match &mut state.task {
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

        if state.running {
            return;
        }

        state.running = true;

        let state = self.state.clone();
        tokio::spawn(async move {
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
        });
    }
}
