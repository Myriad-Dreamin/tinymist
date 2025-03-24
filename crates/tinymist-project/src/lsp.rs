use std::path::Path;
use std::{borrow::Cow, sync::Arc};

use tinymist_std::error::prelude::*;
use tinymist_std::{bail, ImmutPath};
use tinymist_task::ExportTarget;
use tinymist_world::config::CompileFontOpts;
use tinymist_world::font::system::SystemFontSearcher;
use tinymist_world::package::{http::HttpRegistry, RegistryPathMapper};
use tinymist_world::vfs::{system::SystemAccessModel, Vfs};
use tinymist_world::{args::*, WorldComputeGraph};
use tinymist_world::{
    CompileSnapshot, CompilerFeat, CompilerUniverse, CompilerWorld, EntryOpts, EntryState,
};
use typst::foundations::{Dict, Str, Value};
use typst::utils::LazyHash;

use crate::ProjectInput;

use crate::font::TinymistFontResolver;
use crate::{CompiledArtifact, Interrupt};

/// Compiler feature for LSP universe and worlds without typst.ts to implement
/// more for tinymist. type trait of [`CompilerUniverse`].
#[derive(Debug, Clone, Copy)]
pub struct LspCompilerFeat;

impl CompilerFeat for LspCompilerFeat {
    /// Uses [`TinymistFontResolver`] directly.
    type FontResolver = TinymistFontResolver;
    /// It accesses a physical file system.
    type AccessModel = SystemAccessModel;
    /// It performs native HTTP requests for fetching package data.
    type Registry = HttpRegistry;
}

/// LSP universe that spawns LSP worlds.
pub type LspUniverse = CompilerUniverse<LspCompilerFeat>;
/// LSP world that holds compilation resources
pub type LspWorld = CompilerWorld<LspCompilerFeat>;
/// LSP compile snapshot.
pub type LspCompileSnapshot = CompileSnapshot<LspCompilerFeat>;
/// LSP compiled artifact.
pub type LspCompiledArtifact = CompiledArtifact<LspCompilerFeat>;
/// LSP compiled artifact.
pub type LspComputeGraph = Arc<WorldComputeGraph<LspCompilerFeat>>;
/// LSP interrupt.
pub type LspInterrupt = Interrupt<LspCompilerFeat>;
/// Immutable prehashed reference to dictionary.
pub type ImmutDict = Arc<LazyHash<Dict>>;

/// World provider for LSP universe and worlds.
pub trait WorldProvider {
    /// Get the entry options from the arguments.
    fn entry(&self) -> Result<EntryOpts>;
    /// Get a universe instance from the given arguments.
    fn resolve(&self) -> Result<LspUniverse>;
}

impl WorldProvider for CompileOnceArgs {
    fn resolve(&self) -> Result<LspUniverse> {
        let entry = self.entry()?.try_into()?;
        let inputs = self.resolve_inputs().unwrap_or_default();
        let fonts = Arc::new(LspUniverseBuilder::resolve_fonts(self.font.clone())?);
        let package = LspUniverseBuilder::resolve_package(
            self.cert.as_deref().map(From::from),
            Some(&self.package),
        );

        // todo: more export targets
        Ok(LspUniverseBuilder::build(
            entry,
            ExportTarget::Paged,
            inputs,
            fonts,
            package,
        ))
    }

    fn entry(&self) -> Result<EntryOpts> {
        let mut cwd = None;
        let mut cwd = move || {
            cwd.get_or_insert_with(|| {
                std::env::current_dir().context("failed to get current directory")
            })
            .clone()
        };

        let main = {
            let input = self.input.as_ref().context("entry file must be provided")?;
            let input = Path::new(&input);
            if input.is_absolute() {
                input.to_owned()
            } else {
                cwd()?.join(input)
            }
        };

        let root = if let Some(root) = &self.root {
            if root.is_absolute() {
                root.clone()
            } else {
                cwd()?.join(root)
            }
        } else {
            main.parent()
                .context("entry file don't have a valid parent as root")?
                .to_owned()
        };

        let relative_main = match main.strip_prefix(&root) {
            Ok(relative_main) => relative_main,
            Err(_) => {
                log::error!("entry file must be inside the root, file: {main:?}, root: {root:?}");
                bail!("entry file must be inside the root, file: {main:?}, root: {root:?}");
            }
        };

        Ok(EntryOpts::new_rooted(
            root.clone(),
            Some(relative_main.to_owned()),
        ))
    }
}

// todo: merge me with the above impl
impl WorldProvider for (ProjectInput, ImmutPath) {
    fn resolve(&self) -> Result<LspUniverse> {
        let (proj, lock_dir) = self;
        let entry = self.entry()?.try_into()?;
        let inputs = proj
            .inputs
            .iter()
            .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
            .collect();
        let fonts = LspUniverseBuilder::resolve_fonts(CompileFontArgs {
            font_paths: {
                proj.font_paths
                    .iter()
                    .flat_map(|p| p.to_abs_path(lock_dir))
                    .collect::<Vec<_>>()
            },
            ignore_system_fonts: !proj.system_fonts,
        })?;
        let package = LspUniverseBuilder::resolve_package(
            // todo: recover certificate path
            None,
            Some(&CompilePackageArgs {
                package_path: proj
                    .package_path
                    .as_ref()
                    .and_then(|p| p.to_abs_path(lock_dir)),
                package_cache_path: proj
                    .package_cache_path
                    .as_ref()
                    .and_then(|p| p.to_abs_path(lock_dir)),
            }),
        );

        // todo: more export targets
        Ok(LspUniverseBuilder::build(
            entry,
            ExportTarget::Paged,
            Arc::new(LazyHash::new(inputs)),
            Arc::new(fonts),
            package,
        ))
    }

    fn entry(&self) -> Result<EntryOpts> {
        let (proj, lock_dir) = self;

        let entry = proj
            .main
            .to_abs_path(lock_dir)
            .context("failed to resolve entry file")?;

        let root = if let Some(root) = &proj.root {
            root.to_abs_path(lock_dir)
                .context("failed to resolve root")?
        } else {
            lock_dir.as_ref().to_owned()
        };

        if !entry.starts_with(&root) {
            bail!("entry file must be in the root directory, {entry:?}, {root:?}");
        }

        let relative_entry = match entry.strip_prefix(&root) {
            Ok(relative_entry) => relative_entry,
            Err(_) => bail!("entry path must be inside the root: {}", entry.display()),
        };

        Ok(EntryOpts::new_rooted(
            root.clone(),
            Some(relative_entry.to_owned()),
        ))
    }
}

/// Builder for LSP universe.
pub struct LspUniverseBuilder;

impl LspUniverseBuilder {
    /// Create [`LspUniverse`] with the given options.
    /// See [`LspCompilerFeat`] for instantiation details.
    pub fn build(
        entry: EntryState,
        export_target: ExportTarget,
        inputs: ImmutDict,
        font_resolver: Arc<TinymistFontResolver>,
        package_registry: HttpRegistry,
    ) -> LspUniverse {
        let registry = Arc::new(package_registry);
        let resolver = Arc::new(RegistryPathMapper::new(registry.clone()));

        LspUniverse::new_raw(
            entry,
            matches!(export_target, ExportTarget::Html),
            Some(inputs),
            Vfs::new(resolver, SystemAccessModel {}),
            registry,
            font_resolver,
        )
    }

    /// Resolve fonts from given options.
    pub fn only_embedded_fonts() -> Result<TinymistFontResolver> {
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
    pub fn resolve_fonts(args: CompileFontArgs) -> Result<TinymistFontResolver> {
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
