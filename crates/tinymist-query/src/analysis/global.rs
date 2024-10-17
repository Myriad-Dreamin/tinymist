use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::HashSet, ops::Deref};

use comemo::{Track, Tracked};
use lsp_types::Url;
use once_cell::sync::OnceCell;
use reflexo::hash::{hash128, FxDashMap};
use reflexo::{debug_loc::DataSource, ImmutPath};
use tinymist_world::LspWorld;
use tinymist_world::DETACHED_ENTRY;
use typst::diag::{eco_format, At, FileError, FileResult, PackageError, SourceResult};
use typst::engine::Route;
use typst::eval::{Eval, Tracer};
use typst::foundations::{Bytes, Module, Styles};
use typst::layout::Position;
use typst::syntax::{package::PackageSpec, Span, VirtualPath};
use typst::{model::Document, text::Font};

use crate::analysis::prelude::*;
use crate::analysis::{
    analyze_bib, analyze_expr_, analyze_import_, analyze_signature, post_type_check, BibInfo,
    DefUseInfo, DocString, ImportInfo, PathPreference, Signature, SignatureTarget, Ty, TypeScheme,
};
use crate::docs::{DocStringKind, SignatureDocs, VarDocs};
use crate::syntax::{
    construct_module_dependencies, find_expr_in_import, get_deref_target, resolve_id_by_path,
    scan_workspace_files, DerefTarget, LexicalHierarchy, ModuleDependency,
};
use crate::upstream::{tooltip_, Tooltip};
use crate::{
    lsp_to_typst, path_to_url, typst_to_lsp, LspPosition, LspRange, PositionEncoding, TypstRange,
    VersionedDocument,
};

/// The analysis data holds globally.
#[derive(Default)]
pub struct Analysis {
    /// The position encoding for the workspace.
    pub position_encoding: PositionEncoding,
    /// The position encoding for the workspace.
    pub enable_periscope: bool,
    /// The global caches for analysis.
    pub caches: AnalysisGlobalCaches,
    /// The global caches for analysis.
    pub workers: AnalysisGlobalWorkers,
}

impl Analysis {
    /// Get estimated memory usage of the analysis data.
    pub fn estimated_memory(&self) -> usize {
        let _ = LexicalHierarchy::estimated_memory;
        // todo: implement
        // self.caches.modules.capacity() * 32
        //     + self .caches .modules .values() .map(|v| { v.def_use_lexical_hierarchy
        //       .output .read() .as_ref() .map_or(0, |e| e.iter().map(|e|
        //       e.estimated_memory()).sum()) }) .sum::<usize>()
        0
    }

    /// Get a snapshot of the analysis data.
    pub fn snapshot<'a>(
        &'a self,
        root: ImmutPath,
        resources: &'a dyn AnalysisResources,
    ) -> AnalysisContext<'a> {
        AnalysisContext::new(root, resources, self)
    }

    /// Clear all cached resources.
    pub fn clear_cache(&self) {
        self.caches.signatures.clear();
        self.caches.static_signatures.clear();
        self.caches.docstrings.clear();
        self.caches.terms.clear();
        self.caches.def_use.clear();
        self.caches.type_check.clear();
    }
}

/// A global (compiler server spanned) cache for all level of analysis results
/// of a module.
#[derive(Default)]
pub struct AnalysisGlobalCaches {
    lifetime: AtomicU64,
    clear_lifetime: AtomicU64,
    def_use: FxDashMap<u128, (u64, Option<Arc<DefUseInfo>>)>,
    type_check: FxDashMap<u128, (u64, Option<Arc<TypeScheme>>)>,
    static_signatures: FxDashMap<u128, (u64, Source, usize, Option<Signature>)>,
    docstrings: FxDashMap<u128, (u64, Option<Arc<DocString>>)>,
    signatures: FxDashMap<u128, (u64, Func, Option<Signature>)>,
    terms: FxDashMap<u128, (u64, Value, Ty)>,
}

/// A cache for all level of analysis results of a module.
#[derive(Default)]
pub struct AnalysisCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisCache>,
    completion_files: OnceCell<Vec<PathBuf>>,
    root_files: OnceCell<Vec<TypstFileId>>,
    module_deps: OnceCell<HashMap<TypstFileId, ModuleDependency>>,
}

