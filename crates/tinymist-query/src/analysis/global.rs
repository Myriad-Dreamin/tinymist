use std::num::NonZeroUsize;
use std::ops::DerefMut;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::HashSet, ops::Deref};

use comemo::{Track, Tracked};
use lsp_types::Url;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use reflexo::debug_loc::DataSource;
use reflexo::hash::{hash128, FxDashMap};
use reflexo_typst::{EntryReader, WorldDeps};
use rustc_hash::FxHashMap;
use tinymist_world::LspWorld;
use tinymist_world::DETACHED_ENTRY;
use typst::diag::{eco_format, At, FileError, FileResult, SourceResult};
use typst::engine::{Route, Sink, Traced};
use typst::eval::Eval;
use typst::foundations::{Bytes, Module, Styles};
use typst::layout::Position;
use typst::model::Document;
use typst::syntax::{package::PackageSpec, Span, VirtualPath};

use crate::analysis::prelude::*;
use crate::analysis::{
    analyze_bib, analyze_import_, analyze_signature, post_type_check, BibInfo, PathPreference,
    Signature, SignatureTarget, Ty, TypeScheme,
};
use crate::docs::{SignatureDocs, VarDocs};
use crate::syntax::{
    construct_module_dependencies, find_expr_in_import, get_deref_target, resolve_id_by_path,
    scan_workspace_files, DerefTarget, ExprInfo, LexicalScope, ModuleDependency, Processing,
};
use crate::upstream::{tooltip_, Tooltip};
use crate::{
    lsp_to_typst, path_to_url, typst_to_lsp, LspPosition, LspRange, PositionEncoding,
    SemanticTokenContext, TypstRange, VersionedDocument,
};

use super::{analyze_expr_, definition, AllocStats, AnalysisStats, Definition, QueryStatGuard};

/// The analysis data holds globally.
#[derive(Default, Clone)]
pub struct Analysis {
    /// The position encoding for the workspace.
    pub position_encoding: PositionEncoding,
    /// The position encoding for the workspace.
    pub enable_periscope: bool,
    /// The global caches for analysis.
    pub caches: AnalysisGlobalCaches,
    /// The global caches for analysis.
    pub workers: Arc<AnalysisGlobalWorkers>,
    /// The global cache grid for analysis.
    pub cache_grid: Arc<Mutex<AnalysisGlobalCacheGrid>>,
    /// The semantic token context.
    pub tokens_ctx: Arc<SemanticTokenContext>,
    /// The statistics about the analyzers.
    pub analysis_stats: Arc<AnalysisStats>,
}

impl Analysis {
    /// Get a snapshot of the analysis data.
    pub fn snapshot<'a>(
        &self,
        world: LspWorld,
        resources: &'a dyn AnalysisResources,
    ) -> AnalysisContext<'a> {
        AnalysisContext::new(world, resources, self.clone())
    }

    /// Clear all cached resources.
    pub fn clear_cache(&self) {
        self.caches.signatures.clear();
        self.caches.static_signatures.clear();
        self.caches.terms.clear();
        self.cache_grid.lock().clear();
    }

    /// Lock the revision in main thread.
    #[must_use]
    pub fn lock_revision(&self) -> RevisionLock {
        let mut grid = self.cache_grid.lock();
        let revision = grid.revision;
        *grid.locked_revisions.entry(revision).or_default() += 1;
        RevisionLock {
            grid: self.cache_grid.clone(),
            revision,
        }
    }

    /// Report the statistics of the analysis.
    pub fn report_query_stats(&self) -> String {
        self.analysis_stats.report()
    }

    /// Report the statistics of the allocation.
    pub fn report_alloc_stats(&self) -> String {
        AllocStats::report(self)
    }
}

/// The resources for analysis.
pub trait AnalysisResources {
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
    /// The world surface for Typst compiler
    pub resources: &'a dyn AnalysisResources,
    /// Constructed shared context
    pub local: LocalContext,
}

impl Deref for AnalysisContext<'_> {
    type Target = LocalContext;

    fn deref(&self) -> &Self::Target {
        &self.local
    }
}

impl DerefMut for AnalysisContext<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.local
    }
}

// todo: gc in new thread
impl<'w> Drop for AnalysisContext<'w> {
    fn drop(&mut self) {
        self.gc();
    }
}

