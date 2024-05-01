use std::sync::atomic::AtomicBool;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    path::{Path, PathBuf},
    sync::Arc,
};

use ecow::EcoVec;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use reflexo::hash::hash128;
use reflexo::{cow_mut::CowMut, debug_loc::DataSource, ImmutPath};
use typst::eval::Eval;
use typst::foundations;
use typst::syntax::{LinkedNode, SyntaxNode};
use typst::{
    diag::{eco_format, FileError, FileResult, PackageError},
    syntax::{package::PackageSpec, Source, Span, VirtualPath},
    World,
};
use typst::{foundations::Value, syntax::ast, text::Font};
use typst::{layout::Position, syntax::FileId as TypstFileId};

use super::{
    post_type_check, DefUseInfo, FlowType, ImportInfo, PathPreference, Signature, SignatureTarget,
    TypeCheckInfo,
};
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
#[derive(Default)]
pub struct ModuleAnalysisCache {
    source: OnceCell<FileResult<Source>>,
    top_level_eval: OnceCell<Option<Arc<TypeCheckInfo>>>,
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

    /// Try to get the top-level evaluation information of a file.
    pub(crate) fn type_check(&self) -> Option<Arc<TypeCheckInfo>> {
        self.top_level_eval.get().cloned().flatten()
    }

    /// Compute the top-level evaluation information of a file.
    pub(crate) fn compute_type_check(
        &self,
        f: impl FnOnce() -> Option<Arc<TypeCheckInfo>>,
    ) -> Option<Arc<TypeCheckInfo>> {
        self.top_level_eval.get_or_init(f).clone()
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
                    v.def_use_lexical_hierarchy
                        .output
                        .read()
                        .as_ref()
                        .map_or(0, |e| e.iter().map(|e| e.estimated_memory()).sum())
                })
                .sum::<usize>()
    }

    fn gc(&mut self) {
        self.caches
            .signatures
            .retain(|_, (l, _, _)| (self.caches.lifetime - *l) < 30);
    }
}

struct ComputingNode<Inputs, Output> {
    name: &'static str,
    computing: AtomicBool,
    inputs: RwLock<Option<Inputs>>,
    output: RwLock<Option<Output>>,
}

pub(crate) trait ComputeDebug {
    fn compute_debug_repr(&self) -> impl std::fmt::Debug;
}

impl ComputeDebug for Source {
    fn compute_debug_repr(&self) -> impl std::fmt::Debug {
        self.id()
    }
}
impl ComputeDebug for EcoVec<LexicalHierarchy> {
    fn compute_debug_repr(&self) -> impl std::fmt::Debug {
        self.len()
    }
}

impl ComputeDebug for Arc<ImportInfo> {
    fn compute_debug_repr(&self) -> impl std::fmt::Debug {
        self.imports.len()
    }
}

impl<A, B> ComputeDebug for (A, B)
where
    A: ComputeDebug,
    B: ComputeDebug,
{
    fn compute_debug_repr(&self) -> impl std::fmt::Debug {
        (self.0.compute_debug_repr(), self.1.compute_debug_repr())
    }
}

impl<Inputs, Output> ComputingNode<Inputs, Output> {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            computing: AtomicBool::new(false),
            inputs: RwLock::new(None),
            output: RwLock::new(None),
        }
    }

    fn compute(
        &self,
        inputs: Inputs,
        compute: impl FnOnce(Option<Inputs>, Inputs) -> Option<Output>,
    ) -> Result<Option<Output>, ()>
    where
        Inputs: ComputeDebug + Hash + Clone,
        Output: Clone,
    {
        if self
            .computing
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(());
        }
        let input_cmp = self.inputs.read();
        let res = Ok(match input_cmp.as_ref() {
            Some(s) if reflexo::hash::hash128(&inputs) == reflexo::hash::hash128(&s) => {
                log::debug!(
                    "{}({:?}): hit cache",
                    self.name,
                    inputs.compute_debug_repr()
                );
                self.output.read().clone()
            }
            s => {
                let s = s.cloned();
                drop(input_cmp);
                log::info!("{}({:?}): compute", self.name, inputs.compute_debug_repr());
                let output = compute(s, inputs.clone());
                *self.output.write() = output.clone();
                *self.inputs.write() = Some(inputs);
                output
            }
        });

        self.computing
            .store(false, std::sync::atomic::Ordering::SeqCst);
        res
    }
}

/// A cache for module-level analysis results of a module.
///
/// You should not holds across requests, because source code may change.
#[allow(clippy::type_complexity)]
pub struct ModuleAnalysisGlobalCache {
    def_use_lexical_hierarchy: ComputingNode<Source, EcoVec<LexicalHierarchy>>,
    type_check: Arc<ComputingNode<Source, Arc<TypeCheckInfo>>>,
    def_use: Arc<ComputingNode<(EcoVec<LexicalHierarchy>, Arc<ImportInfo>), Arc<DefUseInfo>>>,

