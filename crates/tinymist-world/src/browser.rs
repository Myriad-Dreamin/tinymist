use std::{path::PathBuf, sync::Arc};

use tinymist_vfs::browser::ProxyAccessModel;
use typst::foundations::Dict as TypstDict;
use typst::utils::LazyHash;

use crate::entry::EntryState;
use crate::font::FontResolverImpl;
use crate::package::browser::ProxyRegistry;

/// A world that provides access to the browser.
/// It is under development.
pub type TypstBrowserUniverse = crate::world::CompilerUniverse<BrowserCompilerFeat>;
pub type TypstBrowserWorld = crate::world::CompilerWorld<BrowserCompilerFeat>;

#[derive(Debug, Clone, Copy)]
pub struct BrowserCompilerFeat;

impl crate::CompilerFeat for BrowserCompilerFeat {
    /// Uses [`FontResolverImpl`] directly.
    type FontResolver = FontResolverImpl;
    type AccessModel = ProxyAccessModel;
    type Registry = ProxyRegistry;
}

// todo
/// Safety: `ProxyRegistry` is only used in the browser environment, and we
/// cannot share data between workers.
unsafe impl Send for ProxyRegistry {}
/// Safety: `ProxyRegistry` is only used in the browser environment, and we
/// cannot share data between workers.
unsafe impl Sync for ProxyRegistry {}

impl TypstBrowserUniverse {
    pub fn new(
        root_dir: PathBuf,
        inputs: Option<Arc<LazyHash<TypstDict>>>,
        access_model: ProxyAccessModel,
        registry: ProxyRegistry,
        font_resolver: FontResolverImpl,
    ) -> Self {
        let vfs = tinymist_vfs::Vfs::new(access_model);

        Self::new_raw(
            EntryState::new_rooted(root_dir.into(), None),
            inputs,
            vfs,
            registry,
            Arc::new(font_resolver),
        )
    }
}