/// A cache for module-level analysis results of a module.
///
/// You should not holds across requests, because source code may change.
#[derive(Default)]
pub struct ModuleAnalysisCache {
    def_use: OnceCell<Option<Arc<DefUseInfo>>>,
    type_check: OnceCell<Option<Arc<TypeScheme>>>,
}

impl ModuleAnalysisCache {
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

    /// Try to get the type check information of a file.
    pub(crate) fn type_check(&self) -> Option<Arc<TypeScheme>> {
        self.type_check.get().cloned().flatten()
    }

    /// Compute the type check information of a file.
    pub(crate) fn compute_type_check(
        &self,
        f: impl FnOnce() -> Option<Arc<TypeScheme>>,
    ) -> Option<Arc<TypeScheme>> {
        self.type_check.get_or_init(f).clone()
    }
}

/// The resources for analysis.
pub trait AnalysisResources {
    /// Get the world surface for Typst compiler.
    fn world(&self) -> &LspWorld;

    /// Resolve the real path for a package spec.
    fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError>;

    /// Get all the files in the workspace.
    fn dependencies(&self) -> EcoVec<ImmutPath>;

    /// Resolve extra font information.
    fn font_info(&self, _font: Font) -> Option<Arc<DataSource>> {
        None
    }

    /// Get the local packages and their descriptions.
    fn local_packages(&self) -> EcoVec<PackageSpec> {
        EcoVec::new()
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

/// Shared workers to limit resource usage
#[derive(Default)]
pub struct AnalysisGlobalWorkers {
    /// A possible long running import dynamic analysis task
    import: RateLimiter,
    /// A possible long running expression dynamic analysis task
    expression: RateLimiter,
    /// A possible long running tooltip dynamic analysis task
    tooltip: RateLimiter,
}

/// The context for analyzers.
pub struct AnalysisContext<'a> {
    /// The root of the workspace.
    /// This means that the analysis result won't be valid if the root directory
    /// changes.
    pub root: ImmutPath,
    /// The world surface for Typst compiler
    pub resources: &'a dyn AnalysisResources,
    /// The analysis data
    pub analysis: &'a Analysis,
    /// The caches for analysis.
    lifetime: u64,
    /// Local caches for analysis.
    caches: AnalysisCaches,
}

// todo: gc in new thread
impl<'w> Drop for AnalysisContext<'w> {
    fn drop(&mut self) {
        self.gc();
    }
}

impl<'w> AnalysisContext<'w> {
    /// Create a new analysis context.
    pub fn new(root: ImmutPath, resources: &'w dyn AnalysisResources, a: &'w Analysis) -> Self {
        // self.caches.lifetime += 1;
        let lifetime = a.caches.lifetime.fetch_add(1, Ordering::SeqCst);
        Self {
            root,
            resources,
            analysis: a,
            lifetime,
            caches: AnalysisCaches::default(),
        }
    }

    /// Get the world surface for Typst compiler.
    pub fn world(&self) -> &'w LspWorld {
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
                    &self.root,
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

    /// Get file's id by its path
    pub fn file_id_by_path(&self, p: &Path) -> FileResult<TypstFileId> {
        // todo: source in packages
        let relative_path = p.strip_prefix(&self.root).map_err(|_| {
            FileError::Other(Some(eco_format!(
                "not in root, path is {p:?}, root is {:?}",
                self.root
            )))
        })?;

        Ok(TypstFileId::new(None, VirtualPath::new(relative_path)))
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
            None => self.root.clone(),
        };

        // Join the path to the root. If it tries to escape, deny
        // access. Note: It can still escape via symlinks.
        id.vpath().resolve(&root).ok_or(FileError::AccessDenied)
    }

    /// Resolve the uri for a file id.
    pub fn uri_for_id(&self, id: TypstFileId) -> Result<Url, FileError> {
        self.path_for_id(id).and_then(|e| {
            path_to_url(&e)
                .map_err(|e| FileError::Other(Some(eco_format!("convert to url: {e:?}"))))
        })
    }

    /// Get the content of a file by file id.
    pub fn file_by_id(&self, id: TypstFileId) -> FileResult<Bytes> {
        self.world().file(id)
    }

    /// Get the source of a file by file id.
    pub fn source_by_id(&self, id: TypstFileId) -> FileResult<Source> {
        self.world().source(id)
    }

    /// Get the source of a file by file path.
    pub fn source_by_path(&self, p: &Path) -> FileResult<Source> {
        // todo: source cache
        self.source_by_id(self.file_id_by_path(p)?)
    }

