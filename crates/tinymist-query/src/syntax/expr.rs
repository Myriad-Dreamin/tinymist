use std::ops::DerefMut;

use parking_lot::Mutex;
use rpds::RedBlackTreeMapSync;
use rustc_hash::FxHashMap;
use std::ops::Deref;
use tinymist_analysis::adt::interner::Interned;
use tinymist_std::hash::hash128;
use typst::{
    foundations::{Element, NativeElement, Type, Value},
    model::{EmphElem, EnumElem, HeadingElem, ListElem, ParbreakElem, StrongElem, TermsElem},
    syntax::{Span, SyntaxNode, ast::MathTextKind},
    text::LinebreakElem,
    utils::LazyHash,
};

use crate::{
    analysis::{QueryStatGuard, SharedContext},
    docs::DocString,
    prelude::*,
    syntax::{DefKind, find_module_level_docs, resolve_id_by_path},
    ty::{BuiltinTy, InsTy, Ty},
};

use super::{DocCommentMatcher, InterpretMode, def::*};

/// Maps file identifiers to their lexical scopes for expression analysis
/// routing.
pub type ExprRoute = FxHashMap<TypstFileId, Option<Arc<LazyHash<LexicalScope>>>>;

/// Analyzes expressions in a source file and produces expression information.
///
/// This is the core function for expression analysis, which powers features
/// like go-to-definition, hover, and completion. It performs a two-pass
/// analysis:
///
/// 1. **First pass (init_stage)**: Builds the root lexical scope by scanning
///    top-level definitions without resolving them. This handles forward
///    references and circular dependencies.
///
/// 2. **Second pass**: Performs full expression analysis, resolving
///    identifiers, tracking imports, and building the expression tree with type
///    information.
#[typst_macros::time(span = source.root().span())]
pub(crate) fn expr_of(
    ctx: Arc<SharedContext>,
    source: Source,
    route: &mut ExprRoute,
    guard: QueryStatGuard,
    prev: Option<ExprInfo>,
) -> ExprInfo {
    crate::log_debug_ct!("expr_of: {:?}", source.id());

    route.insert(source.id(), None);

    let cache_hit = prev.and_then(|prev| {
        if prev.source.lines().len_bytes() != source.lines().len_bytes()
            || hash128(&prev.source) != hash128(&source)
        {
            return None;
        }
        for (fid, prev_exports) in &prev.imports {
            let ei = ctx.exports_of(&ctx.source_by_id(*fid).ok()?, route);

            // If there is a cycle, the expression will be stable as the source is
            // unchanged.
            if let Some(exports) = ei
                && (prev_exports.size() != exports.size()
                    || hash128(&prev_exports) != hash128(&exports))
            {
                return None;
            }
        }

        Some(prev)
    });

    if let Some(prev) = cache_hit {
        route.remove(&source.id());
        return prev;
    }
    guard.miss();

    let revision = ctx.revision();

    let resolves_base = Arc::new(Mutex::new(vec![]));
    let resolves = resolves_base.clone();

    // todo: cache docs capture
    let docstrings_base = Arc::new(Mutex::new(FxHashMap::default()));
    let docstrings = docstrings_base.clone();

    let exprs_base = Arc::new(Mutex::new(FxHashMap::default()));
    let exprs = exprs_base.clone();

    let imports_base = Arc::new(Mutex::new(FxHashMap::default()));
    let imports = imports_base.clone();

    let module_docstring = find_module_level_docs(&source)
        .and_then(|docs| ctx.compute_docstring(source.id(), docs, DefKind::Module))
        .unwrap_or_default();

    let mut worker = ExprWorker {
        fid: source.id(),
        source: source.clone(),
        ctx,
        imports,
        docstrings,
        exprs,
        import_buffer: Vec::new(),
        lexical: LexicalContext::default(),
        resolves,
        buffer: vec![],
        module_items: FxHashMap::default(),
        init_stage: true,
        comment_matcher: DocCommentMatcher::default(),
        route,
    };

    let root_markup = source.root().cast::<ast::Markup>().unwrap();
    worker.check_root_scope(root_markup.to_untyped().children());
    let first_scope = Arc::new(LazyHash::new(worker.summarize_scope()));
    worker.route.insert(worker.fid, Some(first_scope.clone()));

    worker.lexical = LexicalContext::default();
    worker.comment_matcher.reset();
    worker.buffer.clear();
    worker.import_buffer.clear();
    worker.module_items.clear();
    let root = worker.check_in_mode(root_markup.to_untyped().children(), InterpretMode::Markup);
    let exports = Arc::new(LazyHash::new(worker.summarize_scope()));

    worker.collect_buffer();
    let module_items = std::mem::take(&mut worker.module_items);

    let info = ExprInfoRepr {
        fid: source.id(),
        revision,
        source: source.clone(),
        resolves: HashMap::from_iter(std::mem::take(resolves_base.lock().deref_mut())),
        module_docstring,
        docstrings: std::mem::take(docstrings_base.lock().deref_mut()),
        imports: HashMap::from_iter(std::mem::take(imports_base.lock().deref_mut())),
        exports,
        exprs: std::mem::take(exprs_base.lock().deref_mut()),
        root,
        module_items,
    };
    crate::log_debug_ct!("expr_of end {:?}", source.id());

    route.remove(&info.fid);
    ExprInfo::new(info)
}

type ConcolicExpr = (Option<Expr>, Option<Ty>);
type ResolveVec = Vec<(Span, Interned<RefExpr>)>;
type SyntaxNodeChildren<'a> = std::slice::Iter<'a, SyntaxNode>;

#[derive(Debug, Clone)]
struct LexicalContext {
    mode: InterpretMode,
    scopes: EcoVec<ExprScope>,
    last: ExprScope,
}

impl Default for LexicalContext {
    fn default() -> Self {
        LexicalContext {
            mode: InterpretMode::Markup,
            scopes: eco_vec![],
            last: ExprScope::Lexical(RedBlackTreeMapSync::default()),
        }
    }
}

