use std::ops::DerefMut;

use parking_lot::Mutex;
use rpds::RedBlackTreeMapSync;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashSet;
use tinymist_analysis::import::resolve_id_by_path;
use typst::{
    foundations::{Element, NativeElement, Value},
    model::{EmphElem, EnumElem, HeadingElem, ListElem, StrongElem, TermsElem},
    syntax::{Span, SyntaxNode},
};

use crate::{
    analysis::SharedContext,
    docs::DocStringKind,
    prelude::*,
    syntax::find_module_level_docs,
    ty::{BuiltinTy, InsTy, Interned, Ty},
};

use super::{compute_docstring, def::*, DocCommentMatcher, DocString, InterpretMode};

pub(crate) fn expr_of(ctx: Arc<SharedContext>, source: Source) -> Arc<ExprInfo> {
    log::debug!("expr_of: {:?}", source.id());

    let resolves_base = Arc::new(Mutex::new(vec![]));
    let resolves = resolves_base.clone();

    // todo: cache docs capture
    let docstrings_base = Arc::new(Mutex::new(FxHashMap::default()));
    let docstrings = docstrings_base.clone();

    let exprs_base = Arc::new(Mutex::new(FxHashMap::default()));
    let exprs = exprs_base.clone();

    let imports_base = Arc::new(Mutex::new(FxHashSet::default()));
    let imports = imports_base.clone();

    let module_docstring = Arc::new(
        find_module_level_docs(&source)
            .and_then(|docs| compute_docstring(&ctx, source.id(), docs, DocStringKind::Module))
            .unwrap_or_default(),
    );

    let (exports, root) = {
        let mut w = ExprWorker {
            fid: source.id(),
            ctx,
            imports,
            docstrings,
            exprs,
            import_buffer: Vec::new(),
            lexical: LexicalContext {
                mode: InterpretMode::Markup,
                scopes: eco_vec![],
                last: ExprScope::Lexical(RedBlackTreeMapSync::default()),
            },
            resolves,
            buffer: vec![],
            comment_matcher: DocCommentMatcher::default(),
        };
        let root = source.root().cast::<ast::Markup>().unwrap();
        let root = w.check_in_mode(root.to_untyped().children(), InterpretMode::Markup);
        w.collect_buffer();

        (w.summarize_scope(), root)
    };

    let info = ExprInfo {
        fid: source.id(),
        resolves: HashMap::from_iter(std::mem::take(resolves_base.lock().deref_mut())),
        module_docstring,
        docstrings: std::mem::take(docstrings_base.lock().deref_mut()),
        imports: HashSet::from_iter(std::mem::take(imports_base.lock().deref_mut())),
        exports,
        exprs: std::mem::take(exprs_base.lock().deref_mut()),
        root,
    };
    log::debug!("expr_of end {:?}", source.id());

    Arc::new(info)
}

#[derive(Debug)]
pub struct ExprInfo {
    pub fid: TypstFileId,
    pub resolves: FxHashMap<Span, Interned<RefExpr>>,
    pub module_docstring: Arc<DocString>,
    pub docstrings: FxHashMap<DeclExpr, Arc<DocString>>,
    pub exprs: FxHashMap<Span, Expr>,
    pub imports: FxHashSet<TypstFileId>,
    pub exports: LexicalScope,
    pub root: Expr,
}

impl std::hash::Hash for ExprInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.root.hash(state);
    }
}

impl ExprInfo {
    pub fn get_refs(
        &self,
        decl: Interned<Decl>,
    ) -> impl Iterator<Item = (&Span, &Interned<RefExpr>)> {
        let of = Some(Expr::Decl(decl.clone()));
        self.resolves
            .iter()
            .filter(move |(_, r)| r.decl == decl || r.root == of)
    }

    pub fn is_exported(&self, decl: &Interned<Decl>) -> bool {
        let of = Expr::Decl(decl.clone());
        self.exports.get(decl.name()).map_or(false, |e| match e {
            Expr::Ref(r) => r.root == Some(of),
            e => *e == of,
        })
    }

