use core::fmt;
use std::{collections::BTreeMap, ops::DerefMut};

use parking_lot::Mutex;
use rpds::RedBlackTreeMapSync;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashSet;
use tinymist_analysis::import::resolve_id_by_path;
use typst::{
    foundations::{Element, Func, Module, Type, Value},
    model::{EmphElem, EnumElem, HeadingElem, ListElem, StrongElem},
    syntax::{Span, SyntaxNode},
};

use crate::{
    adt::interner::impl_internable,
    analysis::SharedContext,
    prelude::*,
    ty::{BuiltinTy, InsTy, Interned, SelectTy, Ty},
};

use super::InterpretMode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A sequence of expressions
    Seq(Interned<Vec<Expr>>),
    /// A array literal
    Array(Interned<Vec<ArgExpr>>),
    /// A dict literal
    Dict(Interned<Vec<ArgExpr>>),
    /// An args literal
    Args(Interned<Vec<ArgExpr>>),
    /// An args literal
    Pattern(Interned<Pattern>),
    /// A element literal
    Element(Interned<ElementExpr>),
    /// A unary operation
    Unary(Interned<UnExpr>),
    /// A binary operation
    Binary(Interned<BinExpr>),
    /// A function call
    Apply(Interned<ApplyExpr>),
    /// A function
    Func(Interned<FuncExpr>),
    /// A let
    Let(Interned<LetExpr>),
    Show(Interned<ShowExpr>),
    Set(Interned<SetExpr>),
    Ref(Interned<RefExpr>),
    ContentRef(Interned<ContentRefExpr>),
    Select(Interned<SelectExpr>),
    Import(Interned<ImportExpr>),
    Include(Interned<IncludeExpr>),
    Contextual(Interned<Expr>),
    Conditional(Interned<IfExpr>),
    WhileLoop(Interned<WhileExpr>),
    ForLoop(Interned<ForExpr>),
    Type(Ty),
    Decl(DeclExpr),
    Star,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ExprFormatter::new(f).write_expr(self)
    }
}

pub(crate) fn expr_of(ctx: Arc<SharedContext>, source: Source) -> Arc<ExprInfo> {
    log::debug!(
        "expr_of: {:?} in thread {:?}",
        source.id(),
        rayon::current_thread_index()
    );

    let imports_base = Arc::new(Mutex::new(FxHashSet::default()));
    let imports = imports_base.clone();

    let resolves_base = Arc::new(Mutex::new(vec![]));
    let resolves = resolves_base.clone();

    let scopes_base = Arc::new(Mutex::new(FxHashMap::default()));
    let scopes = scopes_base.clone();

    let (exports, root) = rayon::scope(|s| {
        let mut w = ExprWorker {
            fid: source.id(),
            _scope: s,
            ctx,
            imports,
            scopes,
            import_buffer: Vec::new(),
            lexical: LexicalContext {
                mode: InterpretMode::Markup,
                scopes: eco_vec![],
                last: ExprScope::Lexical(RedBlackTreeMapSync::default()),
            },
            resolves,
            buffer: vec![],
        };
        let root = source.root().cast::<ast::Markup>().unwrap();
        let root = w.check_in_mode(root.exprs(), InterpretMode::Markup);
        w.collect_buffer();

        (w.summarize_scope(), root)
    });

    let info = ExprInfo {
        fid: source.id(),
        resolves: HashMap::from_iter(std::mem::take(resolves_base.lock().deref_mut())),
        imports: HashSet::from_iter(std::mem::take(imports_base.lock().deref_mut())),
        exports,
        scopes: std::mem::take(scopes_base.lock().deref_mut()),
        root,
    };
    log::debug!("expr_of end {:?}", source.id());

    Arc::new(info)
}

