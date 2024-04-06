use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::{Path, PathBuf},
    sync::Arc,
};

use ecow::EcoVec;
use once_cell::sync::OnceCell;
use reflexo::{cow_mut::CowMut, debug_loc::DataSource, ImmutPath};
use typst::{
    diag::{eco_format, FileError, FileResult, PackageError},
    syntax::{package::PackageSpec, Source, Span, VirtualPath},
    World,
};
use typst::{foundations::Value, syntax::ast, text::Font};
use typst::{layout::Position, syntax::FileId as TypstFileId};

use super::{get_def_use_inner, DefUseInfo};
use crate::{
    lsp_to_typst,
    syntax::{
        construct_module_dependencies, scan_workspace_files, LexicalHierarchy, ModuleDependency,
    },
    typst_to_lsp, LspPosition, LspRange, PositionEncoding, TypstRange, VersionedDocument,
};

/// A cache for module-level analysis results of a module.
///
/// You should not holds across requests, because source code may change.
pub struct ModuleAnalysisCache {
    source: OnceCell<FileResult<Source>>,
    def_use: OnceCell<Option<Arc<DefUseInfo>>>,
}

impl ModuleAnalysisCache {
    /// Get the source of a file.
    pub fn source(&self, ctx: &AnalysisContext, file_id: TypstFileId) -> FileResult<Source> {
        self.source
            .get_or_init(|| ctx.world().source(file_id))
            .clone()
    }

    /// Try to get the def-use information of a file.
    pub fn def_use(&self) -> Option<Arc<DefUseInfo>> {
        self.def_use.get().cloned().flatten()
    }

    /// Compute the def-use information of a file.
    pub(crate) fn compute_def_use(
        &self,
        f: impl FnOnce() -> Option<Arc<DefUseInfo>>,
    ) -> Option<Arc<DefUseInfo>> {
        self.def_use.get_or_init(f).clone()
    }
}

/// The analysis data holds globally.
pub struct Analysis {
    /// The root of the workspace.
    /// This means that the analysis result won't be valid if the root directory
    /// changes.
    pub root: ImmutPath,
    /// The position encoding for the workspace.
    pub position_encoding: PositionEncoding,
    /// The position encoding for the workspace.
    pub enable_periscope: bool,
    /// The global caches for analysis.
    pub caches: AnalysisGlobalCaches,
}

impl Analysis {
    /// Get estimated memory usage of the analysis data.
    pub fn estimated_memory(&self) -> usize {
        self.caches.modules.capacity() * 32
            + self
                .caches
                .modules
                .values()
                .map(|v| {
                    v.def_use_lexical_heirarchy
                        .output
                        .as_ref()
                        .map_or(0, |e| e.iter().map(|e| e.estimated_memory()).sum())
                })
                .sum::<usize>()
    }
}

struct ComputingNode<Inputs, Output> {
    name: &'static str,
    inputs: Option<Inputs>,
    output: Option<Output>,
}

pub(crate) trait ComputeDebug {
    fn compute_debug_repr(&self) -> impl std::fmt::Debug;
}

impl ComputeDebug for Source {
    fn compute_debug_repr(&self) -> impl std::fmt::Debug {
        self.id()
    }
}

impl<Inputs, Output> ComputingNode<Inputs, Output> {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            inputs: None,
            output: None,
        }
    }

    fn compute(
        &mut self,
        inputs: Inputs,
        compute: impl FnOnce(Option<Inputs>, Inputs) -> Option<Output>,
    ) -> Option<Output>
    where
        Inputs: ComputeDebug + Hash + Clone,
        Output: Clone,
    {
        match &self.inputs {
            Some(s) if reflexo::hash::hash128(&inputs) == reflexo::hash::hash128(&s) => {
                log::debug!(
                    "{}({:?}): hit cache",
                    self.name,
                    inputs.compute_debug_repr()
                );
                self.output.clone()
            }
            _ => {
                log::info!("{}({:?}): compute", self.name, inputs.compute_debug_repr());
                let output = compute(self.inputs.clone(), inputs.clone());
                self.output = output.clone();
                self.inputs = Some(inputs);
                output
            }
        }
    }
}

/// A cache for module-level analysis results of a module.
///
/// You should not holds across requests, because source code may change.
pub struct ModuleAnalysisGlobalCache {
    def_use_lexical_heirarchy: ComputingNode<Source, EcoVec<LexicalHierarchy>>,
}

impl Default for ModuleAnalysisGlobalCache {
    fn default() -> Self {
        Self {
            def_use_lexical_heirarchy: ComputingNode::new("def_use_lexical_heirarchy"),
        }
    }
}

