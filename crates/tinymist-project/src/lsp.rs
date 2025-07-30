use std::path::Path;
use std::{borrow::Cow, sync::Arc};

use tinymist_std::error::prelude::*;
use tinymist_std::{bail, ImmutPath};
use tinymist_task::ExportTarget;
use tinymist_world::config::CompileFontOpts;
use tinymist_world::font::system::SystemFontSearcher;
use tinymist_world::package::{registry::HttpRegistry, RegistryPathMapper};
use tinymist_world::vfs::{system::SystemAccessModel, Vfs};
use tinymist_world::{args::*, WorldComputeGraph};
use tinymist_world::{
    CompileSnapshot, CompilerFeat, CompilerUniverse, CompilerWorld, EntryOpts, EntryState,
};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Dict, Str, Value};
use typst::utils::LazyHash;
use typst::Features;

use crate::ProjectInput;

use crate::world::font::FontResolverImpl;
use crate::{CompiledArtifact, Interrupt};

/// Compiler feature for LSP universe and worlds without typst.ts to implement
/// more for tinymist. type trait of [`CompilerUniverse`].
#[derive(Debug, Clone, Copy)]
pub struct LspCompilerFeat;

impl CompilerFeat for LspCompilerFeat {
    /// Uses [`FontResolverImpl`] directly.
    type FontResolver = FontResolverImpl;
    /// It accesses a physical file system.
    type AccessModel = DynAccessModel;
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
/// LSP compute graph.
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
        let packages = LspUniverseBuilder::resolve_package(
            self.cert.as_deref().map(From::from),
            Some(&self.package),
        );

        // todo: more export targets
        Ok(LspUniverseBuilder::build(
            entry,
            ExportTarget::Paged,
            self.resolve_features(),
            inputs,
            packages,
            fonts,
            self.creation_timestamp,
            DynAccessModel(Arc::new(SystemAccessModel {})),
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
        let packages = LspUniverseBuilder::resolve_package(
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
            // todo: features
            Features::default(),
            Arc::new(LazyHash::new(inputs)),
            packages,
            Arc::new(fonts),
            None, // creation_timestamp - not available in project file context
            DynAccessModel(Arc::new(SystemAccessModel {})),
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
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        entry: EntryState,
        export_target: ExportTarget,
        features: Features,
        inputs: ImmutDict,
        package_registry: HttpRegistry,
        font_resolver: Arc<FontResolverImpl>,
        creation_timestamp: Option<i64>,
        access_model: DynAccessModel,
    ) -> LspUniverse {
        let package_registry = Arc::new(package_registry);
        let resolver = Arc::new(RegistryPathMapper::new(package_registry.clone()));

        // todo: typst doesn't allow to merge features
        let features = if matches!(export_target, ExportTarget::Html) {
            Features::from_iter([typst::Feature::Html])
        } else {
            features
        };

        LspUniverse::new_raw(
            entry,
            features,
            Some(inputs),
            Vfs::new(resolver, access_model),
            package_registry,
            font_resolver,
            creation_timestamp,
        )
    }

    /// Resolve fonts from given options.
    pub fn only_embedded_fonts() -> Result<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(CompileFontOpts {
            font_paths: vec![],
            no_system_fonts: true,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.build())
    }

    /// Resolve fonts from given options.
    pub fn resolve_fonts(args: CompileFontArgs) -> Result<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(CompileFontOpts {
            font_paths: args.font_paths,
            no_system_fonts: args.ignore_system_fonts,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.build())
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

/// Access model for LSP universe and worlds.
pub trait LspAccessModel: Send + Sync {
    /// Returns the content of a file entry.
    fn content(&self, src: &Path) -> FileResult<Bytes>;
}

impl<T> LspAccessModel for T
where
    T: tinymist_world::vfs::PathAccessModel + Send + Sync + 'static,
{
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.content(src)
    }
}

/// Access model for LSP universe and worlds.
#[derive(Clone)]
pub struct DynAccessModel(pub Arc<dyn LspAccessModel>);

impl DynAccessModel {
    /// Create a new dynamic access model from the given access model.
    pub fn new(access_model: Arc<dyn LspAccessModel>) -> Self {
        Self(access_model)
    }
}

impl tinymist_world::vfs::PathAccessModel for DynAccessModel {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.0.content(src)
    }

    fn reset(&mut self) {}
}
