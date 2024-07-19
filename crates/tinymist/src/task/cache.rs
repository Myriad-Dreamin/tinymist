//! The actor that handles cache evicting.

use std::sync::{atomic::AtomicUsize, Arc};

use super::{FutureFolder, SyncTaskFactory};

#[derive(Debug, Clone)]
pub struct CacheUserConfig {
    pub max_age: usize,
}

impl Default for CacheUserConfig {
    fn default() -> Self {
        Self { max_age: 30 }
    }
}

#[derive(Clone, Default)]
pub struct CacheTask {
    factory: SyncTaskFactory<CacheUserConfig>,
    cache_evict_folder: FutureFolder,
    revision: Arc<AtomicUsize>,
}

impl CacheTask {
    pub fn new(c: CacheUserConfig) -> Self {
        Self {
            factory: SyncTaskFactory::new(c),
            cache_evict_folder: FutureFolder::default(),
            revision: Arc::new(AtomicUsize::default()),
        }
    }

    pub fn evict(&self) {
        let revision = self
            .revision
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let task = self.factory.task();
        self.cache_evict_folder.spawn(revision, || {
            Box::pin(async move {
                let _ = FutureFolder::compute(move |_| {
                    // Evict compilation cache.
                    let evict_start = std::time::Instant::now();
                    comemo::evict(task.max_age);
                    let elapsed = evict_start.elapsed();
                    log::info!("CacheEvictTask: evict cache in {elapsed:?}");
                })
                .await;

                Some(())
            })
        });
    }
}