/// A global (compiler server spanned) cache for all level of analysis results
/// of a module.
#[derive(Default)]
pub struct AnalysisGlobalCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisGlobalCache>,
}

/// A cache for all level of analysis results of a module.
#[derive(Default)]
pub struct AnalysisCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisCache>,
    root_files: OnceCell<Vec<TypstFileId>>,
    module_deps: OnceCell<HashMap<TypstFileId, ModuleDependency>>,
}

/// The resources for analysis.
pub trait AnaylsisResources {
    /// Get the world surface for Typst compiler.
    fn world(&self) -> &dyn World;

    /// Resolve the real path for a package spec.
    fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError>;

    /// Get all the files in the workspace.
    fn iter_dependencies(&self, f: &mut dyn FnMut(&ImmutPath, std::time::SystemTime));

    /// Resolve extra font information.
    fn font_info(&self, _font: Font) -> Option<Arc<DataSource>> {
        None
    }

    /// Resolve telescope image at the given position.
    fn periscope_at(
        &self,
        _ctx: &mut AnalysisContext,
        _doc: VersionedDocument,
        _pos: Position,
    ) -> Option<String> {
        None
    }
}

/// The context for analyzers.
pub struct AnalysisContext<'a> {
    /// The world surface for Typst compiler
    pub resources: &'a dyn AnaylsisResources,
    /// The analysis data
    pub analysis: CowMut<'a, Analysis>,
    caches: AnalysisCaches,
}

impl<'w> AnalysisContext<'w> {
    /// Create a new analysis context.
    pub fn new(resources: &'w dyn AnaylsisResources, a: Analysis) -> Self {
        Self {
            resources,
            analysis: CowMut::Owned(a),
            caches: AnalysisCaches::default(),
        }
    }

    /// Create a new analysis context with borrowing the analysis data.
    pub fn new_borrow(resources: &'w dyn AnaylsisResources, a: &'w mut Analysis) -> Self {
        Self {
            resources,
            analysis: CowMut::Borrowed(a),
            caches: AnalysisCaches::default(),
        }
    }