#[derive(Debug)]
pub struct ExprInfo {
    pub fid: TypstFileId,
    pub resolves: FxHashMap<Span, Interned<RefExpr>>,
    pub scopes: FxHashMap<Span, Expr>,
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
            for (s, e) in self.scopes.iter() {
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

pub type LexicalScope = rpds::RedBlackTreeMapSync<Interned<str>, Expr>;

#[derive(Debug, Clone)]
enum ExprScope {
    Lexical(LexicalScope),
    Module(Module),
    Func(Func),
    Type(Type),
}

type ResolveVec = Vec<(Span, Interned<RefExpr>)>;

#[derive(Debug, Clone)]
struct LexicalContext {
    mode: InterpretMode,
    scopes: EcoVec<ExprScope>,
    last: ExprScope,
}

pub(crate) struct ExprWorker<'a, 's> {
    fid: TypstFileId,
    _scope: &'a rayon::Scope<'s>,
    ctx: Arc<SharedContext>,
    imports: Arc<Mutex<FxHashSet<TypstFileId>>>,
    import_buffer: Vec<TypstFileId>,
    scopes: Arc<Mutex<FxHashMap<Span, Expr>>>,
    resolves: Arc<Mutex<ResolveVec>>,
    buffer: ResolveVec,
    lexical: LexicalContext,
}

impl<'a, 's> ExprWorker<'a, 's> {
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

    fn summarize_scope(&mut self) -> LexicalScope {
        log::debug!("summarize_scope: {:?}", self.lexical.scopes);
        let mut exports = LexicalScope::default();
        for scope in std::mem::take(&mut self.lexical.scopes).into_iter() {
            match scope {
                ExprScope::Lexical(scope) => {
                    for (name, expr) in scope.iter() {
                        exports.insert_mut(name.clone(), expr.clone());
                    }
                }
                ExprScope::Module(module) => {
                    log::debug!("imported: {module:?}");
                    let v = Interned::new(Ty::Value(InsTy::new(Value::Module(module.clone()))));
                    for (name, _, _) in module.scope().iter() {
                        let name: Interned<str> = name.into();
                        exports.insert_mut(name.clone(), select_of(v.clone(), name));
                    }
                }
                ExprScope::Func(func) => {
                    if let Some(scope) = func.scope() {
                        let v = Interned::new(Ty::Value(InsTy::new(Value::Func(func.clone()))));
                        for (name, _, _) in scope.iter() {
                            let name: Interned<str> = name.into();
                            exports.insert_mut(name.clone(), select_of(v.clone(), name));
                        }
                    }
                }
                ExprScope::Type(ty) => {
                    let v = Interned::new(Ty::Value(InsTy::new(Value::Type(ty))));
                    for (name, _, _) in ty.scope().iter() {
                        let name: Interned<str> = name.into();
                        exports.insert_mut(name.clone(), select_of(v.clone(), name));
                    }
                }
            }
        }
        exports
    }

    fn check(&mut self, m: ast::Expr) -> Expr {
        use ast::Expr::*;
        match m {
            None(_) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            Auto(..) => Expr::Type(Ty::Builtin(BuiltinTy::Auto)),
            Bool(bool) => Expr::Type(Ty::Value(InsTy::new(Value::Bool(bool.get())))),
            Int(int) => Expr::Type(Ty::Value(InsTy::new(Value::Int(int.get())))),
            Float(float) => Expr::Type(Ty::Value(InsTy::new(Value::Float(float.get())))),
            Numeric(numeric) => Expr::Type(Ty::Value(InsTy::new(Value::numeric(numeric.get())))),
            Str(s) => Expr::Type(Ty::Value(InsTy::new(Value::Str(s.get().into())))),

            Equation(equation) => self.check_math(equation.body()),
            Math(math) => self.check_math(math),
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
            Strong(e) => self.check_element(Element::of::<StrongElem>(), e.body().exprs()),
            Emph(e) => self.check_element(Element::of::<EmphElem>(), e.body().exprs()),
            Heading(e) => self.check_element(Element::of::<HeadingElem>(), e.body().exprs()),
            List(e) => self.check_element(Element::of::<ListElem>(), e.body().exprs()),
            Enum(e) => self.check_element(Element::of::<EnumElem>(), e.body().exprs()),
            Term(t) => self.check_element(
                Element::of::<EnumElem>(),
                t.term().exprs().chain(t.description().exprs()),
            ),

            MathAlignPoint(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            MathShorthand(..) => Expr::Type(Ty::Builtin(BuiltinTy::Content)),
            MathDelimited(math_delimited) => {
                self.check_in_mode(math_delimited.body().exprs(), InterpretMode::Math)
            }
            MathAttach(ma) => self.check_in_mode(
                [
                    ma.base(),
                    ma.bottom().unwrap_or_default(),
                    ma.top().unwrap_or_default(),
                ]
                .into_iter(),
                InterpretMode::Math,
            ),
            MathPrimes(..) => Expr::Type(Ty::Builtin(BuiltinTy::None)),
            MathFrac(mf) => {
                self.check_in_mode([mf.num(), mf.denom()].into_iter(), InterpretMode::Math)
            }
            MathRoot(mr) => self.check(mr.radicand()),
        }
    }

    fn check_label(&mut self, ident: ast::Label) -> Expr {
        Expr::Decl(Decl::label(ident).into())
    }

    fn check_element<'b>(
        &mut self,
        elem: Element,
        root: impl Iterator<Item = ast::Expr<'b>>,
    ) -> Expr {
        let content = root
            .filter(|e| !matches!(e, ast::Expr::Space(..)))
            .map(|b| self.check(b))
            .collect();
        Expr::Element(ElementExpr { elem, content }.into())
    }

    fn check_let(&mut self, typed: ast::LetBinding) -> Expr {
        match typed.kind() {
            ast::LetBindingKind::Closure(..) => {
                typed.init().map_or_else(none_expr, |expr| self.check(expr))
            }
            ast::LetBindingKind::Normal(p) => {
                let body = typed.init().map(|e| self.defer(e));
                let pattern = self.check_pattern(p);
                Expr::Let(LetExpr { pattern, body }.into())
            }
        }
    }

    fn check_closure(&mut self, typed: ast::Closure) -> Expr {
        let decl = match typed.name() {
            Some(name) => Decl::func(name).into(),
            None => Decl::Closure(typed.span()).into(),
        };
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
                            Decl::Spread(s.span()).into()
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
                                Decl::Spread(s.span()).into()
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

        let (src, val) = self.check_module_path(source);

        // Prefetch Type Check Information
        if let Some(f) = src {
            self.ctx.prefetch_type_check(f);
            self.import_buffer.push(f);
        }

        let decl = match (typed.new_name(), src) {
            (Some(ident), _) => Some(Decl::module(ident)),
            (None, Some(fid)) if typed.imports().is_none() => {
                let i = fid.vpath().as_rooted_path();
                let name = i.file_stem().and_then(|s| s.to_str());
                // Some(self.alloc_path_end(s))
                name.map(|name| Decl::path_stem(source.to_untyped().clone(), name))
            }
            _ => None,
        };
        if let Some(decl) = &decl {
            self.scope_mut()
                .insert_mut(decl.name().clone(), decl.clone().into());
        }
        let decl = decl
            .unwrap_or_else(|| Decl::ModuleImport(typed.span()))
            .into();
        self.resolve_as(Decl::as_def(
            &decl,
            val.clone().map(|val| Ty::Value(InsTy::new(val))),
        ));

        let pattern;

        if let Some(imports) = typed.imports() {
            match imports {
                ast::Imports::Wildcard => {
                    log::debug!("checking wildcard: {val:?}");
                    match val {
                        Some(Value::Module(m)) => {
                            self.lexical.scopes.push(ExprScope::Module(m));
                        }
                        Some(Value::Func(f)) => {
                            if f.scope().is_some() {
                                self.lexical.scopes.push(ExprScope::Func(f));
                            }
                        }
                        Some(Value::Type(s)) => {
                            self.lexical.scopes.push(ExprScope::Type(s));
                        }
                        Some(_) => {}
                        None if src.is_some() => {
                            let source = self.ctx.source_by_id(src.unwrap());
                            if let Ok(source) = source {
                                self.lexical
                                    .scopes
                                    .push(ExprScope::Lexical(self.ctx.exports_of(source)));
                            }
                        }
                        None => {
                            log::debug!(
                                "cannot analyze import on: {typed:?}, in file {:?}",
                                typed.span().id()
                            );
                        }
                    }

                    pattern = Expr::Star;
                }
                ast::Imports::Items(items) => {
                    let mut imported = eco_vec![];
                    let module = Expr::Decl(decl.clone());

                    for item in items.iter() {
                        let (path, old, rename) = match item {
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

                        let mut sel = module.clone();
                        for seg in path.iter() {
                            let seg = Decl::ident_ref(seg).into();
                            sel = Expr::Select(SelectExpr::new(seg, sel));
                        }

                        self.resolve_as(
                            RefExpr {
                                ident: old.clone(),
                                of: Some(sel),
                                val: None,
                            }
                            .into(),
                        );

                        if let Some(new) = &rename {
                            let rename_ref = RefExpr {
                                ident: new.clone(),
                                of: Some(Expr::Decl(old.clone())),
                                val: None,
                            };
                            self.resolve_as(rename_ref.into());
                        }

                        let new = rename.unwrap_or_else(|| old.clone());
                        let new_name = new.name().clone();
                        let new_expr = Expr::Decl(new);
                        self.scope_mut().insert_mut(new_name, new_expr.clone());
                        imported.push((old, new_expr));
                    }

                    pattern = Expr::Pattern(
                        Pattern {
                            pos: eco_vec![],
                            named: imported,
                            spread_left: None,
                            spread_right: None,
                        }
                        .into(),
                    );
                }
            }
        } else {
            pattern = none_expr();
        };

        Expr::Import(ImportExpr { decl, pattern }.into())
    }

    fn check_module_path(&mut self, source: ast::Expr) -> (Option<TypstFileId>, Option<Value>) {
        match source {
            // todo: analyze ident import
            ast::Expr::Str(s) => {
                let id = resolve_id_by_path(&self.ctx.world, self.fid, s.get().as_str());
                (id, None)
            }
            _source => {
                let (src, val) = self.ctx.analyze_import(source.to_untyped());
                let fid = match (src, val.as_ref()) {
                    (_, Some(Value::Module(m))) => m.file_id(),
                    (Some(Value::Str(m)), _) => {
                        resolve_id_by_path(&self.ctx.world, self.fid, m.as_str())
                    }
                    _ => None,
                };
                (fid, val)
            }
        }
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
                    let key = item.key().to_untyped();
                    let analyzed = self.ctx.analyze_expr2(key);
                    let analyzed = analyzed.iter().find_map(|(s, _)| match s {
                        Value::Str(s) => Some(s),
                        _ => None,
                    });
                    let Some(analyzed) = analyzed else {
                        continue;
                    };
                    let key = Decl::str_name(key.clone(), analyzed).into();
                    let val = self.check(item.expr());
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

    fn check_markup(&mut self, m: ast::Markup) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.exprs(), InterpretMode::Markup))
    }

    fn check_code(&mut self, m: ast::Code) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.exprs(), InterpretMode::Code))
    }

    fn check_math(&mut self, m: ast::Math) -> Expr {
        self.with_scope(|this| this.check_in_mode(m.exprs(), InterpretMode::Math))
    }

    fn check_in_mode<'b>(
        &mut self,
        root: impl Iterator<Item = ast::Expr<'b>>,
        mode: InterpretMode,
    ) -> Expr {
        let old_mode = self.lexical.mode;
        self.lexical.mode = mode;
        let children = root
            .filter(|e| !matches!(e, ast::Expr::Space(..)))
            .map(|b| self.check(b))
            .collect::<Vec<_>>();
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
        self.resolve_ident(Decl::math_ident_ref(ident).into(), InterpretMode::Code)
    }

    fn resolve_as(&mut self, r: Interned<RefExpr>) {
        let s = r.ident.span().unwrap();
        self.buffer.push((s, r.clone()));
    }

    fn resolve_ident(&mut self, decl: DeclExpr, code: InterpretMode) -> Expr {
        let r: Interned<RefExpr> = self.resolve_ident_(decl, code).into();
        let s = r.ident.span().unwrap();
        self.buffer.push((s, r.clone()));
        Expr::Ref(r)
    }

    fn resolve_ident_(&mut self, decl: DeclExpr, code: InterpretMode) -> RefExpr {
        let name = decl.name().clone();

        let mut ref_expr = RefExpr {
            ident: decl,
            of: None,
            val: None,
        };
        for scope in self.lexical.scopes.iter().rev() {
            let (of, val) = match scope {
                ExprScope::Lexical(scope) => {
                    if let Some(of) = scope.get(&name) {
                        (Some(of.clone()), None)
                    } else {
                        continue;
                    }
                }
                ExprScope::Module(module) => {
                    let v = module.scope().get(&name);
                    if v.is_none() {
                        continue;
                    }

                    let decl = v
                        .and_then(|_| Some(Decl::external(module.file_id()?, name.clone()).into()));

                    (decl, v)
                }
                ExprScope::Func(func) => {
                    let v = func.scope().unwrap().get(&name);
                    if v.is_none() {
                        continue;
                    }
                    (None, v)
                }
                ExprScope::Type(ty) => {
                    let v = ty.scope().get(&name);
                    if v.is_none() {
                        continue;
                    }
                    (None, v)
                }
            };

            ref_expr.of = of.clone();
            ref_expr.val = val.map(|v| Ty::Value(InsTy::new(v.clone())));
            return ref_expr;
        }

        let scope = match code {
            InterpretMode::Math => self.ctx.world.library.math.scope(),
            InterpretMode::Markup | InterpretMode::Code => self.ctx.world.library.global.scope(),
            _ => return ref_expr,
        };

        let val = scope.get(&name);
        ref_expr.val = val.map(|v| Ty::Value(InsTy::new(v.clone())));
        ref_expr
    }

    fn defer(&mut self, expr: ast::Expr) -> DeferExpr {
        let expr = expr.to_untyped().clone();
        let span = expr.span();

        let new = self;
        let ret = new.check(expr.cast().unwrap());
        new.scopes.lock().insert(expr.span(), ret);

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
}