    #[allow(dead_code)]
    fn show(&self) {
        use std::io::Write;
        let vpath = self
            .fid
            .vpath()
            .resolve(Path::new("target/exprs/"))
            .unwrap();
        let root = vpath.with_extension("root.expr");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::write(root, format!("{}", self.root)).unwrap();
        let scopes = vpath.with_extension("scopes.expr");
        std::fs::create_dir_all(scopes.parent().unwrap()).unwrap();
        {
            let mut scopes = std::fs::File::create(scopes).unwrap();
            for (s, e) in self.exprs.iter() {
                writeln!(scopes, "{s:?} -> {e}").unwrap();
            }
        }
        let imports = vpath.with_extension("imports.expr");
        std::fs::create_dir_all(imports.parent().unwrap()).unwrap();
        std::fs::write(imports, format!("{:#?}", self.imports)).unwrap();
        let exports = vpath.with_extension("exports.expr");
        std::fs::create_dir_all(exports.parent().unwrap()).unwrap();
        std::fs::write(exports, format!("{:#?}", self.exports)).unwrap();
    }
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

pub(crate) struct ExprWorker {
    fid: TypstFileId,
    ctx: Arc<SharedContext>,
    imports: Arc<Mutex<FxHashSet<TypstFileId>>>,
    import_buffer: Vec<TypstFileId>,
    docstrings: Arc<Mutex<FxHashMap<DeclExpr, Arc<DocString>>>>,
    exprs: Arc<Mutex<FxHashMap<Span, Expr>>>,
    resolves: Arc<Mutex<ResolveVec>>,
    buffer: ResolveVec,
    lexical: LexicalContext,

    comment_matcher: DocCommentMatcher,
}

impl ExprWorker {
    fn with_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.lexical.scopes.push(std::mem::replace(
            &mut self.lexical.last,
            ExprScope::Lexical(RedBlackTreeMapSync::default()),
        ));
        let len = self.lexical.scopes.len();
        let result = f(self);
        self.lexical.scopes.truncate(len);
        self.lexical.last = self.lexical.scopes.pop().unwrap();
        result
    }