    import: Arc<ComputingNode<EcoVec<LexicalHierarchy>, Arc<ImportInfo>>>,
    signature_source: Option<Source>,
    signatures: HashMap<usize, Signature>,
}

impl Default for ModuleAnalysisGlobalCache {
    fn default() -> Self {
        Self {
            def_use_lexical_hierarchy: ComputingNode::new("def_use_lexical_hierarchy"),
            type_check: Arc::new(ComputingNode::new("type_check")),
            import: Arc::new(ComputingNode::new("import")),
            def_use: Arc::new(ComputingNode::new("def_use")),

            signature_source: None,
            signatures: Default::default(),
        }
    }
}

/// A global (compiler server spanned) cache for all level of analysis results
/// of a module.
#[derive(Default)]
pub struct AnalysisGlobalCaches {
    lifetime: u64,
    modules: HashMap<TypstFileId, ModuleAnalysisGlobalCache>,
    signatures: HashMap<u128, (u64, foundations::Func, Signature)>,
}

impl AnalysisGlobalCaches {
    /// Get the signature of a function.
    pub fn signature(&self, source: Option<Source>, func: &SignatureTarget) -> Option<Signature> {
        match func {
            SignatureTarget::Syntax(node) => {
                // todo: check performance on peeking signature source frequently
                let cache = self.modules.get(&node.span().id()?)?;
                if cache
                    .signature_source
                    .as_ref()
                    .zip(source)
                    .map_or(true, |(s, t)| hash128(s) != hash128(&t))
                {
                    return None;
                }

                cache.signatures.get(&node.offset()).cloned()
            }
            SignatureTarget::Runtime(rt) => self
                .signatures
                .get(&hash128(rt))
                .and_then(|(_, cached_func, s)| (rt == cached_func).then_some(s.clone())),
        }
    }

    /// Compute the signature of a function.
    pub fn compute_signature(
        &mut self,
        source: Option<Source>,
        func: SignatureTarget,
        compute: impl FnOnce() -> Signature,
    ) -> Signature {
        match func {
            SignatureTarget::Syntax(node) => {
                let cache = self.modules.entry(node.span().id().unwrap()).or_default();
                // todo: check performance on peeking signature source frequently
                if cache
                    .signature_source
                    .as_ref()
                    .zip(source.as_ref())
                    .map_or(true, |(s, t)| hash128(s) != hash128(t))
                {
                    cache.signature_source = source;
                    cache.signatures.clear();
                }

                let key = node.offset();
                cache.signatures.entry(key).or_insert_with(compute).clone()
            }
            SignatureTarget::Runtime(rt) => {
                let key = hash128(&rt);
                self.signatures
                    .entry(key)
                    .or_insert_with(|| (self.lifetime, rt, compute()))
                    .2
                    .clone()
            }
        }
    }
}

/// A cache for all level of analysis results of a module.
#[derive(Default)]
pub struct AnalysisCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisCache>,
    completion_files: OnceCell<Vec<PathBuf>>,
    root_files: OnceCell<Vec<TypstFileId>>,
    module_deps: OnceCell<HashMap<TypstFileId, ModuleDependency>>,
}

/// The resources for analysis.
pub trait AnalysisResources {
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
    pub resources: &'a dyn AnalysisResources,
    /// The analysis data
    pub analysis: CowMut<'a, Analysis>,
    caches: AnalysisCaches,
}

impl<'w> AnalysisContext<'w> {
    /// Create a new analysis context.
    pub fn new(resources: &'w dyn AnalysisResources, a: Analysis) -> Self {
        Self {
            resources,
            analysis: CowMut::Owned(a),
            caches: AnalysisCaches::default(),
        }
    }