    /// Get the world surface for Typst compiler.
    pub fn world(&self) -> &'w dyn World {
        self.resources.world()
    }

    #[cfg(test)]
    pub fn test_files(&mut self, f: impl FnOnce() -> Vec<TypstFileId>) -> &Vec<TypstFileId> {
        self.caches.root_files.get_or_init(f)
    }

    /// Get all the files in the workspace.
    pub fn files(&mut self) -> &Vec<TypstFileId> {
        self.caches
            .root_files
            .get_or_init(|| scan_workspace_files(&self.analysis.root))
    }

    /// Get the module dependencies of the workspace.
    pub fn module_dependencies(&mut self) -> &HashMap<TypstFileId, ModuleDependency> {
        if self.caches.module_deps.get().is_some() {
            return self.caches.module_deps.get().unwrap();
        } else {
            // may cause multiple times to calculate, but it is okay because we have mutable
            // reference to self.
            let deps = construct_module_dependencies(self);
            self.caches.module_deps.get_or_init(|| deps)
        }
    }

    /// Resolve the real path for a file id.
    pub fn path_for_id(&self, id: TypstFileId) -> Result<PathBuf, FileError> {
        if id.vpath().as_rootless_path() == Path::new("-") {
            return Ok(PathBuf::from("-"));
        }

        // Determine the root path relative to which the file path
        // will be resolved.
        let root = match id.package() {
            Some(spec) => self.resources.resolve(spec)?,
            None => self.analysis.root.clone(),
        };

        // Join the path to the root. If it tries to escape, deny
        // access. Note: It can still escape via symlinks.
        id.vpath().resolve(&root).ok_or(FileError::AccessDenied)
    }

    /// Get the source of a file by file id.
    pub fn source_by_id(&mut self, id: TypstFileId) -> FileResult<Source> {
        self.get_mut(id);
        self.get(id).unwrap().source(self, id)
    }

    /// Get the source of a file by file path.
    pub fn source_by_path(&mut self, p: &Path) -> FileResult<Source> {
        // todo: source in packages
        let relative_path = p.strip_prefix(&self.analysis.root).map_err(|_| {
            FileError::Other(Some(eco_format!(
                "not in root, path is {p:?}, root is {:?}",
                self.analysis.root
            )))
        })?;

        let id = TypstFileId::new(None, VirtualPath::new(relative_path));
        self.source_by_id(id)
    }

    /// Get the module-level analysis cache of a file.
    pub fn get(&self, file_id: TypstFileId) -> Option<&ModuleAnalysisCache> {
        self.caches.modules.get(&file_id)
    }

    /// Get the module-level analysis cache of a file.
    pub fn get_mut(&mut self, file_id: TypstFileId) -> &ModuleAnalysisCache {
        self.caches.modules.entry(file_id).or_insert_with(|| {
            let source = OnceCell::new();
            let def_use = OnceCell::new();
            ModuleAnalysisCache { source, def_use }
        })
    }

    /// Get the def-use information of a source file.
    pub fn def_use(&mut self, source: Source) -> Option<Arc<DefUseInfo>> {
        get_def_use_inner(&mut self.fork_for_search(), source)
    }

    /// Fork a new context for searching in the workspace.
    pub fn fork_for_search<'s>(&'s mut self) -> SearchCtx<'s, 'w> {
        SearchCtx {
            ctx: self,
            searched: Default::default(),
            worklist: Default::default(),
        }
    }

    /// Get the position encoding during session.
    pub(crate) fn position_encoding(&self) -> PositionEncoding {
        self.analysis.position_encoding
    }

    /// Convert a LSP position to a Typst position.
    pub fn to_typst_pos(&self, position: LspPosition, src: &Source) -> Option<usize> {
        lsp_to_typst::position(position, self.analysis.position_encoding, src)
    }

    /// Convert a Typst offset to a LSP position.
    pub fn to_lsp_pos(&self, typst_offset: usize, src: &Source) -> LspPosition {
        typst_to_lsp::offset_to_position(typst_offset, self.analysis.position_encoding, src)
    }

    /// Convert a LSP range to a Typst range.
    pub fn to_typst_range(&self, position: LspRange, src: &Source) -> Option<TypstRange> {
        lsp_to_typst::range(position, self.analysis.position_encoding, src)
    }

    /// Convert a Typst range to a LSP range.
    pub fn to_lsp_range(&self, position: TypstRange, src: &Source) -> LspRange {
        typst_to_lsp::range(position, src, self.analysis.position_encoding)
    }

    pub(crate) fn def_use_lexical_hierarchy(
        &mut self,
        source: Source,
    ) -> Option<EcoVec<LexicalHierarchy>> {
        self.analysis
            .caches
            .modules
            .entry(source.id())
            .or_default()
            .def_use_lexical_heirarchy
            .compute(source, |_before, after| {
                crate::syntax::get_lexical_hierarchy(after, crate::syntax::LexicalScopeKind::DefUse)
            })
    }

    pub(crate) fn mini_eval(&self, rr: ast::Expr<'_>) -> Option<Value> {
        Some(match rr {
            ast::Expr::None(_) => Value::None,
            ast::Expr::Auto(_) => Value::Auto,
            ast::Expr::Bool(v) => Value::Bool(v.get()),
            ast::Expr::Int(v) => Value::Int(v.get()),
            ast::Expr::Float(v) => Value::Float(v.get()),
            ast::Expr::Numeric(v) => Value::numeric(v.get()),
            ast::Expr::Str(v) => Value::Str(v.get().into()),
            e => {
                use comemo::Track;
                use typst::engine::*;
                use typst::eval::*;
                use typst::foundations::*;
                use typst::introspection::*;

                let mut locator = Locator::default();
                let introspector = Introspector::default();
                let mut tracer = Tracer::new();
                let engine = Engine {
                    world: self.world().track(),
                    route: Route::default(),
                    introspector: introspector.track(),
                    locator: &mut locator,
                    tracer: tracer.track_mut(),
                };

                let context = Context::none();
                let mut vm = Vm::new(
                    engine,
                    context.track(),
                    Scopes::new(Some(self.world().library())),
                    Span::detached(),
                );

                return e.eval(&mut vm).ok();
            }
        })
    }
}

/// The context for searching in the workspace.
pub struct SearchCtx<'b, 'w> {
    /// The inner analysis context.
    pub ctx: &'b mut AnalysisContext<'w>,
    /// The set of files that have been searched.
    pub searched: HashSet<TypstFileId>,
    /// The files that need to be searched.
    pub worklist: Vec<TypstFileId>,
}

impl SearchCtx<'_, '_> {
    /// Push a file to the worklist.
    pub fn push(&mut self, id: TypstFileId) -> bool {
        if self.searched.insert(id) {
            self.worklist.push(id);
            true
        } else {
            false
        }
    }

    /// Push the dependents of a file to the worklist.
    pub fn push_dependents(&mut self, id: TypstFileId) {
        let deps = self.ctx.module_dependencies().get(&id);
        let dependents = deps.map(|e| e.dependents.clone()).into_iter().flatten();
        for dep in dependents {
            self.push(dep);
        }
    }
}
