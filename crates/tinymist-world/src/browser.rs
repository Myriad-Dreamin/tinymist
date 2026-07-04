use std::{path::PathBuf, sync::Arc};

use tinymist_vfs::browser::ProxyAccessModel;
use typst::Features;
use typst::foundations::Dict as TypstDict;
use typst::utils::LazyHash;

use crate::entry::EntryState;
use crate::font::FontResolverImpl;
use crate::package::RegistryPathMapper;
use crate::package::registry::JsRegistry;

/// A universe that provides access to the browser.
pub type TypstBrowserUniverse = crate::world::CompilerUniverse<BrowserCompilerFeat>;
/// A world that provides access to the browser.
pub type TypstBrowserWorld = crate::world::CompilerWorld<BrowserCompilerFeat>;

/// The feature of the browser world.
#[derive(Debug, Clone, Copy)]
pub struct BrowserCompilerFeat;

impl crate::CompilerFeat for BrowserCompilerFeat {
    /// Uses [`FontResolverImpl`] directly.
    type FontResolver = FontResolverImpl;
    type AccessModel = ProxyAccessModel;
    type Registry = JsRegistry;
}

impl TypstBrowserUniverse {
    /// Creates a new browser universe.
    pub fn new(
        root_dir: PathBuf,
        inputs: Option<Arc<LazyHash<TypstDict>>>,
        access_model: ProxyAccessModel,
        registry: JsRegistry,
        font_resolver: FontResolverImpl,
    ) -> Self {
        let registry = Arc::new(registry);
        let resolver = Arc::new(RegistryPathMapper::new(registry.clone()));

        let vfs = tinymist_vfs::Vfs::new(resolver, access_model);

        // todo: enable html
        Self::new_raw(
            EntryState::new_rooted(root_dir.into(), None),
            Features::default(),
            inputs,
            vfs,
            registry,
            Arc::new(font_resolver),
            None, // creation_timestamp - not used in browser context
        )
    }
}