    /// Get a module by file id.
    pub fn module_by_id(&self, fid: TypstFileId) -> SourceResult<Module> {
        let source = self.source_by_id(fid).at(Span::detached())?;
        self.module_by_src(source)
    }

    /// Get a module by string.
    pub fn module_by_str(&self, rr: String) -> Option<Module> {
        let src = Source::new(*DETACHED_ENTRY, rr);
        self.module_by_src(src).ok()
    }

    /// Get (Create) a module by source.
    pub fn module_by_src(&self, source: Source) -> SourceResult<Module> {
        let route = Route::default();
        let mut tracer = Tracer::default();

        typst::eval::eval(
            (self.world() as &dyn World).track(),
            route.track(),
            tracer.track_mut(),
            &source,
        )
    }

    /// Get a syntax object at a position.
    pub fn deref_syntax_at<'s>(
        &self,
        source: &'s Source,
        position: LspPosition,
        shift: usize,
    ) -> Option<DerefTarget<'s>> {
        let (_, deref_target) = self.deref_syntax_at_(source, position, shift)?;
        deref_target
    }

    /// Get a syntax object at a position.
    pub fn deref_syntax_at_<'s>(
        &self,
        source: &'s Source,
        position: LspPosition,
        shift: usize,
    ) -> Option<(usize, Option<DerefTarget<'s>>)> {
        let offset = self.to_typst_pos(position, source)?;
        let cursor = ceil_char_boundary(source.text(), offset + shift);

        let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        Some((cursor, get_deref_target(node, cursor)))
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

    /// Convert a Typst range to a LSP range.
    pub fn to_lsp_range_(&mut self, position: TypstRange, fid: TypstFileId) -> Option<LspRange> {
        let w = fid
            .vpath()
            .as_rootless_path()
            .extension()
            .and_then(|e| e.to_str());
        // yaml/yml/bib
        if matches!(w, Some("yaml" | "yml" | "bib")) {
            let bytes = self.file_by_id(fid).ok()?;
            let bytes_len = bytes.len();
            let loc = get_loc_info(bytes)?;
            // binary search
            let start = find_loc(bytes_len, &loc, position.start, self.position_encoding())?;
            let end = find_loc(bytes_len, &loc, position.end, self.position_encoding())?;
            return Some(LspRange { start, end });
        }

        let source = self.source_by_id(fid).ok()?;

        Some(self.to_lsp_range(position, &source))
    }

    pub(crate) fn signature_dyn(&mut self, func: Func) -> Signature {
        log::debug!("check runtime func {func:?}");
        analyze_signature(self, SignatureTarget::Runtime(func)).unwrap()
    }

    pub(crate) fn type_of_func(&mut self, func: Func) -> Signature {
        log::debug!("convert runtime func {func:?}");
        analyze_signature(self, SignatureTarget::Convert(func)).unwrap()
    }

    /// Compute the signature of a function.
    pub fn compute_signature(
        &mut self,
        func: SignatureTarget,
        compute: impl FnOnce(&mut Self) -> Option<Signature>,
    ) -> Option<Signature> {
        if let Some(sig) = self.get_signature(&func) {
            return sig;
        }
        let res = compute(self);
        match func {
            SignatureTarget::Def(source, r) => {
                let cache_key = (source, r.range.start);
                let h = hash128(&cache_key);
                let slot = self.analysis.caches.static_signatures.entry(h);
                let slot = slot.or_insert_with(|| (self.lifetime, cache_key.0, cache_key.1, res));
                slot.3.clone()
            }
            SignatureTarget::SyntaxFast(source, node) => {
                let cache_key = (source, node.offset(), true);
                self.analysis
                    .caches
                    .static_signatures
                    .entry(hash128(&cache_key))
                    .or_insert_with(|| (self.lifetime, cache_key.0, cache_key.1, res))
                    .3
                    .clone()
            }
            SignatureTarget::Syntax(source, node) => {
                let cache_key = (source, node.offset());
                self.analysis
                    .caches
                    .static_signatures
                    .entry(hash128(&cache_key))
                    .or_insert_with(|| (self.lifetime, cache_key.0, cache_key.1, res))
                    .3
                    .clone()
            }
            SignatureTarget::Convert(rt) => self
                .analysis
                .caches
                .signatures
                .entry(hash128(&(&rt, true)))
                .or_insert_with(|| (self.lifetime, rt, res))
                .2
                .clone(),
            SignatureTarget::Runtime(rt) => self
                .analysis
                .caches
                .signatures
                .entry(hash128(&rt))
                .or_insert_with(|| (self.lifetime, rt, res))
                .2
                .clone(),
        }
    }

    /// Get the signature of a function.
    fn get_signature(&self, func: &SignatureTarget) -> Option<Option<Signature>> {
        match func {
            SignatureTarget::Def(source, r) => {
                // todo: check performance on peeking signature source frequently
                let cache_key = (source, r.range.start);
                self.analysis
                    .caches
                    .static_signatures
                    .get(&hash128(&cache_key))
                    .and_then(|slot| (cache_key.1 == slot.2).then_some(slot.3.clone()))
            }
            SignatureTarget::SyntaxFast(source, node) => {
                // todo: check performance on peeking signature source frequently
                let cache_key = (source, node.offset(), true);
                self.analysis
                    .caches
                    .static_signatures
                    .get(&hash128(&cache_key))
                    .and_then(|slot| (cache_key.1 == slot.2).then_some(slot.3.clone()))
            }
            SignatureTarget::Syntax(source, node) => {
                // todo: check performance on peeking signature source frequently
                let cache_key = (source, node.offset());
                self.analysis
                    .caches
                    .static_signatures
                    .get(&hash128(&cache_key))
                    .and_then(|slot| (cache_key.1 == slot.2).then_some(slot.3.clone()))
            }
            SignatureTarget::Convert(rt) => self
                .analysis
                .caches
                .signatures
                .get(&hash128(&(&rt, true)))
                .and_then(|slot| (rt == &slot.1).then_some(slot.2.clone())),
            SignatureTarget::Runtime(rt) => self
                .analysis
                .caches
                .signatures
                .get(&hash128(rt))
                .and_then(|slot| (rt == &slot.1).then_some(slot.2.clone())),
        }
    }

    pub(crate) fn type_of_value(&mut self, val: &Value) -> Ty {
        log::debug!("convert runtime value {val:?}");

        // todo: check performance on peeking signature source frequently
        let cache_key = val;
        let cached = self
            .analysis
            .caches
            .terms
            .get(&hash128(&cache_key))
            .and_then(|slot| (cache_key == &slot.1).then_some(slot.2.clone()));
        if let Some(cached) = cached {
            return cached;
        }

        let res = crate::analysis::term_value(self, val);

        self.analysis
            .caches
            .terms
            .entry(hash128(&cache_key))
            .or_insert_with(|| (self.lifetime, cache_key.clone(), res.clone()));

        res
    }

    pub(crate) fn variable_docs(&mut self, pos: &LinkedNode) -> Option<VarDocs> {
        crate::docs::variable_docs(self, pos)
    }

    pub(crate) fn signature_docs(&mut self, runtime_fn: &Value) -> Option<SignatureDocs> {
        crate::docs::signature_docs(self, runtime_fn, None)
    }

    pub(crate) fn compute_docstring(
        &self,
        fid: TypstFileId,
        docs: String,
        kind: DocStringKind,
    ) -> Option<Arc<DocString>> {
        let h = hash128(&(&fid, &docs, &kind));
        let res = if let Some(res) = self.analysis.caches.docstrings.get(&h) {
            res.1.clone()
        } else {
            let res = crate::analysis::tyck::compute_docstring(self, fid, docs, kind).map(Arc::new);
            self.analysis
                .caches
                .docstrings
                .insert(h, (self.lifetime, res.clone()));
            res
        };

        res
    }

    /// Get the type check information of a source file.
    pub(crate) fn type_check(&mut self, source: &Source) -> Option<Arc<TypeScheme>> {
        let fid = source.id();

        if let Some(res) = self.caches.modules.entry(fid).or_default().type_check() {
            return Some(res);
        }

        let def_use = self.def_use(source.clone())?;

        let h = hash128(&(&source, &def_use));

        let res = if let Some(res) = self.analysis.caches.type_check.get(&h) {
            res.1.clone()
        } else {
            let res = crate::analysis::type_check(self, source.clone());
            self.analysis
                .caches
                .type_check
                .insert(h, (self.lifetime, res.clone()));
            res
        };

        self.caches
            .modules
            .entry(fid)
            .or_default()
            .compute_type_check(|| res.clone());

        res
    }

    /// Get the import information of a source file.
    pub fn import_info(&mut self, source: Source) -> Option<Arc<ImportInfo>> {
        use comemo::Track;
        let w = self.resources.world();
        let w = (w as &dyn World).track();

        let token = &self.analysis.workers.import;
        token.enter(|| import_info(w, source))
    }

    /// Get the def-use information of a source file.
    pub fn def_use(&mut self, source: Source) -> Option<Arc<DefUseInfo>> {
        let mut search_ctx = self.fork_for_search();

        Self::def_use_(&mut search_ctx, source)
    }

    /// Get the def-use information of a source file.
    pub fn def_use_(ctx: &mut SearchCtx<'_, 'w>, source: Source) -> Option<Arc<DefUseInfo>> {
        let fid = source.id();

        if let Some(res) = ctx.ctx.caches.modules.entry(fid).or_default().def_use() {
            return Some(res);
        }

        if !ctx.searched.insert(fid) {
            return None;
        }

        let l = def_use_lexical_hierarchy(source.clone())?;
        let m = ctx.ctx.import_info(source.clone())?;
        let deps = m
            .imports
            .iter()
            .flat_map(|e| e.1)
            .map(|e| Self::def_use_(ctx, e.clone()))
            .collect::<Vec<_>>();

        let key = (&source, &l, &m, deps);
        let h = hash128(&key);

        let res = if let Some(res) = ctx.ctx.analysis.caches.def_use.get(&h) {
            res.1.clone()
        } else {
            let res = crate::analysis::get_def_use_inner(ctx, source, l, m);
            ctx.ctx
                .analysis
                .caches
                .def_use
                .insert(h, (ctx.ctx.lifetime, res.clone()));
            res
        };

        ctx.ctx
            .caches
            .modules
            .entry(fid)
            .or_default()
            .compute_def_use(|| res.clone());
        res
    }

    /// Get bib info of a source file.
    pub fn analyze_bib(
        &mut self,
        span: Span,
        bib_paths: impl Iterator<Item = EcoString>,
    ) -> Option<Arc<BibInfo>> {
        use comemo::Track;
        let w = self.resources.world();
        let w = (w as &dyn World).track();

        bib_info(w, span, bib_paths.collect())
    }

    pub(crate) fn with_vm<T>(&self, f: impl FnOnce(&mut typst::eval::Vm) -> T) -> T {
        crate::upstream::with_vm((self.world() as &dyn World).track(), f)
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

    pub(crate) fn type_of(&mut self, rr: &SyntaxNode) -> Option<Ty> {
        self.type_of_span(rr.span())
    }

    pub(crate) fn type_of_span(&mut self, s: Span) -> Option<Ty> {
        let id = s.id()?;
        let source = self.source_by_id(id).ok()?;
        self.type_of_span_(&source, s)
    }

    pub(crate) fn type_of_span_(&mut self, source: &Source, s: Span) -> Option<Ty> {
        self.type_check(source)?.type_of_span(s)
    }

    pub(crate) fn literal_type_of_node(&mut self, k: LinkedNode) -> Option<Ty> {
        let id = k.span().id()?;
        let source = self.source_by_id(id).ok()?;
        let ty_chk = self.type_check(&source)?;

        post_type_check(self, &ty_chk, k.clone()).or_else(|| ty_chk.type_of_span(k.span()))
    }

    /// Get module import at location.
    pub fn module_ins_at(&mut self, def_fid: TypstFileId, cursor: usize) -> Option<Value> {
        let def_src = self.source_by_id(def_fid).ok()?;
        let def_root = LinkedNode::new(def_src.root());
        let mod_exp = find_expr_in_import(def_root.leaf_at_compat(cursor)?)?;
        let mod_import = mod_exp.parent()?.clone();
        let mod_import_node = mod_import.cast::<ast::ModuleImport>()?;
        let import_path = mod_import.find(mod_import_node.source().span())?;

        self.analyze_import(&import_path)
    }

    /// Try to load a module from the current source file.
    pub fn analyze_import(&mut self, source: &LinkedNode) -> Option<Value> {
        let token = &self.analysis.workers.import;
        token.enter(|| analyze_import_(self.world(), source))
    }

    /// Try to determine a set of possible values for an expression.
    pub fn analyze_expr(&mut self, node: &LinkedNode) -> EcoVec<(Value, Option<Styles>)> {
        let token = &self.analysis.workers.expression;
        token.enter(|| analyze_expr_(self.world(), node))
    }

    /// Describe the item under the cursor.
    ///
    /// Passing a `document` (from a previous compilation) is optional, but
    /// enhances the autocompletions. Label completions, for instance, are
    /// only generated when the document is available.
    pub fn tooltip(
        &mut self,
        document: Option<&Document>,
        source: &Source,
        cursor: usize,
    ) -> Option<Tooltip> {
        let token = &self.analysis.workers.tooltip;
        token.enter(|| tooltip_(self.world(), document, source, cursor))
    }

    fn gc(&self) {
        let lifetime = self.lifetime;
        loop {
            let latest_clear_lifetime = self.analysis.caches.clear_lifetime.load(Ordering::Relaxed);
            if latest_clear_lifetime >= lifetime {
                return;
            }

            if self.analysis.caches.clear_lifetime.compare_exchange(
                latest_clear_lifetime,
                lifetime,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) != Ok(latest_clear_lifetime)
            {
                continue;
            }

            break;
        }

        self.analysis
            .caches
            .static_signatures
            .retain(|_, (l, _, _, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .terms
            .retain(|_, (l, _, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .docstrings
            .retain(|_, (l, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .signatures
            .retain(|_, (l, _, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .def_use
            .retain(|_, (l, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .type_check
            .retain(|_, (l, _)| lifetime - *l < 60);
    }
}

fn ceil_char_boundary(text: &str, mut cursor: usize) -> usize {
    // while is not char boundary, move cursor to right
    while cursor < text.len() && !text.is_char_boundary(cursor) {
        cursor += 1;
    }

    cursor.min(text.len())
}

#[comemo::memoize]
fn def_use_lexical_hierarchy(source: Source) -> Option<EcoVec<LexicalHierarchy>> {
    crate::syntax::get_lexical_hierarchy(source, crate::syntax::LexicalScopeKind::DefUse)
}

#[comemo::memoize]
fn import_info(w: Tracked<dyn World + '_>, source: Source) -> Option<Arc<ImportInfo>> {
    let l = def_use_lexical_hierarchy(source.clone())?;
    crate::analysis::get_import_info(w, source, l)
}

#[comemo::memoize]
fn bib_info(
    w: Tracked<dyn World + '_>,
    span: Span,
    bib_paths: EcoVec<EcoString>,
) -> Option<Arc<BibInfo>> {
    let id = span.id()?;

    let files = bib_paths
        .iter()
        .flat_map(|s| {
            let id = resolve_id_by_path(w.deref(), id, s)?;
            Some((id, w.file(id).ok()?))
        })
        .collect::<EcoVec<_>>();
    analyze_bib(files)
}

#[comemo::memoize]
fn get_loc_info(bytes: Bytes) -> Option<EcoVec<(usize, String)>> {
    let mut loc = EcoVec::new();
    let mut offset = 0;
    for line in bytes.split(|e| *e == b'\n') {
        loc.push((offset, String::from_utf8(line.to_owned()).ok()?));
        offset += line.len() + 1;
    }
    Some(loc)
}

fn find_loc(
    len: usize,
    loc: &EcoVec<(usize, String)>,
    mut offset: usize,
    encoding: PositionEncoding,
) -> Option<LspPosition> {
    if offset > len {
        offset = len;
    }

    let r = match loc.binary_search_by_key(&offset, |line| line.0) {
        Ok(i) => i,
        Err(i) => i - 1,
    };

    let (start, s) = loc.get(r)?;
    let byte_offset = offset.saturating_sub(*start);

    let column_prefix = if byte_offset <= s.len() {
        &s[..byte_offset]
    } else {
        let line = (r + 1) as u32;
        return Some(LspPosition { line, character: 0 });
    };

    let line = r as u32;
    let character = match encoding {
        PositionEncoding::Utf8 => column_prefix.chars().count(),
        PositionEncoding::Utf16 => column_prefix.chars().map(|c| c.len_utf16()).sum(),
    } as u32;

    Some(LspPosition { line, character })
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

/// A rate limiter on some (cpu-heavy) action
#[derive(Default)]
pub struct RateLimiter {
    token: std::sync::Mutex<()>,
}

impl RateLimiter {
    /// Executes some (cpu-heavy) action with rate limit
    #[must_use]
    pub fn enter<T>(&self, f: impl FnOnce() -> T) -> T {
        let _c = self.token.lock().unwrap();
        f()
    }
}