    /// Create a new analysis context with borrowing the analysis data.
    pub fn new_borrow(resources: &'w dyn AnalysisResources, a: &'w mut Analysis) -> Self {
        a.caches.lifetime += 1;
        a.gc();

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
    pub fn test_completion_files(&mut self, f: impl FnOnce() -> Vec<PathBuf>) {
        self.caches.completion_files.get_or_init(f);
    }

    #[cfg(test)]
    pub fn test_files(&mut self, f: impl FnOnce() -> Vec<TypstFileId>) {
        self.caches.root_files.get_or_init(f);
    }

    /// Get all the source files in the workspace.
    pub(crate) fn completion_files(&self, pref: &PathPreference) -> impl Iterator<Item = &PathBuf> {
        let r = pref.ext_matcher();
        self.caches
            .completion_files
            .get_or_init(|| {
                scan_workspace_files(
                    &self.analysis.root,
                    PathPreference::Special.ext_matcher(),
                    |relative_path| relative_path.to_owned(),
                )
            })
            .iter()
            .filter(move |p| {
                p.extension()
                    .and_then(|p| p.to_str())
                    .is_some_and(|e| r.is_match(e))
            })
    }

    /// Get all the source files in the workspace.
    pub fn source_files(&self) -> &Vec<TypstFileId> {
        self.caches.root_files.get_or_init(|| {
            self.completion_files(&PathPreference::Source)
                .map(|p| TypstFileId::new(None, VirtualPath::new(p.as_path())))
                .collect()
        })
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
        self.caches.modules.entry(file_id).or_default()
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

    /// Get the type check information of a source file.
    pub(crate) fn type_check(&mut self, source: Source) -> Option<Arc<TypeCheckInfo>> {
        let fid = source.id();

        if let Some(res) = self.caches.modules.entry(fid).or_default().type_check() {
            return Some(res);
        }

        let cache = self.at_module(fid);

        let tl = cache.type_check.clone();
        let res = tl
            .compute(source, |_before, after| {
                let next = crate::analysis::ty::type_check(self, after);
                next.or_else(|| tl.output.read().clone())
            })
            .ok()
            .flatten();

        self.caches
            .modules
            .entry(fid)
            .or_default()
            .compute_type_check(|| res.clone());

        res
    }

    /// Get the def-use information of a source file.
    pub fn def_use(&mut self, source: Source) -> Option<Arc<DefUseInfo>> {
        let fid = source.id();

        if let Some(res) = self.caches.modules.entry(fid).or_default().def_use() {
            return Some(res);
        }

        let cache = self.at_module(fid);
        let l = cache
            .def_use_lexical_hierarchy
            .compute(source.clone(), |_before, after| {
                cache.signatures.clear();
                crate::syntax::get_lexical_hierarchy(after, crate::syntax::LexicalScopeKind::DefUse)
            })
            .ok()
            .flatten()?;

        let source2 = source.clone();
        let m = cache
            .import
            .clone()
            .compute(l.clone(), |_before, after| {
                crate::analysis::get_import_info(self, source2, after)
            })
            .ok()
            .flatten()?;

        let cache = self.at_module(fid);
        let res = cache
            .def_use
            .clone()
            .compute((l, m), |_before, after| {
                crate::analysis::get_def_use_inner(self, source, after.0, after.1)
            })
            .ok()
            .flatten();

        self.caches
            .modules
            .entry(fid)
            .or_default()
            .compute_def_use(|| res.clone());
        res
    }

    fn at_module(&mut self, fid: TypstFileId) -> &mut ModuleAnalysisGlobalCache {
        self.analysis.caches.modules.entry(fid).or_default()
    }

    pub(crate) fn with_vm<T>(&self, f: impl FnOnce(&mut typst::eval::Vm) -> T) -> T {
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

        f(&mut vm)
    }

    pub(crate) fn const_eval(&self, rr: ast::Expr<'_>) -> Option<Value> {
        Some(match rr {
            ast::Expr::None(_) => Value::None,
            ast::Expr::Auto(_) => Value::Auto,
            ast::Expr::Bool(v) => Value::Bool(v.get()),
            ast::Expr::Int(v) => Value::Int(v.get()),
            ast::Expr::Float(v) => Value::Float(v.get()),
            ast::Expr::Numeric(v) => Value::numeric(v.get()),
            ast::Expr::Str(v) => Value::Str(v.get().into()),
            _ => return None,
        })
    }

    pub(crate) fn mini_eval(&self, rr: ast::Expr<'_>) -> Option<Value> {
        self.const_eval(rr)
            .or_else(|| self.with_vm(|vm| rr.eval(vm).ok()))
    }

    pub(crate) fn type_of(&mut self, rr: &SyntaxNode) -> Option<FlowType> {
        self.type_of_span(rr.span())
    }

    pub(crate) fn type_of_span(&mut self, s: Span) -> Option<FlowType> {
        let id = s.id()?;
        let source = self.source_by_id(id).ok()?;
        let ty_chk = self.type_check(source)?;
        ty_chk.mapping.get(&s).cloned()
    }

    pub(crate) fn literal_type_of_span(&mut self, s: Span) -> Option<FlowType> {
        let id = s.id()?;
        let source = self.source_by_id(id).ok()?;
        let k = LinkedNode::new(source.root()).find(s)?;

        self.literal_type_of_node(k)
    }

    pub(crate) fn literal_type_of_node(&mut self, k: LinkedNode) -> Option<FlowType> {
        let id = k.span().id()?;
        let source = self.source_by_id(id).ok()?;
        let ty_chk = self.type_check(source.clone())?;

        post_type_check(self, &ty_chk, k.clone()).or_else(|| ty_chk.mapping.get(&k.span()).cloned())
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
