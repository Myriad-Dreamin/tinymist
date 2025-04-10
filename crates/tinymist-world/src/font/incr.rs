#![allow(missing_docs)]
#![allow(unused)]

use std::{
    any::TypeId,
    num::NonZeroUsize,
    ops::Range,
    sync::{atomic::AtomicUsize, Arc},
};

use ecow::EcoVec;
use rayon::iter::{FromParallelIterator, IntoParallelRefIterator, ParallelIterator};
use tinymist_std::hash::FxHashMap;
use typst::{
    foundations::Bytes,
    text::{FontBook, FontInfo},
};

use super::{memory::MemoryFontSearcher, FontResolver, FontResolverImpl, FontSlot};

pub trait FontSearchOp: std::any::Any + Send + Sync {
    fn hash(&self) -> u128 {
        tinymist_std::hash::hash128(&self.type_id())
    }
    fn load(&self) -> Vec<(FontInfo, FontSlot)>;
}

#[derive(Default)]
struct ReusableResources {
    pub prev: FxHashMap<(u128, TypeId), Range<usize>>,
    pub fonts: Arc<FontResolverImpl>,
}

#[derive(Clone, Default)]
pub struct IncrFontSearcher {
    ops: Vec<Arc<dyn FontSearchOp>>,
    prev: Option<Arc<ReusableResources>>,
}

impl IncrFontSearcher {
    pub fn new(ops: Vec<Arc<dyn FontSearchOp>>) -> Self {
        Self { ops, prev: None }
    }

    pub fn push_op(&mut self, op: impl FontSearchOp) {
        self.ops.push(Arc::new(op));
    }

    pub fn push_op_arc(&mut self, op: Arc<dyn FontSearchOp>) {
        self.ops.push(op);
    }

    #[must_use]
    pub fn set_op(&mut self, index: usize, op: impl FontSearchOp) -> bool {
        self.ops.get_mut(index).map_or(false, |o| {
            *o = Arc::new(op);
            true
        })
    }

    pub fn build(&mut self) -> Arc<FontResolverImpl> {
        let prev = self.prev.take().unwrap_or_default();
        let prev_book = prev.fonts.font_book();

        let mut next = FxHashMap::default();
        let mut base = MemoryFontSearcher::new();

        for op in &self.ops {
            let search_key = (op.hash(), op.type_id());

            if let Some(range) = prev.prev.get(&search_key) {
                let base_len = base.fonts.len();
                base.extend(range.clone().into_iter().flat_map(|index| {
                    Some((
                        prev_book.info(index)?.clone(),
                        prev.fonts.slot(index)?.clone(),
                    ))
                }));
                let new_range = base_len..base_len + range.len();
                next.insert(search_key, new_range);
            } else {
                let mut new_range = base.fonts.len()..base.fonts.len();
                let new_fonts = op.load();
                for (info, slot) in new_fonts {
                    let index = base.fonts.len();
                    base.fonts.push((info, slot));
                    new_range.end += 1;
                }
                next.insert(search_key, new_range);
            }
        }

        let fonts = Arc::new(base.build());
        let resource = Arc::new(ReusableResources {
            prev: next,
            fonts: fonts.clone(),
        });

        self.prev = Some(resource.clone());
        fonts
    }
}

pub struct EmbeddedAsset;

impl FontSearchOp for EmbeddedAsset {
    fn load(&self) -> Vec<(FontInfo, FontSlot)> {
        MemoryFontSearcher::from_par_iter(typst_assets::fonts().collect::<Vec<_>>()).fonts
    }
}

pub struct StaticAssets(pub Vec<Bytes>);

impl FontSearchOp for StaticAssets {
    fn load(&self) -> Vec<(FontInfo, FontSlot)> {
        MemoryFontSearcher::from_par_iter(self.0.par_iter().cloned()).fonts
    }
}

#[cfg(feature = "system")]
mod system {
    use super::*;
    use crate::font::system::SystemFontSearcher;

    pub struct SystemFontsOnce;

    impl FontSearchOp for SystemFontsOnce {
        fn load(&self) -> Vec<(FontInfo, FontSlot)> {
            let mut searcher = SystemFontSearcher::new();
            searcher.flush();
            searcher.base.fonts
        }
    }

    pub struct SystemFonts;

    impl FontSearchOp for SystemFonts {
        fn hash(&self) -> u128 {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u128
        }

        fn load(&self) -> Vec<(FontInfo, FontSlot)> {
            SystemFontsOnce.load()
        }
    }
}
#[cfg(feature = "system")]
pub use system::*;

#[cfg(feature = "system")]
#[cfg(test)]
mod tests {
    use ecow::eco_vec;

    use super::*;

    #[test]
    fn test_incr_font_searcher() {
        let mut searcher = IncrFontSearcher::default();

        pub struct TestSystemFonts;

        static LOAD_ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

        impl FontSearchOp for TestSystemFonts {
            fn hash(&self) -> u128 {
                SystemFonts.hash()
            }

            fn load(&self) -> Vec<(FontInfo, FontSlot)> {
                let loaded = LOAD_ONCE.swap(true, std::sync::atomic::Ordering::SeqCst);
                if loaded {
                    panic!("system fonts already loaded");
                }

                SystemFonts.load()
            }
        }

        // Initial build
        searcher.push_op(StaticAssets(vec![]));
        searcher.push_op(TestSystemFonts);
        searcher.build();

        // Incremental build
        let _ = searcher.set_op(0, StaticAssets(vec![]));
        searcher.build();

        if !LOAD_ONCE.load(std::sync::atomic::Ordering::SeqCst) {
            panic!("system fonts not loaded");
        }
    }
}