fn select_of(source: Interned<Ty>, name: Interned<str>) -> Expr {
    Expr::Type(Ty::Select(SelectTy::new(source, name)))
}

fn none_expr() -> Expr {
    Expr::Type(Ty::Builtin(BuiltinTy::None))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DefKind {
    Export,
    Func,
    ImportAlias,
    Constant,
    Var,
    BibKey,
    IdentRef,
    Module,
    Import,
    Label,
    Ref,
    StrName,
    PathStem,
    ModuleImport,
    ModuleInclude,
    Spread,
}

pub type DeclExpr = Interned<Decl>;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Decl {
    Export {
        name: Interned<str>,
        fid: TypstFileId,
    },
    Func {
        name: Interned<str>,
        at: Span,
    },
    ImportAlias {
        name: Interned<str>,
        at: Span,
    },
    Var {
        name: Interned<str>,
        at: Span,
    },
    IdentRef {
        name: Interned<str>,
        at: Span,
    },
    Module {
        name: Interned<str>,
        at: Span,
    },
    Import {
        name: Interned<str>,
        at: Span,
    },
    Ref {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    Label {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    StrName {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    PathStem {
        name: Interned<str>,
        at: Box<SyntaxNode>,
    },
    ModuleImport(Span),
    Closure(Span),
    Spread(Span),
}

impl Decl {
    pub fn external(fid: TypstFileId, name: Interned<str>) -> Self {
        Self::Export { fid, name }
    }

    pub fn func(ident: ast::Ident) -> Self {
        Self::Func {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn var(ident: ast::Ident) -> Self {
        Self::Var {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn import_alias(ident: ast::Ident) -> Self {
        Self::ImportAlias {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn ident_ref(ident: ast::Ident) -> Self {
        Self::IdentRef {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn math_ident_ref(ident: ast::MathIdent) -> Self {
        Self::IdentRef {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn module(ident: ast::Ident) -> Self {
        Self::Module {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn import(ident: ast::Ident) -> Self {
        Self::Import {
            name: ident.get().into(),
            at: ident.span(),
        }
    }

    pub fn label(ident: ast::Label) -> Self {
        Self::Label {
            name: ident.get().into(),
            at: Box::new(ident.to_untyped().clone()),
        }
    }

    pub fn ref_(ident: ast::Ref) -> Self {
        Self::Ref {
            name: ident.target().into(),
            at: Box::new(ident.to_untyped().clone()),
        }
    }

    fn str_name(s: SyntaxNode, name: &str) -> Decl {
        Self::StrName {
            name: name.into(),
            at: Box::new(s),
        }
    }

    pub fn path_stem(s: SyntaxNode, name: &str) -> Self {
        Self::PathStem {
            name: name.into(),
            at: Box::new(s),
        }
    }

    pub fn name(&self) -> &Interned<str> {
        match self {
            Decl::Export { name, .. }
            | Decl::Func { name, .. }
            | Decl::Var { name, .. }
            | Decl::ImportAlias { name, .. }
            | Decl::IdentRef { name, .. }
            | Decl::Module { name, .. }
            | Decl::Import { name, .. }
            | Decl::Label { name, .. }
            | Decl::Ref { name, .. }
            | Decl::StrName { name, .. }
            | Decl::PathStem { name, .. } => name,
            Decl::ModuleImport(..) | Decl::Closure(..) | Decl::Spread(..) => Interned::empty(),
        }
    }

    pub fn kind(&self) -> DefKind {
        match self {
            Decl::Export { .. } => DefKind::Export,
            Decl::Func { .. } => DefKind::Func,
            Decl::Closure(..) => DefKind::Func,
            Decl::ImportAlias { .. } => DefKind::ImportAlias,
            Decl::Var { .. } => DefKind::Var,
            Decl::IdentRef { .. } => DefKind::IdentRef,
            Decl::Module { .. } => DefKind::Module,
            Decl::Import { .. } => DefKind::Import,
            Decl::Label { .. } => DefKind::Label,
            Decl::Ref { .. } => DefKind::Ref,
            Decl::StrName { .. } => DefKind::StrName,
            Decl::PathStem { .. } => DefKind::PathStem,
            Decl::ModuleImport(..) => DefKind::ModuleImport,
            Decl::Spread(..) => DefKind::Spread,
        }
    }

    pub fn file_id(&self) -> Option<TypstFileId> {
        match self {
            Decl::Export { fid, .. } => Some(*fid),
            that => that.span()?.id(),
        }
    }

    pub fn span(&self) -> Option<Span> {
        match self {
            Decl::Export { .. } => None,
            Decl::ModuleImport(at)
            | Decl::Closure(at)
            | Decl::Spread(at)
            | Decl::Func { at, .. }
            | Decl::Var { at, .. }
            | Decl::ImportAlias { at, .. }
            | Decl::IdentRef { at, .. }
            | Decl::Module { at, .. }
            | Decl::Import { at, .. } => Some(*at),
            Decl::Label { at, .. }
            | Decl::Ref { at, .. }
            | Decl::StrName { at, .. }
            | Decl::PathStem { at, .. } => Some(at.span()),
        }
    }

    fn as_def(this: &Interned<Self>, val: Option<Ty>) -> Interned<RefExpr> {
        Interned::new(RefExpr {
            ident: this.clone(),
            of: Some(this.clone().into()),
            val,
        })
    }
}

impl From<Decl> for Expr {
    fn from(decl: Decl) -> Self {
        Expr::Decl(decl.into())
    }
}

impl From<DeclExpr> for Expr {
    fn from(decl: DeclExpr) -> Self {
        Expr::Decl(decl)
    }
}

impl fmt::Debug for Decl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Decl::Ident(ident) => write!(f, "Ident({:?})", ident.name),
            Decl::Export { name, fid } => write!(f, "Export({name:?}, {fid:?})"),
            Decl::Func { name, .. } => write!(f, "Func({name:?})"),
            Decl::Var { name, .. } => write!(f, "Var({name:?})"),
            Decl::ImportAlias { name, .. } => write!(f, "ImportAlias({name:?})"),
            Decl::IdentRef { name, .. } => write!(f, "IdentRef({name:?})"),
            Decl::Module { name, .. } => write!(f, "Module({name:?})"),
            Decl::Import { name, .. } => write!(f, "Import({name:?})"),
            Decl::Label { name, .. } => write!(f, "Label({name:?})"),
            Decl::Ref { name, .. } => write!(f, "Ref({name:?})"),
            Decl::StrName { name, at } => write!(f, "StrName({name:?}, {at:?})"),
            Decl::PathStem { name, at } => write!(f, "PathStem({name:?}, {at:?})"),
            Decl::ModuleImport(..) => write!(f, "ModuleImport(..)"),
            Decl::Closure(..) => write!(f, "Closure(..)"),
            Decl::Spread(..) => write!(f, "Spread(..)"),
        }
    }
}

pub type UnExpr = UnInst<Expr>;
pub type BinExpr = BinInst<Expr>;

pub type ExportMap = BTreeMap<Interned<str>, Expr>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArgExpr {
    Pos(Expr),
    Named(Box<(DeclExpr, Expr)>),
    Spread(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern {
    pub pos: EcoVec<Expr>,
    pub named: EcoVec<(DeclExpr, Expr)>,
    pub spread_left: Option<(DeclExpr, Expr)>,
    pub spread_right: Option<(DeclExpr, Expr)>,
}

impl Pattern {}

impl_internable!(Decl,);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentSeqExpr {
    pub ty: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefExpr {
    pub ident: DeclExpr,
    pub of: Option<Expr>,
    pub val: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentRefExpr {
    pub ident: DeclExpr,
    pub of: Option<DeclExpr>,
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectExpr {
    pub lhs: Expr,
    pub key: DeclExpr,
}

impl SelectExpr {
    pub fn new(key: DeclExpr, lhs: Expr) -> Interned<Self> {
        Interned::new(Self { key, lhs })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeferExpr {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementExpr {
    pub elem: Element,
    pub content: EcoVec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyExpr {
    pub callee: Expr,
    pub args: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncExpr {
    pub decl: DeclExpr,
    pub params: Interned<Pattern>,
    pub body: DeferExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LetExpr {
    pub pattern: Expr,
    pub body: Option<DeferExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowExpr {
    pub selector: Option<Expr>,
    pub edit: DeferExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SetExpr {
    pub target: Expr,
    pub args: Expr,
    pub cond: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportExpr {
    pub decl: DeclExpr,
    pub pattern: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludeExpr {
    pub source: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfExpr {
    pub cond: Expr,
    pub then: Expr,
    pub else_: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhileExpr {
    pub cond: Expr,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForExpr {
    pub pattern: Expr,
    pub iter: Expr,
    pub body: Expr,
}

/// The kind of unary operation
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum UnaryOp {
    /// The (arithmetic) positive operation
    /// `+t`
    Pos,
    /// The (arithmetic) negate operation
    /// `-t`
    Neg,
    /// The (logical) not operation
    /// `not t`
    Not,
    /// The return operation
    /// `return t`
    Return,
    /// The typst context operation
    /// `context t`
    Context,
    /// The spreading operation
    /// `..t`
    Spread,
    /// The not element of operation
    /// `not in t`
    NotElementOf,
    /// The element of operation
    /// `in t`
    ElementOf,
    /// The type of operation
    /// `type(t)`
    TypeOf,
}

/// A unary operation type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct UnInst<T> {
    /// The operand of the unary operation
    pub lhs: T,
    /// The kind of the unary operation
    pub op: UnaryOp,
}

impl<T: Ord> PartialOrd for UnInst<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for UnInst<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let op_as_int = self.op as u8;
        let other_op_as_int = other.op as u8;
        op_as_int
            .cmp(&other_op_as_int)
            .then_with(|| self.lhs.cmp(&other.lhs))
    }
}

impl UnInst<Expr> {
    /// Create a unary operation type
    pub fn new(op: UnaryOp, lhs: Expr) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }
}

impl<T> UnInst<T> {
    /// Get the operands of the unary operation
    pub fn operands(&self) -> [&T; 1] {
        [&self.lhs]
    }
}

/// The kind of binary operation
pub type BinaryOp = ast::BinOp;

/// A binary operation type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct BinInst<T> {
    /// The operands of the binary operation
    pub operands: (T, T),
    /// The kind of the binary operation
    pub op: BinaryOp,
}

impl<T: Ord> PartialOrd for BinInst<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for BinInst<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let op_as_int = self.op as u8;
        let other_op_as_int = other.op as u8;
        op_as_int
            .cmp(&other_op_as_int)
            .then_with(|| self.operands.cmp(&other.operands))
    }
}

impl BinInst<Expr> {
    /// Create a binary operation type
    pub fn new(op: BinaryOp, lhs: Expr, rhs: Expr) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }
}

impl<T> BinInst<T> {
    /// Get the operands of the binary operation
    pub fn operands(&self) -> [&T; 2] {
        [&self.operands.0, &self.operands.1]
    }
}

impl_internable!(
    Expr,
    ElementExpr,
    ContentSeqExpr,
    RefExpr,
    ContentRefExpr,
    SelectExpr,
    ImportExpr,
    IncludeExpr,
    IfExpr,
    WhileExpr,
    ForExpr,
    FuncExpr,
    LetExpr,
    ShowExpr,
    SetExpr,
    Pattern,
    EcoVec<(Decl, Expr)>,
    Vec<ArgExpr>,
    Vec<Expr>,
    UnInst<Expr>,
    BinInst<Expr>,
    ApplyExpr,
);

struct ExprFormatter<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    indent: usize,
}

impl<'a, 'b> ExprFormatter<'a, 'b> {
    fn new(f: &'a mut fmt::Formatter<'b>) -> Self {
        Self { f, indent: 0 }
    }

    fn write_decl(&mut self, d: &Decl) -> fmt::Result {
        write!(self.f, "{d:?}")
    }

    fn write_expr(&mut self, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Seq(s) => self.write_seq(s),
            Expr::Array(a) => self.write_array(a),
            Expr::Dict(d) => self.write_dict(d),
            Expr::Args(a) => self.write_args(a),
            Expr::Pattern(p) => self.write_pattern(p),
            Expr::Element(e) => self.write_element(e),
            Expr::Unary(u) => self.write_unary(u),
            Expr::Binary(b) => self.write_binary(b),
            Expr::Apply(a) => self.write_apply(a),
            Expr::Func(func) => self.write_func(func),
            Expr::Let(l) => self.write_let(l),
            Expr::Show(s) => self.write_show(s),
            Expr::Set(s) => self.write_set(s),
            Expr::Ref(r) => self.write_ref(r),
            Expr::ContentRef(r) => self.write_content_ref(r),
            Expr::Select(s) => self.write_select(s),
            Expr::Import(i) => self.write_import(i),
            Expr::Include(i) => self.write_include(i),
            Expr::Contextual(c) => self.write_contextual(c),
            Expr::Conditional(c) => self.write_conditional(c),
            Expr::WhileLoop(w) => self.write_while_loop(w),
            Expr::ForLoop(f) => self.write_for_loop(f),
            Expr::Type(t) => self.write_type(t),
            Expr::Decl(d) => self.write_decl(d),
            Expr::Star => self.write_star(),
        }
    }

    fn write_indent(&mut self) -> fmt::Result {
        write!(self.f, "{:indent$}", "", indent = self.indent)
    }

    fn write_seq(&mut self, s: &Interned<Vec<Expr>>) -> fmt::Result {
        writeln!(self.f, "[")?;
        self.indent += 1;
        for expr in s.iter() {
            self.write_indent()?;
            self.write_expr(expr)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, "]")
    }

    fn write_array(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        self.indent += 1;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_dict(&mut self, d: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(:")?;
        self.indent += 1;
        for arg in d.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_args(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_arg(&mut self, a: &ArgExpr) -> fmt::Result {
        match a {
            ArgExpr::Pos(e) => self.write_expr(e),
            ArgExpr::Named(n) => {
                let n = n.as_ref();
                write!(self.f, "{n:?}: ")?;
                self.write_expr(&n.1)
            }
            ArgExpr::Spread(e) => {
                write!(self.f, "..")?;
                self.write_expr(e)
            }
        }
    }

    fn write_pattern(&mut self, p: &Interned<Pattern>) -> fmt::Result {
        self.f.write_str("pat(\n")?;
        self.indent += 1;
        for pos in &p.pos {
            self.write_indent()?;
            self.write_expr(pos)?;
            self.f.write_str(",\n")?;
        }
        for (name, pat) in &p.named {
            self.write_indent()?;
            write!(self.f, "{name:?} = ")?;
            self.write_expr(pat)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_left {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_expr(rest)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_right {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_expr(rest)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_element(&mut self, e: &Interned<ElementExpr>) -> fmt::Result {
        self.f.write_str("elem(\n")?;
        self.indent += 1;
        for v in &e.content {
            self.write_indent()?;
            self.write_expr(v)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_unary(&mut self, u: &Interned<UnExpr>) -> fmt::Result {
        write!(self.f, "un({:?})(", u.op)?;
        self.write_expr(&u.lhs)?;
        self.f.write_str(")")
    }

    fn write_binary(&mut self, b: &Interned<BinExpr>) -> fmt::Result {
        let [lhs, rhs] = b.operands();
        write!(self.f, "bin({:?})(", b.op)?;
        self.write_expr(lhs)?;
        self.f.write_str(", ")?;
        self.write_expr(rhs)?;
        self.f.write_str(")")
    }

    fn write_apply(&mut self, a: &Interned<ApplyExpr>) -> fmt::Result {
        write!(self.f, "apply(")?;
        self.write_expr(&a.callee)?;
        self.f.write_str(", ")?;
        self.write_expr(&a.args)?;
        write!(self.f, ")")
    }

    fn write_func(&mut self, func: &Interned<FuncExpr>) -> fmt::Result {
        write!(self.f, "func[{:?}](", func.decl)?;
        self.write_pattern(&func.params)?;
        write!(self.f, " = {:?})", func.body.span)
    }

    fn write_let(&mut self, l: &Interned<LetExpr>) -> fmt::Result {
        write!(self.f, "let(")?;
        self.write_expr(&l.pattern)?;
        if let Some(body) = &l.body {
            write!(self.f, " = {:?}", body.span)?;
        }
        write!(self.f, ")")
    }

    fn write_show(&mut self, s: &Interned<ShowExpr>) -> fmt::Result {
        write!(self.f, "show(")?;
        if let Some(selector) = &s.selector {
            self.write_expr(selector)?;
            self.f.write_str(", ")?;
        }
        write!(self.f, "{:?})", s.edit.span)
    }

    fn write_set(&mut self, s: &Interned<SetExpr>) -> fmt::Result {
        write!(self.f, "set(")?;
        self.write_expr(&s.target)?;
        self.f.write_str(", ")?;
        self.write_expr(&s.args)?;
        if let Some(cond) = &s.cond {
            self.f.write_str(", ")?;
            self.write_expr(cond)?;
        }
        write!(self.f, ")")
    }

    fn write_ref(&mut self, r: &Interned<RefExpr>) -> fmt::Result {
        write!(self.f, "ref({:?}", r.ident)?;
        if let Some(of) = &r.of {
            self.f.write_str(", ")?;
            self.write_expr(of)?;
        }
        if let Some(val) = &r.val {
            write!(self.f, ", val = {val:?}")?;
        }
        self.f.write_str(")")
    }

    fn write_content_ref(&mut self, r: &Interned<ContentRefExpr>) -> fmt::Result {
        write!(self.f, "content_ref({:?}", r.ident)?;
        if let Some(of) = &r.of {
            self.f.write_str(", ")?;
            self.write_decl(of)?;
        }
        if let Some(val) = &r.body {
            self.write_expr(val)?;
        }
        self.f.write_str(")")
    }

    fn write_select(&mut self, s: &Interned<SelectExpr>) -> fmt::Result {
        write!(self.f, "(")?;
        self.write_expr(&s.lhs)?;
        self.f.write_str(").")?;
        self.write_decl(&s.key)
    }

    fn write_import(&mut self, i: &Interned<ImportExpr>) -> fmt::Result {
        self.f.write_str("import(")?;
        self.write_decl(&i.decl)?;
        self.write_expr(&i.pattern)?;
        self.f.write_str(")")
    }

    fn write_include(&mut self, i: &Interned<IncludeExpr>) -> fmt::Result {
        self.f.write_str("include(")?;
        self.write_expr(&i.source)?;
        self.f.write_str(")")
    }

    fn write_contextual(&mut self, c: &Interned<Expr>) -> fmt::Result {
        self.f.write_str("contextual(")?;
        self.write_expr(c)?;
        self.f.write_str(")")
    }

    fn write_conditional(&mut self, c: &Interned<IfExpr>) -> fmt::Result {
        self.f.write_str("if(")?;
        self.write_expr(&c.cond)?;
        self.f.write_str(", then = ")?;
        self.write_expr(&c.then)?;
        self.f.write_str(", else = ")?;
        self.write_expr(&c.else_)?;
        self.f.write_str(")")
    }

    fn write_while_loop(&mut self, w: &Interned<WhileExpr>) -> fmt::Result {
        self.f.write_str("while(")?;
        self.write_expr(&w.cond)?;
        self.f.write_str(", ")?;
        self.write_expr(&w.body)?;
        self.f.write_str(")")
    }

    fn write_for_loop(&mut self, f: &Interned<ForExpr>) -> fmt::Result {
        self.f.write_str("for(")?;
        self.write_expr(&f.pattern)?;
        self.f.write_str(", ")?;
        self.write_expr(&f.iter)?;
        self.f.write_str(", ")?;
        self.write_expr(&f.body)?;
        self.f.write_str(")")
    }

    fn write_type(&mut self, t: &Ty) -> fmt::Result {
        write!(self.f, "{t:?}")
    }

    fn write_star(&mut self) -> fmt::Result {
        self.f.write_str("*")
    }
}