/// Worker for processing expressions during source file analysis.
pub(crate) struct ExprWorker<'a> {
    fid: TypstFileId,
    source: Source,
    ctx: Arc<SharedContext>,
    imports: Arc<Mutex<FxHashMap<TypstFileId, Arc<LazyHash<LexicalScope>>>>>,
    import_buffer: Vec<(TypstFileId, Arc<LazyHash<LexicalScope>>)>,
    docstrings: Arc<Mutex<FxHashMap<DeclExpr, Arc<DocString>>>>,
    exprs: Arc<Mutex<FxHashMap<Span, Expr>>>,
    resolves: Arc<Mutex<ResolveVec>>,
    buffer: ResolveVec,
    lexical: LexicalContext,
    module_items: FxHashMap<DeclExpr, ModuleItemLayout>,
    init_stage: bool,

    route: &'a mut ExprRoute,
    comment_matcher: DocCommentMatcher,
}

impl ExprWorker<'_> {
    fn with_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.lexical.scopes.push(std::mem::replace(
            &mut self.lexical.last,
            ExprScope::empty(),
        ));
        let len = self.lexical.scopes.len();
        let result = f(self);
        self.lexical.scopes.truncate(len);
        self.lexical.last = self.lexical.scopes.pop().unwrap();
        result
    }

    fn push_scope(&mut self, scope: ExprScope) {
        let last = std::mem::replace(&mut self.lexical.last, scope);
        if !last.is_empty() {
            self.lexical.scopes.push(last);
        }
    }

    #[must_use]
    fn scope_mut(&mut self) -> &mut LexicalScope {
        if matches!(self.lexical.last, ExprScope::Lexical(_)) {
            return self.lexical_scope_unchecked();
        }
        self.lexical.scopes.push(std::mem::replace(
            &mut self.lexical.last,
            ExprScope::empty(),
        ));
        self.lexical_scope_unchecked()
    }

    fn lexical_scope_unchecked(&mut self) -> &mut LexicalScope {
        let scope = &mut self.lexical.last;
        if let ExprScope::Lexical(scope) = scope {
            scope
        } else {
            unreachable!()
        }
    }

    fn check_docstring(&mut self, decl: &DeclExpr, docs: Option<String>, kind: DefKind) {
        if let Some(docs) = docs {
            let docstring = self.ctx.compute_docstring(self.fid, docs, kind);
            if let Some(docstring) = docstring {
                self.docstrings.lock().insert(decl.clone(), docstring);
            }
        }
    }

    fn summarize_scope(&self) -> LexicalScope {
        let mut exports = LexicalScope::default();
        for scope in self
            .lexical
            .scopes
            .iter()
            .chain(std::iter::once(&self.lexical.last))
        {
            scope.merge_into(&mut exports);
        }
        exports
    }

    fn check(&mut self, m: ast::Expr) -> Expr {
        let s = m.span();
        let ret = self.do_check(m);
        self.exprs.lock().insert(s, ret.clone());
        ret
    }

    fn do_check(&mut self, m: ast::Expr) -> Expr {
        use ast::Expr::*;
        match m {
            None(_) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            Auto(..) => Expr::Type(Ty::Builtin(BuiltinTy::Auto)),
            Bool(bool) => Expr::Type(Ty::Value(InsTy::new(Value::Bool(bool.get())))),
            Int(int) => Expr::Type(Ty::Value(InsTy::new(Value::Int(int.get())))),
            Float(float) => Expr::Type(Ty::Value(InsTy::new(Value::Float(float.get())))),
            Numeric(numeric) => Expr::Type(Ty::Value(InsTy::new(Value::numeric(numeric.get())))),
            Str(s) => Expr::Type(Ty::Value(InsTy::new(Value::Str(s.get().into())))),

            Equation(equation) => self.check_math(equation.body().to_untyped().children()),
            Math(math) => self.check_math(math.to_untyped().children()),
            CodeBlock(code_block) => self.check_code(code_block.body()),
            ContentBlock(content_block) => self.check_markup(content_block.body()),

            Ident(ident) => self.check_ident(ident),
            MathIdent(math_ident) => self.check_math_ident(math_ident),
            Label(label) => self.check_label(label),
            Ref(ref_node) => self.check_ref(ref_node),

            LetBinding(let_binding) => self.check_let(let_binding),
            Closure(closure) => self.check_closure(closure),
            ModuleImport(module_import) => self.check_module_import(module_import),
            ModuleInclude(module_include) => self.check_module_include(module_include),

            Parenthesized(paren_expr) => self.check(paren_expr.expr()),
            Array(array) => self.check_array(array),
            Dict(dict) => self.check_dict(dict),
            Unary(unary) => self.check_unary(unary),
            Binary(binary) => self.check_binary(binary),
            FieldAccess(field_access) => self.check_field_access(field_access),
            FuncCall(func_call) => self.check_func_call(func_call),
            DestructAssignment(destruct_assignment) => {
                self.check_destruct_assign(destruct_assignment)
            }
            SetRule(set_rule) => self.check_set(set_rule),
            ShowRule(show_rule) => self.check_show(show_rule),
            Contextual(contextual) => {
                Expr::Unary(UnInst::new(UnaryOp::Context, self.defer(contextual.body())))
            }
            Conditional(conditional) => self.check_conditional(conditional),
            WhileLoop(while_loop) => self.check_while_loop(while_loop),
            ForLoop(for_loop) => self.check_for_loop(for_loop),
            LoopBreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Break)),
            LoopContinue(..) => Expr::Type(Ty::Builtin(BuiltinTy::Continue)),
            FuncReturn(func_return) => Expr::Unary(UnInst::new(
                UnaryOp::Return,
                func_return
                    .body()
                    .map_or_else(none_expr, |body| self.check(body)),
            )),

            Text(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                typst::text::TextElem,
            >())))),
            MathText(t) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some({
                match t.get() {
                    MathTextKind::Character(..) => Element::of::<typst::foundations::SymbolElem>(),
                    MathTextKind::Number(..) => Element::of::<typst::foundations::SymbolElem>(),
                }
            })))),
            Raw(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                typst::text::RawElem,
            >())))),
            Link(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                typst::model::LinkElem,
            >())))),
            Space(..) => Expr::Type(Ty::Builtin(BuiltinTy::Space)),
            Linebreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                LinebreakElem,
            >())))),
            Parbreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                ParbreakElem,
            >())))),
            Escape(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                typst::text::TextElem,
            >())))),
            Shorthand(..) => Expr::Type(Ty::Builtin(BuiltinTy::Type(Type::of::<
                typst::foundations::Symbol,
            >()))),
            SmartQuote(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                typst::text::SmartQuoteElem,
            >())))),

            Strong(strong) => {
                let body = self.check_inline_markup(strong.body());
                self.check_element::<StrongElem>(eco_vec![body])
            }
            Emph(emph) => {
                let body = self.check_inline_markup(emph.body());
                self.check_element::<EmphElem>(eco_vec![body])
            }
            Heading(heading) => {
                let body = self.check_markup(heading.body());
                self.check_element::<HeadingElem>(eco_vec![body])
            }
            ListItem(item) => {
                let body = self.check_markup(item.body());
                self.check_element::<ListElem>(eco_vec![body])
            }
            EnumItem(item) => {
                let body = self.check_markup(item.body());
                self.check_element::<EnumElem>(eco_vec![body])
            }
            TermItem(item) => {
                let term = self.check_markup(item.term());
                let description = self.check_markup(item.description());
                self.check_element::<TermsElem>(eco_vec![term, description])
            }

            MathAlignPoint(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content(Some(Element::of::<
                typst::math::AlignPointElem,
            >(
            ))))),
            MathShorthand(..) => Expr::Type(Ty::Builtin(BuiltinTy::Type(Type::of::<
                typst::foundations::Symbol,
            >()))),
            MathDelimited(math_delimited) => {
                self.check_math(math_delimited.body().to_untyped().children())
            }
            MathAttach(attach) => {
                let base = attach.base().to_untyped().clone();
                let bottom = attach.bottom().unwrap_or_default().to_untyped().clone();
                let top = attach.top().unwrap_or_default().to_untyped().clone();
                self.check_math([base, bottom, top].iter())
            }
            MathPrimes(..) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            MathFrac(frac) => {
                let num = frac.num().to_untyped().clone();
                let denom = frac.denom().to_untyped().clone();
                self.check_math([num, denom].iter())
            }
            MathRoot(root) => self.check(root.radicand()),
        }
    }

    fn check_element<T: NativeElement>(&mut self, content: EcoVec<Expr>) -> Expr {
        let elem = Element::of::<T>();
        Expr::Element(ElementExpr { elem, content }.into())
    }

    fn check_let(&mut self, typed: ast::LetBinding) -> Expr {
        match typed.kind() {
            ast::LetBindingKind::Closure(..) => {
                typed.init().map_or_else(none_expr, |expr| self.check(expr))
            }
            ast::LetBindingKind::Normal(pat) => {
                let docs = self.comment_matcher.collect();
                // Check init expression before pattern checking
                let body = typed.init().map(|init| self.defer(init));

                let span = pat.span();
                let decl = Decl::pattern(span).into();
                self.check_docstring(&decl, docs, DefKind::Variable);
                let pattern = self.check_pattern(pat);
                Expr::Let(Interned::new(LetExpr {
                    span,
                    pattern,
                    body,
                }))
            }
        }
    }

    fn check_closure(&mut self, typed: ast::Closure) -> Expr {
        let docs = self.comment_matcher.collect();
        let decl = match typed.name() {
            Some(name) => Decl::func(name).into(),
            None => Decl::closure(typed.span()).into(),
        };
        self.check_docstring(&decl, docs, DefKind::Function);
        self.resolve_as(Decl::as_def(&decl, None));

        let (params, body) = self.with_scope(|this| {
            this.scope_mut()
                .insert_mut(decl.name().clone(), decl.clone().into());
            let mut inputs = eco_vec![];
            let mut names = eco_vec![];
            let mut spread_left = None;
            let mut spread_right = None;
            for arg in typed.params().children() {
                match arg {
                    ast::Param::Pos(arg) => {
                        inputs.push(this.check_pattern(arg));
                    }
                    ast::Param::Named(arg) => {
                        let key: DeclExpr = Decl::var(arg.name()).into();
                        let val = Pattern::Expr(this.check(arg.expr())).into();
                        names.push((key.clone(), val));

                        this.resolve_as(Decl::as_def(&key, None));
                        this.scope_mut().insert_mut(key.name().clone(), key.into());
                    }
                    ast::Param::Spread(s) => {
                        let decl: DeclExpr = if let Some(ident) = s.sink_ident() {
                            Decl::var(ident).into()
                        } else {
                            Decl::spread(s.span()).into()
                        };

                        let spread = Pattern::Expr(this.check(s.expr())).into();
                        if inputs.is_empty() {
                            spread_left = Some((decl.clone(), spread));
                        } else {
                            spread_right = Some((decl.clone(), spread));
                        }

                        this.resolve_as(Decl::as_def(&decl, None));
                        this.scope_mut()
                            .insert_mut(decl.name().clone(), decl.into());
                    }
                }
            }

            if inputs.is_empty() {
                spread_right = spread_left.take();
            }

            let pattern = PatternSig {
                pos: inputs,
                named: names,
                spread_left,
                spread_right,
            };

            (pattern, this.defer(typed.body()))
        });

        self.scope_mut()
            .insert_mut(decl.name().clone(), decl.clone().into());
        Expr::Func(FuncExpr { decl, params, body }.into())
    }

    fn check_pattern(&mut self, typed: ast::Pattern) -> Interned<Pattern> {
        match typed {
            ast::Pattern::Normal(expr) => self.check_pattern_expr(expr),
            ast::Pattern::Placeholder(..) => Pattern::Expr(Expr::Star).into(),
            ast::Pattern::Parenthesized(paren_expr) => self.check_pattern(paren_expr.pattern()),
            ast::Pattern::Destructuring(destructing) => {
                let mut inputs = eco_vec![];
                let mut names = eco_vec![];
                let mut spread_left = None;
                let mut spread_right = None;

                for item in destructing.items() {
                    match item {
                        ast::DestructuringItem::Pattern(pos) => {
                            inputs.push(self.check_pattern(pos));
                        }
                        ast::DestructuringItem::Named(named) => {
                            let key = Decl::var(named.name()).into();
                            let val = self.check_pattern(named.pattern());
                            names.push((key, val));
                        }
                        ast::DestructuringItem::Spread(spreading) => {
                            let decl: DeclExpr = if let Some(ident) = spreading.sink_ident() {
                                Decl::var(ident).into()
                            } else {
                                Decl::spread(spreading.span()).into()
                            };
                            let pattern = Pattern::Expr(Expr::Star).into();

                            if inputs.is_empty() {
                                spread_left = Some((decl.clone(), pattern));
                            } else {
                                spread_right = Some((decl.clone(), pattern));
                            }

                            self.resolve_as(Decl::as_def(&decl, None));
                            self.scope_mut()
                                .insert_mut(decl.name().clone(), decl.into());
                        }
                    }
                }

                if inputs.is_empty() {
                    spread_right = spread_left.take();
                }

                let pattern = PatternSig {
                    pos: inputs,
                    named: names,
                    spread_left,
                    spread_right,
                };

                Pattern::Sig(Box::new(pattern)).into()
            }
        }
    }

    fn check_pattern_expr(&mut self, typed: ast::Expr) -> Interned<Pattern> {
        match typed {
            ast::Expr::Ident(ident) => {
                let decl = Decl::var(ident).into();
                self.resolve_as(Decl::as_def(&decl, None));
                self.scope_mut()
                    .insert_mut(decl.name().clone(), decl.clone().into());
                Pattern::Simple(decl).into()
            }
            ast::Expr::Parenthesized(parenthesized) => self.check_pattern(parenthesized.pattern()),
            _ => Pattern::Expr(self.check(typed)).into(),
        }
    }

    fn check_module_import(&mut self, typed: ast::ModuleImport) -> Expr {
        let is_wildcard_import = matches!(typed.imports(), Some(ast::Imports::Wildcard));

        let source = typed.source();
        let mod_expr = self.check_import(typed.source(), true, is_wildcard_import);
        crate::log_debug_ct!("checking import: {source:?} => {mod_expr:?}");

        let mod_var = typed.new_name().map(Decl::module_alias).or_else(|| {
            typed.imports().is_none().then(|| {
                let name = match mod_expr.as_ref()? {
                    Expr::Decl(decl) if matches!(decl.as_ref(), Decl::Module { .. }) => {
                        decl.name().clone()
                    }
                    _ => return None,
                };
                // todo: package stem
                Some(Decl::path_stem(source.to_untyped().clone(), name))
            })?
        });

        let creating_mod_var = mod_var.is_some();
        let mod_var = Interned::new(mod_var.unwrap_or_else(|| Decl::module_import(typed.span())));

        // Create a RefExpr for the module import variable.
        // - decl: The import variable (e.g., "foo" in "import 'file.typ' as foo")
        // - step & root: Both point to the module expression (same for imports)
        // - term: None because module types are complex and not stored here
        let mod_ref = RefExpr {
            decl: mod_var.clone(),
            step: mod_expr.clone(),
            root: mod_expr.clone(),
            term: None,
        };
        crate::log_debug_ct!("create import variable: {mod_ref:?}");
        let mod_ref = Interned::new(mod_ref);
        if creating_mod_var {
            self.scope_mut()
                .insert_mut(mod_var.name().clone(), Expr::Ref(mod_ref.clone()));
        }

        self.resolve_as(mod_ref.clone());

        let fid = mod_expr.as_ref().and_then(|mod_expr| match mod_expr {
            Expr::Type(Ty::Value(v)) => match &v.val {
                Value::Module(m) => m.file_id(),
                _ => None,
            },
            Expr::Decl(decl) => {
                if matches!(decl.as_ref(), Decl::Module { .. }) {
                    decl.file_id()
                } else {
                    None
                }
            }
            _ => None,
        });

        // Prefetch Type Check Information
        if let Some(fid) = fid {
            crate::log_debug_ct!("prefetch type check: {fid:?}");
            self.ctx.prefetch_type_check(fid);
        }

        let scope = if let Some(fid) = &fid {
            Some(ExprScope::Lexical(self.exports_of(*fid)))
        } else {
            match &mod_expr {
                Some(Expr::Type(Ty::Value(v))) => match &v.val {
                    Value::Module(m) => Some(ExprScope::Module(m.clone())),
                    Value::Func(func) => {
                        if func.scope().is_some() {
                            Some(ExprScope::Func(func.clone()))
                        } else {
                            None
                        }
                    }
                    Value::Type(s) => Some(ExprScope::Type(*s)),
                    _ => None,
                },
                _ => None,
            }
        };

        let scope = if let Some(scope) = scope {
            scope
        } else {
            log::warn!(
                "cannot analyze import on: {typed:?}, expr {mod_expr:?}, in file {:?}",
                typed.span().id()
            );
            ExprScope::empty()
        };

        if let Some(imports) = typed.imports() {
            match imports {
                ast::Imports::Wildcard => {
                    crate::log_debug_ct!("checking wildcard: {mod_expr:?}");
                    self.push_scope(scope);
                }
                ast::Imports::Items(items) => {
                    let module = Expr::Decl(mod_var.clone());
                    self.import_decls(&scope, Some(mod_var.clone()), module, items);
                }
            }
        };

        Expr::Import(
            ImportExpr {
                source: self.check(source),
                decl: mod_ref,
            }
            .into(),
        )
    }

    fn check_import(
        &mut self,
        source: ast::Expr,
        is_import: bool,
        is_wildcard_import: bool,
    ) -> Option<Expr> {
        let src = self.eval_expr(source, InterpretMode::Code);
        let src_expr = self.fold_expr_and_val(src).or_else(|| {
            self.ctx
                .analyze_expr(source.to_untyped())
                .into_iter()
                .find_map(|(v, _)| match v {
                    Value::Str(s) => Some(Expr::Type(Ty::Value(InsTy::new(Value::Str(s))))),
                    _ => None,
                })
        })?;

        crate::log_debug_ct!("checking import source: {src_expr:?}");
        let const_res = match &src_expr {
            Expr::Type(Ty::Value(val)) => {
                self.check_import_source_val(source, &val.val, Some(&src_expr), is_import)
            }
            Expr::Decl(decl) if matches!(decl.as_ref(), Decl::Module { .. }) => {
                return Some(src_expr.clone());
            }

            _ => None,
        };
        const_res
            .or_else(|| self.check_import_by_def(&src_expr))
            .or_else(|| is_wildcard_import.then(|| self.check_import_dyn(source, &src_expr))?)
    }

    fn check_import_dyn(&mut self, source: ast::Expr, src_expr: &Expr) -> Option<Expr> {
        let src_or_module = self.ctx.analyze_import(source.to_untyped());
        crate::log_debug_ct!("checking import source dyn: {src_or_module:?}");

        match src_or_module {
            (_, Some(Value::Module(m))) => {
                // todo: dyn resolve src_expr
                match m.file_id() {
                    Some(fid) => Some(Expr::Decl(
                        Decl::module_with_name(m.name().unwrap().into(), fid).into(),
                    )),
                    None => Some(Expr::Type(Ty::Value(InsTy::new(Value::Module(m))))),
                }
            }
            (_, Some(v)) => Some(Expr::Type(Ty::Value(InsTy::new(v)))),
            (Some(s), _) => self.check_import_source_val(source, &s, Some(src_expr), true),
            (None, None) => None,
        }
    }

    fn check_import_source_val(
        &mut self,
        source: ast::Expr,
        src: &Value,
        src_expr: Option<&Expr>,
        is_import: bool,
    ) -> Option<Expr> {
        match &src {
            _ if src.scope().is_some() => src_expr
                .cloned()
                .or_else(|| Some(Expr::Type(Ty::Value(InsTy::new(src.clone()))))),
            Value::Str(s) => self.check_import_by_str(source, s.as_str(), is_import),
            _ => None,
        }
    }

    fn check_import_by_str(
        &mut self,
        source: ast::Expr,
        src: &str,
        is_import: bool,
    ) -> Option<Expr> {
        let fid = resolve_id_by_path(&self.ctx.world(), self.fid, src)?;
        let name = Decl::calc_path_stem(src);
        let module = Expr::Decl(Decl::module_with_name(name.clone(), fid).into());

        let import_path = if is_import {
            Decl::import_path(source.span(), name)
        } else {
            Decl::include_path(source.span(), name)
        };

        // Create a RefExpr for the import/include path.
        // - decl: The path declaration (tracks the file path being imported)
        // - step & root: Both point to the loaded module
        // - term: None (module types not stored directly)
        let ref_expr = RefExpr {
            decl: import_path.into(),
            step: Some(module.clone()),
            root: Some(module.clone()),
            term: None,
        };
        self.resolve_as(ref_expr.into());
        Some(module)
    }

    fn check_import_by_def(&mut self, src_expr: &Expr) -> Option<Expr> {
        match src_expr {
            Expr::Decl(m) if matches!(m.kind(), DefKind::Module) => Some(src_expr.clone()),
            Expr::Ref(r) => r.root.clone(),
            _ => None,
        }
    }

    fn import_decls(
        &mut self,
        scope: &ExprScope,
        module_decl: Option<DeclExpr>,
        module: Expr,
        items: ast::ImportItems,
    ) {
        crate::log_debug_ct!("import scope {scope:?}");

        for item in items.iter() {
            let (path_ast, old, rename) = match item {
                ast::ImportItem::Simple(path) => {
                    let old: DeclExpr = Decl::import(path.name()).into();
                    (path, old, None)
                }
                ast::ImportItem::Renamed(renamed) => {
                    let path = renamed.path();
                    let old: DeclExpr = Decl::import(path.name()).into();
                    let new: DeclExpr = Decl::import_alias(renamed.new_name()).into();
                    (path, old, Some(new))
                }
            };

            let item_span = match item {
                ast::ImportItem::Simple(path) => path.span(),
                ast::ImportItem::Renamed(renamed) => renamed.span(),
            };

            if let Some(parent) = module_decl.as_ref() {
                self.record_module_item(parent, &old, item_span);
                if let Some(rename_decl) = &rename {
                    self.record_module_item(parent, rename_decl, item_span);
                }
            }

            let mut path = Vec::with_capacity(1);
            for seg in path_ast.iter() {
                let seg = Interned::new(Decl::ident_ref(seg));
                path.push(seg);
            }
            // todo: import path
            let (mut root, val) = match path.last().map(|decl| decl.name()) {
                Some(name) => scope.get(name),
                None => (None, None),
            };

            crate::log_debug_ct!("path {path:?} -> {root:?} {val:?}");
            if root.is_none() && val.is_none() {
                let mut sel = module.clone();
                for seg in path.into_iter() {
                    sel = Expr::Select(SelectExpr::new(seg, sel));
                }
                root = Some(sel)
            }

            let (root, step) = extract_ref(root);

            // Create RefExpr for the original name in the import.
            // - decl: The original identifier (e.g., "old" in "import: old as new")
            // - root: The module or selection expression where the value comes from
            // - step: Intermediate expression (from extract_ref, handles reference chains)
            // - term: The type if it was found in the scope
            let mut ref_expr = Interned::new(RefExpr {
                decl: old.clone(),
                root,
                step,
                term: val,
            });
            self.resolve_as(ref_expr.clone());

            // If renamed, create a second RefExpr for the new name that chains to the old
            // one. This builds the chain: new -> old -> root
            if let Some(new) = &rename {
                // - decl: The new name (e.g., "new" in "import: old as new")
                // - root: Same as original (ultimate source of the value)
                // - step: Points to the old name (intermediate link in the chain)
                // - term: Same type as original
                ref_expr = Interned::new(RefExpr {
                    decl: new.clone(),
                    root: ref_expr.root.clone(),
                    step: Some(ref_expr.decl.clone().into()),
                    term: ref_expr.term.clone(),
                });
                self.resolve_as(ref_expr.clone());
            }

            // final resolves
            let name = rename.as_ref().unwrap_or(&old).name().clone();
            let expr = Expr::Ref(ref_expr);
            self.scope_mut().insert_mut(name, expr.clone());
        }
    }

    fn record_module_item(&mut self, parent: &DeclExpr, child: &DeclExpr, span: Span) {
        if self.init_stage || span.is_detached() || span.id() != Some(self.fid) {
            return;
        }
        let Some(item_range) = self.source.range(span) else {
            return;
        };
        let Some(binding_range) = self.source.range(child.span()) else {
            return;
        };
        self.module_items.insert(
            child.clone(),
            ModuleItemLayout {
                parent: parent.clone(),
                item_range,
                binding_range,
            },
        );
    }

    fn check_module_include(&mut self, typed: ast::ModuleInclude) -> Expr {
        let _mod_expr = self.check_import(typed.source(), false, false);
        let source = self.check(typed.source());
        Expr::Include(IncludeExpr { source }.into())
    }

    fn check_array(&mut self, typed: ast::Array) -> Expr {
        let mut items = vec![];
        for item in typed.items() {
            match item {
                ast::ArrayItem::Pos(item) => {
                    items.push(ArgExpr::Pos(self.check(item)));
                }
                ast::ArrayItem::Spread(s) => {
                    items.push(ArgExpr::Spread(self.check(s.expr())));
                }
            }
        }

        Expr::Array(ArgsExpr::new(typed.span(), items))
    }

    fn check_dict(&mut self, typed: ast::Dict) -> Expr {
        let mut items = vec![];
        for item in typed.items() {
            match item {
                ast::DictItem::Named(item) => {
                    let key = Decl::ident_ref(item.name()).into();
                    let val = self.check(item.expr());
                    items.push(ArgExpr::Named(Box::new((key, val))));
                }
                ast::DictItem::Keyed(item) => {
                    let val = self.check(item.expr());
                    let key = item.key();
                    let analyzed = self
                        .const_eval_expr(key)
                        .and_then(|v| match v {
                            Value::Str(s) => Some(s),
                            _ => None,
                        })
                        .or_else(|| {
                            let (expr, term) = self.eval_expr(key, InterpretMode::Code);

                            if let Some(Ty::Value(v)) = term
                                && let Value::Str(s) = &v.val
                            {
                                return Some(s.clone());
                            }

                            if let Some(Expr::Type(Ty::Value(v))) = expr
                                && let Value::Str(s) = &v.val
                            {
                                return Some(s.clone());
                            }

                            None
                        });
                    let Some(analyzed) = analyzed else {
                        let key = self.check(key);
                        items.push(ArgExpr::NamedRt(Box::new((key, val))));
                        continue;
                    };
                    let key = Decl::str_name(key.to_untyped().clone(), analyzed.as_str()).into();
                    items.push(ArgExpr::Named(Box::new((key, val))));
                }
                ast::DictItem::Spread(s) => {
                    items.push(ArgExpr::Spread(self.check(s.expr())));
                }
            }
        }

        Expr::Dict(ArgsExpr::new(typed.span(), items))
    }

    fn check_args(&mut self, typed: ast::Args) -> Expr {
        let mut args = vec![];
        for arg in typed.items() {
            match arg {
                ast::Arg::Pos(arg) => {
                    args.push(ArgExpr::Pos(self.check(arg)));
                }
                ast::Arg::Named(arg) => {
                    let key = Decl::ident_ref(arg.name()).into();
                    let val = self.check(arg.expr());
                    args.push(ArgExpr::Named(Box::new((key, val))));
                }
                ast::Arg::Spread(s) => {
                    args.push(ArgExpr::Spread(self.check(s.expr())));
                }
            }
        }
        Expr::Args(ArgsExpr::new(typed.span(), args))
    }

    fn check_unary(&mut self, typed: ast::Unary) -> Expr {
        let op = match typed.op() {
            ast::UnOp::Pos => UnaryOp::Pos,
            ast::UnOp::Neg => UnaryOp::Neg,
            ast::UnOp::Not => UnaryOp::Not,
        };
        let lhs = self.check(typed.expr());
        Expr::Unary(UnInst::new(op, lhs))
    }

    fn check_binary(&mut self, typed: ast::Binary) -> Expr {
        let lhs = self.check(typed.lhs());
        let rhs = self.check(typed.rhs());
        Expr::Binary(BinInst::new(typed.op(), lhs, rhs))
    }

    fn check_destruct_assign(&mut self, typed: ast::DestructAssignment) -> Expr {
        let pat = Expr::Pattern(self.check_pattern(typed.pattern()));
        let val = self.check(typed.value());
        let inst = BinInst::new(ast::BinOp::Assign, pat, val);
        Expr::Binary(inst)
    }

    fn check_field_access(&mut self, typed: ast::FieldAccess) -> Expr {
        let lhs = self.check(typed.target());
        let key = Decl::ident_ref(typed.field()).into();
        let span = typed.span();
        Expr::Select(SelectExpr { lhs, key, span }.into())
    }

    fn check_func_call(&mut self, typed: ast::FuncCall) -> Expr {
        let callee = self.check(typed.callee());
        let args = self.check_args(typed.args());
        let span = typed.span();
        Expr::Apply(ApplyExpr { callee, args, span }.into())
    }

    fn check_set(&mut self, typed: ast::SetRule) -> Expr {
        let target = self.check(typed.target());
        let args = self.check_args(typed.args());
        let cond = typed.condition().map(|cond| self.check(cond));
        Expr::Set(SetExpr { target, args, cond }.into())
    }

    fn check_show(&mut self, typed: ast::ShowRule) -> Expr {
        let selector = typed.selector().map(|selector| self.check(selector));
        let edit = self.defer(typed.transform());
        Expr::Show(ShowExpr { selector, edit }.into())
    }

    fn check_conditional(&mut self, typed: ast::Conditional) -> Expr {
        let cond = self.check(typed.condition());
        let then = self.defer(typed.if_body());
        let else_ = typed
            .else_body()
            .map_or_else(none_expr, |expr| self.defer(expr));
        Expr::Conditional(IfExpr { cond, then, else_ }.into())
    }

    fn check_while_loop(&mut self, typed: ast::WhileLoop) -> Expr {
        let cond = self.check(typed.condition());
        let body = self.defer(typed.body());
        Expr::WhileLoop(WhileExpr { cond, body }.into())
    }

    fn check_for_loop(&mut self, typed: ast::ForLoop) -> Expr {
        self.with_scope(|this| {
            let pattern = this.check_pattern(typed.pattern());
            let iter = this.check(typed.iterable());
            let body = this.defer(typed.body());
            Expr::ForLoop(
                ForExpr {
                    pattern,
                    iter,
                    body,
                }
                .into(),
            )
        })
    }

    fn check_inline_markup(&mut self, markup: ast::Markup) -> Expr {
        self.check_in_mode(markup.to_untyped().children(), InterpretMode::Markup)
    }

    fn check_markup(&mut self, markup: ast::Markup) -> Expr {
        self.with_scope(|this| this.check_inline_markup(markup))
    }

    fn check_code(&mut self, code: ast::Code) -> Expr {
        self.with_scope(|this| {
            this.check_in_mode(code.to_untyped().children(), InterpretMode::Code)
        })
    }

    fn check_math(&mut self, children: SyntaxNodeChildren) -> Expr {
        self.check_in_mode(children, InterpretMode::Math)
    }

    fn check_root_scope(&mut self, children: SyntaxNodeChildren) {
        self.init_stage = true;
        self.check_in_mode(children, InterpretMode::Markup);
        self.init_stage = false;
    }

    fn check_in_mode(&mut self, children: SyntaxNodeChildren, mode: InterpretMode) -> Expr {
        let old_mode = self.lexical.mode;
        self.lexical.mode = mode;

        // collect all comments before the definition
        self.comment_matcher.reset();

        let mut items = Vec::with_capacity(4);
        for n in children {
            if let Some(expr) = n.cast::<ast::Expr>() {
                items.push(self.check(expr));
                self.comment_matcher.reset();
                continue;
            }
            if !self.init_stage && self.comment_matcher.process(n) {
                self.comment_matcher.reset();
            }
        }

        self.lexical.mode = old_mode;
        Expr::Block(items.into())
    }

    fn check_label(&mut self, label: ast::Label) -> Expr {
        let decl: Interned<Decl> = Decl::label(label.get(), label.span()).into();

        self.resolve_as(
            RefExpr {
                decl: decl.clone(),
                step: None,
                root: None,
                term: None,
            }
            .into(),
        );
        Expr::Decl(decl)
    }

    fn check_ref(&mut self, ref_node: ast::Ref) -> Expr {
        let ident = Interned::new(Decl::ref_(ref_node));
        let body = ref_node
            .supplement()
            .map(|block| self.check(ast::Expr::ContentBlock(block)));
        let ref_expr = ContentRefExpr {
            ident: ident.clone(),
            of: None,
            body,
        };
        self.resolve_as(
            RefExpr {
                decl: ident,
                step: None,
                root: None,
                term: None,
            }
            .into(),
        );
        Expr::ContentRef(ref_expr.into())
    }

    fn check_ident(&mut self, ident: ast::Ident) -> Expr {
        self.resolve_ident(Decl::ident_ref(ident).into(), InterpretMode::Code)
    }

    fn check_math_ident(&mut self, ident: ast::MathIdent) -> Expr {
        self.resolve_ident(Decl::math_ident_ref(ident).into(), InterpretMode::Math)
    }

    fn resolve_as(&mut self, r: Interned<RefExpr>) {
        self.resolve_as_(r.decl.span(), r);
    }

    fn resolve_as_(&mut self, s: Span, r: Interned<RefExpr>) {
        self.buffer.push((s, r.clone()));
    }

    fn resolve_ident(&mut self, decl: DeclExpr, mode: InterpretMode) -> Expr {
        let r: Interned<RefExpr> = self.resolve_ident_(decl, mode).into();
        let s = r.decl.span();
        self.buffer.push((s, r.clone()));
        Expr::Ref(r)
    }

    /// Resolves an identifier to a reference expression.
    ///
    /// This function looks up an identifier in the lexical scope and creates
    /// a `RefExpr` that tracks the resolution chain.
    ///
    /// # Resolution Process
    ///
    /// 1. Evaluates the identifier to get its expression and type
    ///    (`eval_ident`)
    /// 2. If the result is itself a `RefExpr`, extracts its `root` and uses the
    ///    RefExpr's `decl` as the `step` (building a reference chain)
    /// 3. Otherwise, uses the expression as both `root` and `step`
    ///
    /// # Field Assignment
    ///
    /// - `decl`: The identifier being resolved
    /// - `root`: The ultimate source of the value (extracted from chain or the
    ///   expression itself)
    /// - `step`: The immediate resolution (extracted from chain or the
    ///   expression itself)
    /// - `term`: The resolved type (if available from evaluation)
    ///
    /// # Example
    ///
    /// For `let x = 1; let y = x; let z = y`:
    /// - Resolving `x` gives: `RefExpr { decl: x, root: None, step: None, term:
    ///   Some(int) }`
    /// - Resolving `y` gives: `RefExpr { decl: y, root: Some(x), step: Some(x),
    ///   term: Some(int) }`
    /// - Resolving `z` gives: `RefExpr { decl: z, root: Some(x), step: Some(y),
    ///   term: Some(int) }`
    fn resolve_ident_(&mut self, decl: DeclExpr, mode: InterpretMode) -> RefExpr {
        let (step, val) = self.eval_ident(decl.name(), mode);
        let (root, step) = extract_ref(step);

        RefExpr {
            decl,
            root,
            step,
            term: val,
        }
    }

    fn defer(&mut self, expr: ast::Expr) -> Expr {
        if self.init_stage {
            Expr::Star
        } else {
            self.check(expr)
        }
    }

    fn collect_buffer(&mut self) {
        let mut resolves = self.resolves.lock();
        resolves.extend(self.buffer.drain(..));
        drop(resolves);
        let mut imports = self.imports.lock();
        imports.extend(self.import_buffer.drain(..));
    }

    fn const_eval_expr(&self, expr: ast::Expr) -> Option<Value> {
        SharedContext::const_eval(expr)
    }

    fn eval_expr(&mut self, expr: ast::Expr, mode: InterpretMode) -> ConcolicExpr {
        if let Some(term) = self.const_eval_expr(expr) {
            return (None, Some(Ty::Value(InsTy::new(term))));
        }
        crate::log_debug_ct!("checking expr: {expr:?}");

        match expr {
            ast::Expr::Parenthesized(paren) => self.eval_expr(paren.expr(), mode),
            ast::Expr::FieldAccess(field_access) => {
                let field = Decl::ident_ref(field_access.field());

                let (expr, term) = self.eval_expr(field_access.target(), mode);
                let term = term.and_then(|v| {
                    // todo: use type select
                    // v.select(field.name()).ok()
                    match v {
                        Ty::Value(val) => {
                            Some(Ty::Value(InsTy::new(val.val.field(field.name(), ()).ok()?)))
                        }
                        _ => None,
                    }
                });
                let sel = expr.map(|expr| Expr::Select(SelectExpr::new(field.into(), expr)));
                (sel, term)
            }
            ast::Expr::Ident(ident) => {
                let expr_term = self.eval_ident(&ident.get().into(), mode);
                crate::log_debug_ct!("checking expr: {expr:?} -> res: {expr_term:?}");
                expr_term
            }
            _ => (None, None),
        }
    }

    /// Evaluates an identifier by looking it up in the lexical scope.
    ///
    /// Returns a tuple of `(expression, type)` where:
    /// - `expression`: The expression the identifier resolves to (may be a
    ///   `RefExpr`)
    /// - `type`: The type of the value (if known)
    ///
    /// # Lookup Order
    ///
    /// 1. Current scope (`self.lexical.last`) - for block-local variables
    /// 2. Parent scopes (`self.lexical.scopes`) - for outer scope variables
    /// 3. Global/Math library scope - for built-in functions and constants
    /// 4. Special case: "std" module
    fn eval_ident(&self, name: &Interned<str>, mode: InterpretMode) -> ConcolicExpr {
        let res = self.lexical.last.get(name);
        if res.0.is_some() || res.1.is_some() {
            return res;
        }

        for scope in self.lexical.scopes.iter().rev() {
            let res = scope.get(name);
            if res.0.is_some() || res.1.is_some() {
                return res;
            }
        }

        let scope = match mode {
            InterpretMode::Math => self.ctx.world().library.math.scope(),
            InterpretMode::Markup | InterpretMode::Code => self.ctx.world().library.global.scope(),
            _ => return (None, None),
        };

        let val = scope
            .get(name)
            .cloned()
            .map(|val| Ty::Value(InsTy::new(val.read().clone())));
        if let Some(val) = val {
            return (None, Some(val));
        }

        if name.as_ref() == "std" {
            let val = Ty::Value(InsTy::new(self.ctx.world().library.std.read().clone()));
            return (None, Some(val));
        }

        (None, None)
    }

    fn fold_expr_and_val(&mut self, src: ConcolicExpr) -> Option<Expr> {
        crate::log_debug_ct!("folding cc: {src:?}");
        match src {
            (None, Some(val)) => Some(Expr::Type(val)),
            (expr, _) => self.fold_expr(expr),
        }
    }

    fn fold_expr(&mut self, expr: Option<Expr>) -> Option<Expr> {
        crate::log_debug_ct!("folding cc: {expr:?}");
        match expr {
            Some(Expr::Decl(decl)) if !decl.is_def() => {
                crate::log_debug_ct!("folding decl: {decl:?}");
                let (x, y) = self.eval_ident(decl.name(), InterpretMode::Code);
                self.fold_expr_and_val((x, y))
            }
            Some(Expr::Ref(r)) => {
                crate::log_debug_ct!("folding ref: {r:?}");
                self.fold_expr_and_val((r.root.clone(), r.term.clone()))
            }
            Some(Expr::Select(r)) => {
                let lhs = self.fold_expr(Some(r.lhs.clone()));
                crate::log_debug_ct!("folding select: {r:?} ([{lhs:?}].[{:?}])", r.key);
                self.syntax_level_select(lhs?, &r.key, r.span)
            }
            Some(expr) => {
                crate::log_debug_ct!("folding expr: {expr:?}");
                Some(expr)
            }
            _ => None,
        }
    }

    fn syntax_level_select(&mut self, lhs: Expr, key: &Interned<Decl>, span: Span) -> Option<Expr> {
        match &lhs {
            Expr::Decl(decl) => match decl.as_ref() {
                Decl::Module(module) => {
                    let exports = self.exports_of(module.fid);
                    let selected = exports.get(key.name())?;

                    let select_ref = Interned::new(RefExpr {
                        decl: key.clone(),
                        root: Some(lhs.clone()),
                        step: Some(selected.clone()),
                        term: None,
                    });
                    self.resolve_as(select_ref.clone());
                    self.resolve_as_(span, select_ref);
                    Some(selected.clone())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn exports_of(&mut self, fid: TypstFileId) -> LexicalScope {
        let imported = self
            .ctx
            .source_by_id(fid)
            .ok()
            .and_then(|src| self.ctx.exports_of(&src, self.route))
            .unwrap_or_default();
        let res = imported.as_ref().deref().clone();
        self.import_buffer.push((fid, imported));
        res
    }
}

/// Extracts the root and step from a potential reference expression.
///
/// This is a key helper function for building reference chains. It handles
/// the case where an identifier resolves to another reference.
///
/// # Returns
///
/// A tuple of `(root, step)`:
/// - If `step` is a `RefExpr`: Returns `(ref.root, Some(ref.decl))` -
///   propagates the root forward and uses the ref's declaration as the new step
/// - Otherwise: Returns `(step, step)` - the expression is both root and step
fn extract_ref(step: Option<Expr>) -> (Option<Expr>, Option<Expr>) {
    match step {
        Some(Expr::Ref(r)) => (r.root.clone(), Some(r.decl.clone().into())),
        step => (step.clone(), step),
    }
}

fn none_expr() -> Expr {
    Expr::Type(Ty::Builtin(BuiltinTy::None))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_expr_size() {
        use super::*;
        assert!(size_of::<Expr>() <= size_of::<usize>() * 2);
    }
}