impl<'w> AnalysisContext<'w> {
    /// Create a new analysis context.
    pub fn new(world: LspWorld, resources: &'w dyn AnalysisResources, a: Analysis) -> Self {
        let lifetime = a.caches.lifetime.fetch_add(1, Ordering::SeqCst);
        let slot = a.cache_grid.lock().find_revision(world.revision());
        Self {
            resources,
            local: LocalContext {
                caches: AnalysisCaches::default(),
                shared: Arc::new(SharedContext {
                    slot,
                    lifetime,
                    world,
                    analysis: a,
                }),
            },
        }
    }

    /// Resolve extra font information.
    pub fn font_info(&self, font: typst::text::Font) -> Option<Arc<DataSource>> {
        self.world().font_resolver.describe_font(&font)
    }

    /// Get the world surface for Typst compiler.
    pub fn world(&self) -> &LspWorld {
        &self.shared.world
    }

    /// Get the shared context.
    pub fn shared(&self) -> &Arc<SharedContext> {
        &self.local.shared
    }

    /// Get the shared context.
    pub fn shared_(&self) -> Arc<SharedContext> {
        self.local.shared.clone()
    }

    /// Fork a new context for searching in the workspace.
    pub fn fork_for_search<'s>(&'s mut self) -> SearchCtx<'s, 'w> {
        SearchCtx {
            ctx: self,
            searched: Default::default(),
            worklist: Default::default(),
        }
    }

    pub(crate) fn variable_docs(&mut self, pos: &LinkedNode) -> Option<VarDocs> {
        crate::docs::variable_docs(self, pos)
    }

    pub(crate) fn preload_package(&self, entry_point: TypstFileId) {
        self.shared_().preload_package(entry_point);
    }

    pub(crate) fn with_vm<T>(&self, f: impl FnOnce(&mut typst::eval::Vm) -> T) -> T {
        crate::upstream::with_vm((self.world() as &dyn World).track(), f)
    }

    pub(crate) fn const_eval(&self, rr: ast::Expr<'_>) -> Option<Value> {
        SharedContext::const_eval(rr)
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
        self.type_check(source).type_of_span(s)
    }

    pub(crate) fn literal_type_of_node(&mut self, k: LinkedNode) -> Option<Ty> {
        let id = k.span().id()?;
        let source = self.source_by_id(id).ok()?;
        let ty_chk = self.type_check(&source);

        let ty = post_type_check(self.shared_(), &ty_chk, k.clone())
            .or_else(|| ty_chk.type_of_span(k.span()))?;
        Some(ty_chk.simplify(ty, false))
    }

    /// Get module import at location.
    pub fn module_ins_at(&mut self, def_fid: TypstFileId, cursor: usize) -> Option<Value> {
        let def_src = self.source_by_id(def_fid).ok()?;
        let def_root = LinkedNode::new(def_src.root());
        let mod_exp = find_expr_in_import(def_root.leaf_at_compat(cursor)?)?;
        let mod_import = mod_exp.parent()?.clone();
        let mod_import_node = mod_import.cast::<ast::ModuleImport>()?;
        self.analyze_import(mod_import_node.source().to_untyped()).1
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
            .def_signatures
            .retain(|(l, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .static_signatures
            .retain(|(l, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .terms
            .retain(|(l, _)| lifetime - *l < 60);
        self.analysis
            .caches
            .signatures
            .retain(|(l, _)| lifetime - *l < 60);
    }
}

/// The local context for analyzers.
pub struct LocalContext {
    /// Local caches for analysis.
    pub caches: AnalysisCaches,
    /// Constructed shared context
    pub shared: Arc<SharedContext>,
}

impl Deref for LocalContext {
    type Target = Arc<SharedContext>;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl DerefMut for LocalContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shared
    }
}

impl LocalContext {
    #[cfg(test)]
    pub fn test_completion_files(&mut self, f: impl FnOnce() -> Vec<TypstFileId>) {
        self.caches.completion_files.get_or_init(f);
    }

    #[cfg(test)]
    pub fn test_files(&mut self, f: impl FnOnce() -> Vec<TypstFileId>) {
        self.caches.root_files.get_or_init(f);
    }

    /// Get all the source files in the workspace.
    pub(crate) fn completion_files(
        &self,
        pref: &PathPreference,
    ) -> impl Iterator<Item = &TypstFileId> {
        let r = pref.ext_matcher();
        self.caches
            .completion_files
            .get_or_init(|| {
                if let Some(root) = self.world.workspace_root() {
                    scan_workspace_files(&root, PathPreference::Special.ext_matcher(), |p| {
                        TypstFileId::new(None, VirtualPath::new(p))
                    })
                } else {
                    vec![]
                }
            })
            .iter()
            .filter(move |p| {
                p.vpath()
                    .as_rooted_path()
                    .extension()
                    .and_then(|p| p.to_str())
                    .is_some_and(|e| r.is_match(e))
            })
    }

    /// Get all the source files in the workspace.
    pub fn source_files(&self) -> &Vec<TypstFileId> {
        self.caches.root_files.get_or_init(|| {
            self.completion_files(&PathPreference::Source)
                .copied()
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

    /// Get the expression information of a source file.
    pub(crate) fn expr_stage(&mut self, source: &Source) -> Arc<ExprInfo> {
        let id = source.id();
        let cache = &self.caches.modules.entry(id).or_default().expr_stage;
        cache.get_or_init(|| self.shared.expr_stage(source)).clone()
    }

    /// Get the type check information of a source file.
    pub(crate) fn type_check(&mut self, source: &Source) -> Arc<TypeScheme> {
        let id = source.id();
        let cache = &self.caches.modules.entry(id).or_default().type_check;
        cache.get_or_init(|| self.shared.type_check(source)).clone()
    }
}

/// The shared analysis context for analyzers.
pub struct SharedContext {
    /// The caches lifetime tick for analysis.
    pub lifetime: u64,
    /// Get the world surface for Typst compiler.
    pub world: LspWorld,
    /// The analysis data
    pub analysis: Analysis,
    /// The revision slot
    slot: Arc<RevisionSlot>,
}

impl SharedContext {
    /// Get revision of current analysis
    pub fn revision(&self) -> usize {
        self.slot.revision
    }

    fn query_stat(&self, id: TypstFileId, query: &'static str) -> QueryStatGuard {
        let stats = &self.analysis.analysis_stats.query_stats;
        let entry = stats.entry(id).or_default();
        let entry = entry.entry(query).or_default();
        QueryStatGuard {
            bucket: entry.clone(),
            since: std::time::SystemTime::now(),
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
    pub fn to_lsp_range_(&self, position: TypstRange, fid: TypstFileId) -> Option<LspRange> {
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

    /// Resolve the real path for a file id.
    pub fn path_for_id(&self, id: TypstFileId) -> Result<PathBuf, FileError> {
        self.world.path_for_id(id)
    }

    /// Resolve the uri for a file id.
    pub fn uri_for_id(&self, id: TypstFileId) -> Result<Url, FileError> {
        self.path_for_id(id).and_then(|e| {
            path_to_url(&e)
                .map_err(|e| FileError::Other(Some(eco_format!("convert to url: {e:?}"))))
        })
    }

    /// Get file's id by its path
    pub fn file_id_by_path(&self, p: &Path) -> FileResult<TypstFileId> {
        // todo: source in packages
        let root = self.world.workspace_root().ok_or_else(|| {
            let reason = eco_format!("workspace root not found");
            FileError::Other(Some(reason))
        })?;
        let relative_path = p.strip_prefix(&root).map_err(|_| {
            let reason = eco_format!("access denied, path: {p:?}, root: {root:?}");
            FileError::Other(Some(reason))
        })?;

        Ok(TypstFileId::new(None, VirtualPath::new(relative_path)))
    }

    /// Get the content of a file by file id.
    pub fn file_by_id(&self, id: TypstFileId) -> FileResult<Bytes> {
        self.world.file(id)
    }

    /// Get the source of a file by file id.
    pub fn source_by_id(&self, id: TypstFileId) -> FileResult<Source> {
        self.world.source(id)
    }

    /// Get the source of a file by file path.
    pub fn source_by_path(&self, p: &Path) -> FileResult<Source> {
        // todo: source cache
        self.source_by_id(self.file_id_by_path(p)?)
    }

    /// Get a syntax object at a position.
    pub fn deref_syntax<'s>(&self, source: &'s Source, span: Span) -> Option<DerefTarget<'s>> {
        let node = LinkedNode::new(source.root()).find(span)?;
        let cursor = node.offset() + 1;
        get_deref_target(node, cursor)
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

    /// Get the real definition of a compilation.
    /// Note: must be called after compliation.
    pub(crate) fn dependencies(&self) -> EcoVec<reflexo::ImmutPath> {
        let mut v = EcoVec::new();
        self.world.iter_dependencies(&mut |p| {
            v.push(p);
        });

        v
    }
    /// Get the local packages and their descriptions.
    pub fn local_packages(&self) -> EcoVec<PackageSpec> {
        crate::package::list_package_by_namespace(&self.world.registry, eco_format!("local"))
            .into_iter()
            .map(|(_, spec)| spec)
            .collect()
    }

    pub(crate) fn type_of_func(self: &Arc<Self>, func: Func) -> Signature {
        log::debug!("convert runtime func {func:?}");
        analyze_signature(self, SignatureTarget::Convert(func)).unwrap()
    }

    pub(crate) fn type_of_value(self: &Arc<Self>, val: &Value) -> Ty {
        log::debug!("convert runtime value {val:?}");

        // todo: check performance on peeking signature source frequently
        let cache_key = val;
        let cached = self
            .analysis
            .caches
            .terms
            .m
            .get(&hash128(&cache_key))
            .and_then(|slot| (cache_key == &slot.1 .0).then_some(slot.1 .1.clone()));
        if let Some(cached) = cached {
            return cached;
        }

        let res = crate::analysis::term_value(self, val);

        self.analysis
            .caches
            .terms
            .m
            .entry(hash128(&cache_key))
            .or_insert_with(|| (self.lifetime, (cache_key.clone(), res.clone())));

        res
    }

    /// Get the expression information of a source file.
    pub(crate) fn expr_stage(self: &Arc<Self>, source: &Source) -> Arc<ExprInfo> {
        let mut route = Processing::default();
        self.expr_stage_(source, &mut route)
    }

    /// Get the expression information of a source file.
    pub(crate) fn expr_stage_(
        self: &Arc<Self>,
        source: &Source,
        route: &mut Processing<Option<Arc<LazyHash<LexicalScope>>>>,
    ) -> Arc<ExprInfo> {
        use crate::syntax::expr_of;
        let guard = self.query_stat(source.id(), "expr_stage");
        self.slot.expr_stage.compute(hash128(&source), |prev| {
            expr_of(self.clone(), source.clone(), route, guard, prev)
        })
    }

    pub(crate) fn exports_of(
        self: &Arc<Self>,
        source: &Source,
        route: &mut Processing<Option<Arc<LazyHash<LexicalScope>>>>,
    ) -> Option<Arc<LazyHash<LexicalScope>>> {
        if let Some(s) = route.get(&source.id()) {
            return s.clone();
        }

        Some(self.expr_stage_(source, route).exports.clone())
    }

    /// Get the type check information of a source file.
    pub(crate) fn type_check(self: &Arc<Self>, source: &Source) -> Arc<TypeScheme> {
        let mut route = Processing::default();
        self.type_check_(source, &mut route)
    }

    /// Get the type check information of a source file.
    pub(crate) fn type_check_(
        self: &Arc<Self>,
        source: &Source,
        route: &mut Processing<Arc<TypeScheme>>,
    ) -> Arc<TypeScheme> {
        use crate::analysis::type_check;

        let ei = self.expr_stage(source);
        let guard = self.query_stat(source.id(), "type_check");
        self.slot.type_check.compute(hash128(&ei), |prev| {
            let cache_hit = prev.and_then(|prev| {
                // todo: recursively check changed scheme type
                if prev.revision != ei.revision {
                    return None;
                }

                Some(prev)
            });

            if let Some(prev) = cache_hit {
                return prev.clone();
            }

            guard.miss();
            type_check(self.clone(), ei, route)
        })
    }

    pub(crate) fn definition(
        self: &Arc<Self>,
        source: &Source,
        doc: Option<&VersionedDocument>,
        deref_target: DerefTarget,
    ) -> Option<Definition> {
        definition(self, source, doc, deref_target)
    }

    pub(crate) fn signature_def(self: &Arc<Self>, def: Definition) -> Option<Signature> {
        log::debug!("check definition func {def:?}");
        let source = def.decl.file_id().and_then(|f| self.source_by_id(f).ok());
        analyze_signature(self, SignatureTarget::Def(source, def))
    }

    pub(crate) fn signature_dyn(self: &Arc<Self>, func: Func) -> Signature {
        log::debug!("check runtime func {func:?}");
        analyze_signature(self, SignatureTarget::Runtime(func)).unwrap()
    }

    pub(crate) fn signature_docs(self: &Arc<Self>, def: &Definition) -> Option<SignatureDocs> {
        let sig = self.signature_def(def.clone())?;
        crate::docs::signature_docs(&sig, None)
    }

    /// Try to find imported target from the current source file.
    /// This function will try to resolves target statically.
    ///
    /// ## Returns
    /// The first value is the resolved source.
    /// The second value is the resolved scope.
    pub fn analyze_import(&self, source: &SyntaxNode) -> (Option<Value>, Option<Value>) {
        if let Some(v) = source.cast::<ast::Expr>().and_then(Self::const_eval) {
            return (Some(v), None);
        }
        let token = &self.analysis.workers.import;
        token.enter(|| analyze_import_(&self.world, source))
    }

    /// Try to load a module from the current source file.
    pub fn analyze_expr(&self, source: &SyntaxNode) -> EcoVec<(Value, Option<Styles>)> {
        let token = &self.analysis.workers.expression;
        token.enter(|| analyze_expr_(&self.world, source))
    }

    /// Get bib info of a source file.
    pub fn analyze_bib(
        &self,
        span: Span,
        bib_paths: impl Iterator<Item = EcoString>,
    ) -> Option<Arc<BibInfo>> {
        use comemo::Track;
        let w = &self.world;
        let w = (w as &dyn World).track();

        bib_info(w, span, bib_paths.collect())
    }

    /// Describe the item under the cursor.
    ///
    /// Passing a `document` (from a previous compilation) is optional, but
    /// enhances the autocompletions. Label completions, for instance, are
    /// only generated when the document is available.
    pub fn tooltip(
        &self,
        document: Option<&Document>,
        source: &Source,
        cursor: usize,
    ) -> Option<Tooltip> {
        let token = &self.analysis.workers.tooltip;
        token.enter(|| tooltip_(&self.world, document, source, cursor))
    }

    pub(crate) fn const_eval(rr: ast::Expr<'_>) -> Option<Value> {
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
        let traced = Traced::default();
        let mut sink = Sink::default();

        typst::eval::eval(
            ((&self.world) as &dyn World).track(),
            traced.track(),
            sink.track_mut(),
            route.track(),
            &source,
        )
    }

    /// Try to load a module from the current source file.
    pub fn module_by_syntax(&self, source: &SyntaxNode) -> Option<Value> {
        let (src, scope) = self.analyze_import(source);
        if let Some(scope) = scope {
            return Some(scope);
        }

        match src {
            Some(Value::Str(s)) => {
                let id = resolve_id_by_path(&self.world, source.span().id()?, s.as_str())?;
                self.module_by_id(id).ok().map(Value::Module)
            }
            _ => None,
        }
    }

    /// Compute the signature of a function.
    pub fn compute_signature(
        self: &Arc<Self>,
        func: SignatureTarget,
        compute: impl FnOnce(&Arc<Self>) -> Option<Signature> + Send + Sync + 'static,
    ) -> Option<Signature> {
        let res = match func {
            SignatureTarget::Def(src, d) => self
                .analysis
                .caches
                .def_signatures
                .entry(hash128(&(src, d.clone())), self.lifetime),
            SignatureTarget::SyntaxFast(source, span) => {
                let cache_key = (source, span, true);
                self.analysis
                    .caches
                    .static_signatures
                    .entry(hash128(&cache_key), self.lifetime)
            }
            SignatureTarget::Syntax(source, span) => {
                let cache_key = (source, span);
                self.analysis
                    .caches
                    .static_signatures
                    .entry(hash128(&cache_key), self.lifetime)
            }
            SignatureTarget::Convert(rt) => self
                .analysis
                .caches
                .signatures
                .entry(hash128(&(&rt, true)), self.lifetime),
            SignatureTarget::Runtime(rt) => self
                .analysis
                .caches
                .signatures
                .entry(hash128(&rt), self.lifetime),
        };
        res.get_or_init(|| compute(self)).clone()
    }

    /// Check on a module before really needing them. But we likely use them
    /// after a while.
    pub(crate) fn prefetch_type_check(self: &Arc<Self>, _fid: TypstFileId) {
        // log::debug!("prefetch type check {fid:?}");
        // let this = self.clone();
        // rayon::spawn(move || {
        //     let Some(source) = this.world.source(fid).ok() else {
        //         return;
        //     };
        //     this.type_check(&source);
        //     // log::debug!("prefetch type check end {fid:?}");
        // });
    }

    pub(crate) fn preload_package(self: Arc<Self>, entry_point: TypstFileId) {
        log::debug!("preload package start {entry_point:?}");

        #[derive(Clone)]
        struct Preloader {
            shared: Arc<SharedContext>,
            analyzed: Arc<Mutex<HashSet<TypstFileId>>>,
        }

        impl Preloader {
            fn work(&self, fid: TypstFileId) {
                log::debug!("preload package {fid:?}");
                let source = self.shared.source_by_id(fid).ok().unwrap();
                let expr = self.shared.expr_stage(&source);
                self.shared.type_check(&source);
                expr.imports.iter().for_each(|(fid, _)| {
                    if !self.analyzed.lock().insert(*fid) {
                        return;
                    }
                    self.work(*fid);
                })
            }
        }

        let preloader = Preloader {
            shared: self,
            analyzed: Arc::default(),
        };

        preloader.work(entry_point);
    }
}

#[derive(Clone)]
struct IncrCacheMap<K, V> {
    revision: usize,
    global: Arc<Mutex<FxDashMap<K, (usize, V)>>>,
    prev: Arc<Mutex<FxHashMap<K, DeferredCompute<V>>>>,
    next: Arc<Mutex<FxHashMap<K, DeferredCompute<V>>>>,
}

impl<K: Eq + Hash, V> Default for IncrCacheMap<K, V> {
    fn default() -> Self {
        Self {
            revision: 0,
            global: Arc::default(),
            prev: Arc::default(),
            next: Arc::default(),
        }
    }
}

impl<K, V> IncrCacheMap<K, V> {
    fn compute(&self, key: K, compute: impl FnOnce(Option<V>) -> V) -> V
    where
        K: Clone + Eq + Hash,
        V: Clone,
    {
        let next = self.next.lock().entry(key.clone()).or_default().clone();

        next.get_or_init(|| {
            let prev = self.prev.lock().get(&key).cloned();
            let prev = prev.and_then(|p| p.get().cloned());
            let prev = prev.or_else(|| {
                let global = self.global.lock();
                global.get(&key).map(|global| global.1.clone())
            });

            let res = compute(prev);

            let global = self.global.lock();
            let entry = global.entry(key.clone());
            use dashmap::mapref::entry::Entry;
            match entry {
                Entry::Occupied(mut e) => {
                    let (revision, _) = e.get();
                    if *revision < self.revision {
                        e.insert((self.revision, res.clone()));
                    }
                }
                Entry::Vacant(e) => {
                    e.insert((self.revision, res.clone()));
                }
            }

            res
        })
        .clone()
    }

    fn crawl(&self, revision: usize) -> Self {
        Self {
            revision,
            prev: self.next.clone(),
            global: self.global.clone(),
            next: Default::default(),
        }
    }
}

#[derive(Clone)]
struct CacheMap<T> {
    m: Arc<FxDashMap<u128, (u64, T)>>,
    // pub alloc: AllocStats,
}

impl<T> Default for CacheMap<T> {
    fn default() -> Self {
        Self {
            m: Default::default(),
            // alloc: Default::default(),
        }
    }
}

impl<T> CacheMap<T> {
    fn clear(&self) {
        self.m.clear();
    }

    fn retain(&self, mut f: impl FnMut(&mut (u64, T)) -> bool) {
        self.m.retain(|_k, v| f(v));
    }
}

impl<T: Default + Clone> CacheMap<T> {
    fn entry(&self, k: u128, lifetime: u64) -> T {
        let entry = self.m.entry(k);
        let entry = entry.or_insert_with(|| (lifetime, T::default()));
        entry.1.clone()
    }
}

// Needed by recursive computation
type DeferredCompute<T> = Arc<OnceCell<T>>;

/// A global (compiler server spanned) cache for all level of analysis results
/// of a module.
#[derive(Default, Clone)]
pub struct AnalysisGlobalCaches {
    lifetime: Arc<AtomicU64>,
    clear_lifetime: Arc<AtomicU64>,
    def_signatures: CacheMap<DeferredCompute<Option<Signature>>>,
    static_signatures: CacheMap<DeferredCompute<Option<Signature>>>,
    signatures: CacheMap<DeferredCompute<Option<Signature>>>,
    terms: CacheMap<(Value, Ty)>,
}

/// A cache for all level of analysis results of a module.
#[derive(Default)]
pub struct AnalysisCaches {
    modules: HashMap<TypstFileId, ModuleAnalysisCache>,
    completion_files: OnceCell<Vec<TypstFileId>>,
    root_files: OnceCell<Vec<TypstFileId>>,
    module_deps: OnceCell<HashMap<TypstFileId, ModuleDependency>>,
}

/// A cache for module-level analysis results of a module.
///
/// You should not holds across requests, because source code may change.
#[derive(Default)]
pub struct ModuleAnalysisCache {
    expr_stage: OnceCell<Arc<ExprInfo>>,
    type_check: OnceCell<Arc<TypeScheme>>,
}

/// The grid cache for all level of analysis results of a module.
#[derive(Default)]
pub struct AnalysisGlobalCacheGrid {
    revision: usize,
    default_slot: RevisionSlot,
    revisions: Vec<Arc<RevisionSlot>>,
    locked_revisions: HashMap<usize, usize>,
}

impl AnalysisGlobalCacheGrid {
    fn clear(&mut self) {
        self.revisions.clear();
    }

    fn gc(&mut self, rev: usize) {
        self.revisions.retain(|r| r.revision >= rev);
        self.default_slot
            .expr_stage
            .global
            .lock()
            .retain(|_, r| r.0 + 60 >= rev);
        self.default_slot
            .type_check
            .global
            .lock()
            .retain(|_, r| r.0 + 60 >= rev);
    }

    /// Find the last revision slot by revision number.
    fn find_revision(&mut self, revision: NonZeroUsize) -> Arc<RevisionSlot> {
        let slot_base = self
            .revisions
            .iter()
            .filter(|e| e.revision <= revision.get())
            .reduce(|a, b| if a.revision > b.revision { a } else { b });

        if let Some(slot) = slot_base {
            if slot.revision == revision.get() {
                return slot.clone();
            }
        }

        let mut slot = slot_base
            .map(|e| RevisionSlot {
                revision: e.revision,
                expr_stage: e.expr_stage.crawl(revision.get()),
                type_check: e.type_check.crawl(revision.get()),
            })
            .unwrap_or_else(|| self.default_slot.clone());

        slot.revision = revision.get();
        let slot = Arc::new(slot);
        self.revisions.push(slot.clone());
        self.revision = revision.get().max(self.revision);
        slot
    }
}

/// A lock for revision.
pub struct RevisionLock {
    grid: Arc<Mutex<AnalysisGlobalCacheGrid>>,
    revision: usize,
}

impl Drop for RevisionLock {
    fn drop(&mut self) {
        let mut grid = self.grid.lock();
        let revision_cnt = grid
            .locked_revisions
            .entry(self.revision)
            .or_insert_with(|| panic!("revision {} is not locked", self.revision));
        *revision_cnt -= 1;
        if *revision_cnt != 0 {
            return;
        }

        grid.locked_revisions.remove(&self.revision);
        if grid.revision <= self.revision {
            return;
        }
        let existing = grid.locked_revisions.keys().min().copied();
        let gc_revision = existing.unwrap_or(self.revision);
        let grid = self.grid.clone();

        rayon::spawn(move || {
            grid.lock().gc(gc_revision);
        });
    }
}

#[derive(Default, Clone)]
struct RevisionSlot {
    revision: usize,
    expr_stage: IncrCacheMap<u128, Arc<ExprInfo>>,
    type_check: IncrCacheMap<u128, Arc<TypeScheme>>,
}

impl Drop for RevisionSlot {
    fn drop(&mut self) {
        log::info!("revision {} is dropped", self.revision)
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
pub struct SearchCtx<'a, 'w> {
    /// The inner analysis context.
    pub ctx: &'a mut AnalysisContext<'w>,
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
