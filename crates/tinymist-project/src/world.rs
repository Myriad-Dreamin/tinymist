//! World implementation of typst for tinymist.

pub use tinymist_std::error::prelude;
pub use tinymist_world as base;
pub use tinymist_world::args::*;
pub use tinymist_world::config::CompileFontOpts;
use tinymist_world::package::RegistryPathMapper;
pub use tinymist_world::vfs;
pub use tinymist_world::{entry::*, EntryOpts, EntryState};
pub use tinymist_world::{font, package, CompilerUniverse, CompilerWorld, Revising, TaskInputs};

use std::path::Path;
use std::{borrow::Cow, sync::Arc};

use ::typst::utils::LazyHash;
use anyhow::Context;
use tinymist_std::error::prelude::*;
use tinymist_std::ImmutPath;
use tinymist_world::font::system::SystemFontSearcher;
use tinymist_world::package::http::HttpRegistry;
use tinymist_world::vfs::{system::SystemAccessModel, Vfs};
use tinymist_world::CompilerFeat;
use typst::foundations::{Dict, Str, Value};

use crate::font::TinymistFontResolver;

/// Compiler feature for LSP universe and worlds without typst.ts to implement
/// more for tinymist. type trait of [`CompilerUniverse`].
#[derive(Debug, Clone, Copy)]
pub struct SystemCompilerFeatExtend;

impl CompilerFeat for SystemCompilerFeatExtend {
    /// Uses [`TinymistFontResolver`] directly.
    type FontResolver = TinymistFontResolver;
    /// It accesses a physical file system.
    type AccessModel = SystemAccessModel;
    /// It performs native HTTP requests for fetching package data.
    type Registry = HttpRegistry;
}

/// The compiler universe in system environment.
pub type TypstSystemUniverseExtend = CompilerUniverse<SystemCompilerFeatExtend>;
/// The compiler world in system environment.
pub type TypstSystemWorldExtend = CompilerWorld<SystemCompilerFeatExtend>;

pub trait WorldProvider {
    /// Get the entry options from the arguments.
    fn entry(&self) -> anyhow::Result<EntryOpts>;
    /// Get a universe instance from the given arguments.
    fn resolve(&self) -> anyhow::Result<LspUniverse>;
}

impl WorldProvider for CompileOnceArgs {
    fn resolve(&self) -> anyhow::Result<LspUniverse> {
        let entry = self.entry()?.try_into()?;
        let inputs = self
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
            .collect();
        let fonts = LspUniverseBuilder::resolve_fonts(self.font.clone())?;
        let package = LspUniverseBuilder::resolve_package(
            self.cert.as_deref().map(From::from),
            Some(&self.package),
        );

        LspUniverseBuilder::build(
            entry,
            Arc::new(LazyHash::new(inputs)),
            Arc::new(fonts),
            package,
        )
        .context("failed to create universe")
    }

    fn entry(&self) -> anyhow::Result<EntryOpts> {
        let input = self.input.as_ref().context("entry file must be provided")?;
        let input = Path::new(&input);
        let entry = if input.is_absolute() {
            input.to_owned()
        } else {
            std::env::current_dir().unwrap().join(input)
        };

        let root = if let Some(root) = &self.root {
            if root.is_absolute() {
                root.clone()
            } else {
                std::env::current_dir().unwrap().join(root)
            }
        } else {
            std::env::current_dir().unwrap()
        };

        if !entry.starts_with(&root) {
            log::error!("entry file must be in the root directory");
            std::process::exit(1);
        }

        let relative_entry = match entry.strip_prefix(&root) {
            Ok(relative_entry) => relative_entry,
            Err(_) => {
                log::error!("entry path must be inside the root: {}", entry.display());
                std::process::exit(1);
            }
        };

        Ok(EntryOpts::new_rooted(
            root.clone(),
            Some(relative_entry.to_owned()),
        ))
    }
}

/// Compiler feature for LSP universe and worlds.
pub type LspCompilerFeat = SystemCompilerFeatExtend;
/// LSP universe that spawns LSP worlds.
pub type LspUniverse = TypstSystemUniverseExtend;
/// LSP world.
pub type LspWorld = TypstSystemWorldExtend;
/// Immutable prehashed reference to dictionary.
pub type ImmutDict = Arc<LazyHash<Dict>>;

/// Builder for LSP universe.
pub struct LspUniverseBuilder;

impl LspUniverseBuilder {
    /// Create [`LspUniverse`] with the given options.
    /// See [`LspCompilerFeat`] for instantiation details.
    pub fn build(
        entry: EntryState,
        inputs: ImmutDict,
        font_resolver: Arc<TinymistFontResolver>,
        package_registry: HttpRegistry,
    ) -> ZResult<LspUniverse> {
        let registry = Arc::new(package_registry);
        let resolver = Arc::new(RegistryPathMapper::new(registry.clone()));

        Ok(LspUniverse::new_raw(
            entry,
            Some(inputs),
            Vfs::new(resolver, SystemAccessModel {}),
            registry,
            font_resolver,
        ))
    }

    /// Resolve fonts from given options.
    pub fn only_embedded_fonts() -> ZResult<TinymistFontResolver> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(CompileFontOpts {
            font_profile_cache_path: Default::default(),
            font_paths: vec![],
            no_system_fonts: true,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.into())
    }

    /// Resolve fonts from given options.
    pub fn resolve_fonts(args: CompileFontArgs) -> ZResult<TinymistFontResolver> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(CompileFontOpts {
            font_profile_cache_path: Default::default(),
            font_paths: args.font_paths,
            no_system_fonts: args.ignore_system_fonts,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.into())
    }

    /// Resolve package registry from given options.
    pub fn resolve_package(
        cert_path: Option<ImmutPath>,
        args: Option<&CompilePackageArgs>,
    ) -> HttpRegistry {
        HttpRegistry::new(
            cert_path,
            args.and_then(|args| Some(args.package_path.clone()?.into())),
            args.and_then(|args| Some(args.package_cache_path.clone()?.into())),
        )
    }
}