    #[must_use]
    fn scope_mut(&mut self) -> &mut LexicalScope {
        if matches!(self.lexical.last, ExprScope::Lexical(_)) {
            return self.lexical_scope_unchecked();
        }
        self.lexical.scopes.push(std::mem::replace(
            &mut self.lexical.last,
            ExprScope::Lexical(RedBlackTreeMapSync::default()),
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

    fn check_docstring(&mut self, decl: &DeclExpr, kind: DocStringKind) {
        if let Some(docs) = self.comment_matcher.collect() {
            let docstring = compute_docstring(&self.ctx, self.fid, docs, kind);
            if let Some(docstring) = docstring {
                self.docstrings
                    .lock()
                    .insert(decl.clone(), Arc::new(docstring));
            }
        }
    }

    fn summarize_scope(&self) -> LexicalScope {
        let mut exports = LexicalScope::default();
        for scope in std::iter::once(&self.lexical.last).chain(self.lexical.scopes.iter()) {
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
            Code(code_block) => self.check_code(code_block.body()),
            Content(c) => self.check_markup(c.body()),

            Ident(ident) => self.check_ident(ident),
            MathIdent(math_ident) => self.check_math_ident(math_ident),
            Label(label) => self.check_label(label),
            Ref(r) => self.check_ref(r),

            Let(let_binding) => self.check_let(let_binding),
            Closure(closure) => self.check_closure(closure),
            Import(module_import) => self.check_module_import(module_import),
            Include(module_include) => self.check_module_include(module_include),

            Parenthesized(p) => self.check(p.expr()),
            Array(array) => self.check_array(array),
            Dict(dict) => self.check_dict(dict),
            Unary(unary) => self.check_unary(unary),
            Binary(binary) => self.check_binary(binary),
            FieldAccess(field_access) => self.check_field_access(field_access),
            FuncCall(func_call) => self.check_func_call(func_call),
            DestructAssign(destruct_assignment) => self.check_destruct_assign(destruct_assignment),
            Set(set_rule) => self.check_set(set_rule),
            Show(show_rule) => self.check_show(show_rule),
            Contextual(contextual) => {
                Expr::Unary(UnInst::new(UnaryOp::Context, self.check(contextual.body())))
            }
            Conditional(conditional) => self.check_conditional(conditional),
            While(while_loop) => self.check_while_loop(while_loop),
            For(for_loop) => self.check_for_loop(for_loop),
            Break(..) => Expr::Type(Ty::Builtin(BuiltinTy::Break)),
            Continue(..) => Expr::Type(Ty::Builtin(BuiltinTy::Continue)),
            Return(func_return) => Expr::Unary(UnInst::new(
                UnaryOp::Return,
                func_return
                    .body()
                    .map_or_else(none_expr, |body| self.check(body)),
            )),

            Text(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Raw(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Link(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Space(..) => Expr::Type(Ty::Builtin(BuiltinTy::Space)),
            Linebreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Parbreak(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Escape(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            Shorthand(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            SmartQuote(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),

            Strong(e) => {
                let body = self.check_inline_markup(e.body());
                self.check_element::<StrongElem>(eco_vec![body])
            }
            Emph(e) => {
                let body = self.check_inline_markup(e.body());
                self.check_element::<EmphElem>(eco_vec![body])
            }
            Heading(e) => {
                let body = self.check_markup(e.body());
                self.check_element::<HeadingElem>(eco_vec![body])
            }
            List(e) => {
                let body = self.check_markup(e.body());
                self.check_element::<ListElem>(eco_vec![body])
            }
            Enum(e) => {
                let body = self.check_markup(e.body());
                self.check_element::<EnumElem>(eco_vec![body])
            }
            Term(t) => {
                let term = self.check_markup(t.term());
                let description = self.check_markup(t.description());
                self.check_element::<TermsElem>(eco_vec![term, description])
            }

            MathAlignPoint(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            MathShorthand(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            MathDelimited(math_delimited) => {
                self.check_math(math_delimited.body().to_untyped().children())
            }
            MathAttach(ma) => {
                let base = ma.base().to_untyped().clone();
                let bottom = ma.bottom().unwrap_or_default().to_untyped().clone();
                let top = ma.top().unwrap_or_default().to_untyped().clone();
                self.check_math([base, bottom, top].iter())
            }
            MathPrimes(..) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            MathFrac(mf) => {
                let num = mf.num().to_untyped().clone();
                let denom = mf.denom().to_untyped().clone();
                self.check_math([num, denom].iter())
            }
            MathRoot(mr) => self.check(mr.radicand()),
        }
    }

    fn check_label(&mut self, ident: ast::Label) -> Expr {
        Expr::Decl(Decl::label(ident.get(), ident.span()).into())
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
            ast::LetBindingKind::Normal(p) => {
                // Check init expression before pattern checking
                let body = typed.init().map(|e| self.defer(e));

                let span = p.span();
                let decl = Decl::pattern(span).into();
                self.check_docstring(&decl, DocStringKind::Variable);
                let pattern = self.check_pattern(p);
                Expr::Let(Interned::new(LetExpr {
                    span,
                    pattern,
                    body,
                }))
            }
        }
    }

    fn check_closure(&mut self, typed: ast::Closure) -> Expr {
        let decl = match typed.name() {
            Some(name) => Decl::func(name).into(),
            None => Decl::closure(typed.span()).into(),
        };
        self.check_docstring(&decl, DocStringKind::Function);
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
                        let val = this.check(arg.expr());
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

                        let spreaded = this.check(s.expr());
                        if inputs.is_empty() {
                            spread_left = Some((decl.clone(), spreaded));
                        } else {
                            spread_right = Some((decl.clone(), spreaded));
                        }

                        this.resolve_as(Decl::as_def(&decl, None));
                        this.scope_mut()
                            .insert_mut(decl.name().clone(), decl.into());
                    }
                }
            }

            let pattern = Pattern {
                pos: inputs,
                named: names,
                spread_left,
                spread_right,
            };

            (pattern.into(), this.defer(typed.body()))
        });

        self.scope_mut()
            .insert_mut(decl.name().clone(), decl.clone().into());
        Expr::Func(FuncExpr { decl, params, body }.into())
    }

    fn check_pattern(&mut self, typed: ast::Pattern) -> Expr {
        match typed {
            ast::Pattern::Normal(expr) => self.check_pattern_expr(expr),
            ast::Pattern::Placeholder(..) => Expr::Star,
            ast::Pattern::Parenthesized(p) => self.check_pattern(p.pattern()),
            ast::Pattern::Destructuring(d) => {
                let mut inputs = eco_vec![];
                let mut names = eco_vec![];
                let mut spread_left = None;
                let mut spread_right = None;

                for item in d.items() {
                    match item {
                        ast::DestructuringItem::Pattern(p) => {
                            inputs.push(self.check_pattern(p));
                        }
                        ast::DestructuringItem::Named(n) => {
                            let key = Decl::var(n.name()).into();
                            let val = self.check_pattern_expr(n.expr());
                            names.push((key, val));
                        }
                        ast::DestructuringItem::Spread(s) => {
                            let decl: DeclExpr = if let Some(ident) = s.sink_ident() {
                                Decl::var(ident).into()
                            } else {
                                Decl::spread(s.span()).into()
                            };

                            if inputs.is_empty() {
                                spread_left = Some((decl, self.check_pattern_expr(s.expr())));
                            } else {
                                spread_right = Some((decl, self.check_pattern_expr(s.expr())));
                            }
                        }
                    }
                }

                let pattern = Pattern {
                    pos: inputs,
                    named: names,
                    spread_left,
                    spread_right,
                };

                Expr::Pattern(pattern.into())
            }
        }
    }

    fn check_pattern_expr(&mut self, typed: ast::Expr) -> Expr {
        match typed {
            ast::Expr::Ident(ident) => {
                let decl = Decl::var(ident).into();
                self.resolve_as(Decl::as_def(&decl, None));
                self.scope_mut()
                    .insert_mut(decl.name().clone(), decl.clone().into());
                Expr::Decl(decl)
            }
            ast::Expr::Parenthesized(parenthesized) => self.check_pattern(parenthesized.pattern()),
            ast::Expr::Closure(c) => self.check_closure(c),
            _ => self.check(typed),
        }
    }

    fn check_module_import(&mut self, typed: ast::ModuleImport) -> Expr {
        let source = typed.source();
        log::debug!("checking import: {source:?}");
        let src = self.eval_expr(source, InterpretMode::Code);
        let src_expr = self.fold_expr_and_val(src).or_else(|| {
            self.ctx
                .analyze_expr(source.to_untyped())
                .into_iter()
                .find_map(|(v, _)| match v {
                    Value::Str(s) => Some(Expr::Type(Ty::Value(InsTy::new(Value::Str(s))))),
                    _ => None,
                })
        });

        let mod_expr = src_expr.as_ref().and_then(|src_expr| {
            log::debug!("checking import source: {src_expr:?}");
            let src_str = match src_expr {
                Expr::Type(Ty::Value(val)) => {
                    if val.val.scope().is_some() {
                        return Some(src_expr.clone());
                    }

                    match &val.val {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    }
                }
                Expr::Decl(d) if matches!(d.as_ref(), Decl::Module { .. }) => {
                    return Some(src_expr.clone())
                }

                _ => None,
            }?;

            let fid = resolve_id_by_path(&self.ctx.world, self.fid, src_str)?;
            let name = Decl::calc_path_stem(src_str);
            Some(Expr::Decl(Decl::module(name, fid).into()))
        });

        let decl = typed.new_name().map(Decl::module_alias).or_else(|| {
            typed.imports().is_none().then(|| {
                let name = match mod_expr.as_ref()? {
                    Expr::Decl(d) if matches!(d.as_ref(), Decl::Module { .. }) => d.name().clone(),
                    _ => return None,
                };
                // todo: package stem
                Some(Decl::path_stem(source.to_untyped().clone(), name))
            })?
        });

        let is_named = decl.is_some();
        let decl = Interned::new(decl.unwrap_or_else(|| Decl::module_import(typed.span())));
        let mod_ref = RefExpr {
            decl: decl.clone(),
            step: mod_expr.clone(),
            root: mod_expr.clone(),
            val: None,
        };
        log::debug!("create import variable: {mod_ref:?}");
        let mod_ref = Interned::new(mod_ref);
        if is_named {
            self.scope_mut()
                .insert_mut(decl.name().clone(), Expr::Ref(mod_ref.clone()));
        }

        self.resolve_as(mod_ref);

        let fid = mod_expr.as_ref().and_then(|mod_expr| match mod_expr {
            Expr::Type(Ty::Value(v)) => match &v.val {
                Value::Module(m) => m.file_id(),
                _ => None,
            },
            Expr::Decl(d) => {
                if matches!(d.as_ref(), Decl::Module { .. }) {
                    d.file_id()
                } else {
                    None
                }
            }
            _ => None,
        });

        // Prefetch Type Check Information
        if let Some(f) = fid {
            log::debug!("prefetch type check: {f:?}");
            self.ctx.prefetch_type_check(f);
            self.import_buffer.push(f);
        }

        let pattern;

        let scope = if let Some(fid) = &fid {
            let source = self.ctx.source_by_id(*fid);
            if let Ok(source) = source {
                Some(ExprScope::Lexical(self.ctx.exports_of(source)))
            } else {
                None
            }
        } else {
            match &mod_expr {
                Some(Expr::Type(Ty::Value(v))) => match &v.val {
                    Value::Module(m) => Some(ExprScope::Module(m.clone())),
                    Value::Func(f) => {
                        if f.scope().is_some() {
                            Some(ExprScope::Func(f.clone()))
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
            log::debug!(
                "cannot analyze import on: {typed:?}, expr {mod_expr:?}, in file {:?}",
                typed.span().id()
            );
            ExprScope::empty()
        };

        if let Some(imports) = typed.imports() {
            match imports {
                ast::Imports::Wildcard => {
                    log::debug!("checking wildcard: {mod_expr:?}");
                    self.lexical.scopes.push(scope);

                    pattern = Expr::Star;
                }
                ast::Imports::Items(items) => {
                    let module = Expr::Decl(decl.clone());
                    pattern = self.import_decls(&scope, module, items);
                }
            }
        } else {
            pattern = none_expr();
        };

        Expr::Import(ImportExpr { decl, pattern }.into())
    }

    fn import_decls(&mut self, scope: &ExprScope, module: Expr, items: ast::ImportItems) -> Expr {
        let mut imported = eco_vec![];
        log::debug!("import scope {scope:?}");

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

            let mut path = Vec::with_capacity(1);
            for seg in path_ast.iter() {
                let seg = Interned::new(Decl::ident_ref(seg));
                path.push(seg);
            }
            // todo: import path
            let (mut root, val) = match path.last().map(|d| d.name()) {
                Some(name) => scope.get(name),
                None => (None, None),
            };

            log::debug!("path {path:?} -> {root:?} {val:?}");
            if root.is_none() && val.is_none() {
                let mut sel = module.clone();
                for seg in path.into_iter() {
                    sel = Expr::Select(SelectExpr::new(seg, sel));
                }
                root = Some(sel)
            }

            let (root, step) = extract_ref(root);
            let mut ref_expr = Interned::new(RefExpr {
                decl: old.clone(),
                root,
                step,
                val,
            });
            self.resolve_as(ref_expr.clone());

            if let Some(new) = &rename {
                ref_expr = Interned::new(RefExpr {
                    decl: new.clone(),
                    root: ref_expr.root.clone(),
                    step: Some(ref_expr.decl.clone().into()),
                    val: ref_expr.val.clone(),
                });
                self.resolve_as(ref_expr.clone());
            }

            // final resolves
            let name = rename.as_ref().unwrap_or(&old).name().clone();
            let expr = Expr::Ref(ref_expr);
            self.scope_mut().insert_mut(name, expr.clone());
            imported.push((old, expr));
        }

        Expr::Pattern(
            Pattern {
                pos: eco_vec![],
                named: imported,
                spread_left: None,
                spread_right: None,
            }
            .into(),
        )
    }

    fn check_module_include(&mut self, typed: ast::ModuleInclude) -> Expr {
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

        Expr::Array(items.into())
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
                    let analyzed = self.const_eval_expr(key);
                    let analyzed = match &analyzed {
                        Some(Value::Str(s)) => Some(s),
                        _ => None,
                    };
                    let Some(analyzed) = analyzed else {
                        let key = self.check(key);
                        items.push(ArgExpr::NamedRt(Box::new((key, val))));
                        continue;
                    };
                    let key = Decl::str_name(key.to_untyped().clone(), analyzed).into();
                    items.push(ArgExpr::Named(Box::new((key, val))));
                }
                ast::DictItem::Spread(s) => {
                    items.push(ArgExpr::Spread(self.check(s.expr())));
                }
            }
        }

        Expr::Dict(items.into())
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
        Expr::Args(args.into())
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
        let pat = self.check_pattern(typed.pattern());
        let val = self.check(typed.value());
        let inst = BinInst::new(ast::BinOp::Assign, pat, val);
        Expr::Binary(inst)
    }

    fn check_field_access(&mut self, typed: ast::FieldAccess) -> Expr {
        let lhs = self.check(typed.target());
        let rhs = Decl::ident_ref(typed.field()).into();
        Expr::Select(SelectExpr { lhs, key: rhs }.into())
    }

    fn check_func_call(&mut self, typed: ast::FuncCall) -> Expr {
        let callee = self.check(typed.callee());
        let args = self.check_args(typed.args());
        Expr::Apply(ApplyExpr { callee, args }.into())
    }

    fn check_set(&mut self, typed: ast::SetRule) -> Expr {
        let target = self.check(typed.target());
        let args = self.check_args(typed.args());
        let cond = typed.condition().map(|c| self.check(c));
        Expr::Set(SetExpr { target, args, cond }.into())
    }

    fn check_show(&mut self, typed: ast::ShowRule) -> Expr {
        let selector = typed.selector().map(|s| self.check(s));
        let edit = self.defer(typed.transform());
        Expr::Show(ShowExpr { selector, edit }.into())
    }

    fn check_conditional(&mut self, typed: ast::Conditional) -> Expr {
        let cond = self.check(typed.condition());
        let then = self.check(typed.if_body());
        let else_ = typed
            .else_body()
            .map_or_else(none_expr, |expr| self.check(expr));
        Expr::Conditional(IfExpr { cond, then, else_ }.into())
    }

    fn check_while_loop(&mut self, typed: ast::WhileLoop) -> Expr {
        let cond = self.check(typed.condition());
        let body = self.check(typed.body());
        Expr::WhileLoop(WhileExpr { cond, body }.into())
    }

    fn check_for_loop(&mut self, typed: ast::ForLoop) -> Expr {
        self.with_scope(|this| {
            let pattern = this.check_pattern(typed.pattern());
            let iter = this.check(typed.iterable());
            let body = this.check(typed.body());
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

    fn check_inline_markup(&mut self, m: ast::Markup) -> Expr {
        self.check_in_mode(m.to_untyped().children(), InterpretMode::Markup)
    }

    fn check_markup(&mut self, m: ast::Markup) -> Expr {
        self.with_scope(|this| this.check_inline_markup(m))
    }

    fn check_code(&mut self, m: ast::Code) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.to_untyped().children(), InterpretMode::Code))
    }

    fn check_math(&mut self, root: SyntaxNodeChildren) -> Expr {
        self.check_in_mode(root, InterpretMode::Math)
    }

    fn check_in_mode(&mut self, root: SyntaxNodeChildren, mode: InterpretMode) -> Expr {
        let old_mode = self.lexical.mode;
        self.lexical.mode = mode;

        // collect all comments before the definition
        self.comment_matcher.reset();

        let mut children = Vec::with_capacity(4);
        for n in root {
            if let Some(expr) = n.cast::<ast::Expr>() {
                children.push(self.check(expr));
                self.comment_matcher.reset();
                continue;
            }
            self.comment_matcher.process(n);
        }

        self.lexical.mode = old_mode;
        Expr::Seq(children.into())
    }

    fn check_ref(&mut self, r: ast::Ref) -> Expr {
        let ident = Decl::ref_(r).into();
        let body = r.supplement().map(|s| self.check(ast::Expr::Content(s)));
        let ref_expr = ContentRefExpr {
            ident,
            of: None,
            body,
        };
        Expr::ContentRef(ref_expr.into())
    }

    fn check_ident(&mut self, ident: ast::Ident) -> Expr {
        self.resolve_ident(Decl::ident_ref(ident).into(), InterpretMode::Code)
    }

    fn check_math_ident(&mut self, ident: ast::MathIdent) -> Expr {
        self.resolve_ident(Decl::math_ident_ref(ident).into(), InterpretMode::Math)
    }

    fn resolve_as(&mut self, r: Interned<RefExpr>) {
        let s = r.decl.span().unwrap();
        self.buffer.push((s, r.clone()));
    }

    fn resolve_ident(&mut self, decl: DeclExpr, mode: InterpretMode) -> Expr {
        let r: Interned<RefExpr> = self.resolve_ident_(decl, mode).into();
        let s = r.decl.span().unwrap();
        self.buffer.push((s, r.clone()));
        Expr::Ref(r)
    }

    fn resolve_ident_(&mut self, decl: DeclExpr, mode: InterpretMode) -> RefExpr {
        let (step, val) = self.eval_ident(decl.name(), mode);
        let (root, step) = extract_ref(step);

        RefExpr {
            decl,
            root,
            step,
            val,
        }
    }

    fn defer(&mut self, expr: ast::Expr) -> DeferExpr {
        let expr = expr.to_untyped().clone();
        let span = expr.span();

        let new = self;
        new.check(expr.cast().unwrap());

        // let fid = self.fid;
        // let ctx = self.ctx.clone();
        // let imports = self.imports.clone();
        // let resolves = self.resolves.clone();
        // let scopes = self.scopes.clone();
        // let lexical = self.lexical.clone();

        // self.scope.spawn(move |t| {
        //     let mut new = ExprWorker {
        //         fid,
        //         scope: t,
        //         ctx,
        //         imports,
        //         scopes,
        //         resolves,
        //         lexical,
        //         import_buffer: vec![],
        //         buffer: vec![],
        //     };

        //     let ret = new.check(expr.cast().unwrap());
        //     new.collect_buffer();
        //     new.scopes.lock().insert(expr.span(), ret);
        // });

        DeferExpr { span }
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
        if let Some(s) = self.const_eval_expr(expr) {
            return (None, Some(Ty::Value(InsTy::new(s))));
        }
        log::debug!("checking expr: {expr:?}");

        match expr {
            ast::Expr::FieldAccess(f) => {
                let field = Decl::ident_ref(f.field());

                let (expr, val) = self.eval_expr(f.target(), mode);
                let val = val.and_then(|v| {
                    // todo: use type select
                    // v.select(field.name()).ok()
                    match v {
                        Ty::Value(val) => {
                            Some(Ty::Value(InsTy::new(val.val.field(field.name()).ok()?)))
                        }
                        _ => None,
                    }
                });
                let expr = expr.map(|e| Expr::Select(SelectExpr::new(field.into(), e)));
                (expr, val)
            }
            ast::Expr::Ident(ident) => {
                let res = self.eval_ident(&ident.get().into(), mode);
                log::debug!("checking expr: {expr:?} -> res: {res:?}");
                res
            }
            _ => (None, None),
        }
    }

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
            InterpretMode::Math => self.ctx.world.library.math.scope(),
            InterpretMode::Markup | InterpretMode::Code => self.ctx.world.library.global.scope(),
            _ => return (None, None),
        };

        // ref_expr.val = val.map(|v| Ty::Value(InsTy::new(v.clone())));
        let val = scope
            .get(name)
            .cloned()
            .map(|val| Ty::Value(InsTy::new(val)));
        (None, val)
    }

    fn fold_expr_and_val(&self, src: ConcolicExpr) -> Option<Expr> {
        log::debug!("folding cc: {src:?}");
        match src {
            (None, Some(val)) => Some(Expr::Type(val)),
            (expr, _) => self.fold_expr(expr),
        }
    }

    fn fold_expr(&self, src: Option<Expr>) -> Option<Expr> {
        log::debug!("folding cc: {src:?}");
        match src {
            Some(Expr::Decl(decl)) if !decl.is_def() => {
                log::debug!("folding decl: {decl:?}");
                let (x, y) = self.eval_ident(decl.name(), InterpretMode::Code);
                self.fold_expr_and_val((x, y))
            }
            Some(Expr::Ref(r)) => {
                log::debug!("folding ref: {r:?}");
                self.fold_expr_and_val((r.root.clone(), r.val.clone()))
            }
            Some(expr) => {
                log::debug!("folding expr: {expr:?}");
                Some(expr)
            }
            _ => None,
        }
    }
}

fn extract_ref(step: Option<Expr>) -> (Option<Expr>, Option<Expr>) {
    match step {
        Some(Expr::Ref(r)) => (r.root.clone(), Some(r.decl.clone().into())),
        step => (step.clone(), step),
    }
}

fn none_expr() -> Expr {
    Expr::Type(Ty::Builtin(BuiltinTy::None))
}
