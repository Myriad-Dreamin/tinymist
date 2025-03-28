//! Linked definition analysis

use tinymist_std::typst::TypstDocument;
use typst::foundations::{IntoValue, Label, Selector, Type};
use typst::introspection::Introspector;
use typst::model::BibliographyElem;

use super::{prelude::*, InsTy, SharedContext};
use crate::syntax::{Decl, DeclExpr, Expr, ExprInfo, SyntaxClass, VarClass};
use crate::ty::DocSource;

/// A linked definition in the source code
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Definition {
    /// The declaration identifier of the definition.
    pub decl: DeclExpr,
    /// A possible instance of the definition.
    pub term: Option<Ty>,
}

impl Definition {
    /// Creates a definition
    pub fn new(decl: DeclExpr, term: Option<Ty>) -> Self {
        Self { decl, term }
    }

    /// Creates a definition according to some term
    pub fn new_var(name: Interned<str>, term: Ty) -> Self {
        let decl = Decl::lit_(name);
        Self::new(decl.into(), Some(term))
    }

    /// The name of the definition.
    pub fn name(&self) -> &Interned<str> {
        self.decl.name()
    }

    /// The location of the definition.
    // todo: cache
    pub(crate) fn location(&self, ctx: &SharedContext) -> Option<(TypstFileId, Range<usize>)> {
        let fid = self.decl.file_id()?;
        let span = self.decl.span();
        let range = (!span.is_detached()).then(|| ctx.source_by_id(fid).ok()?.range(span));
        Some((fid, range.flatten().unwrap_or_default()))
    }

    /// The range of the name of the definition.
    pub fn name_range(&self, ctx: &SharedContext) -> Option<Range<usize>> {
        self.decl.name_range(ctx)
    }

    pub(crate) fn value(&self) -> Option<Value> {
        self.term.as_ref()?.value()
    }
}

// todo: field definition
/// Finds the definition of a symbol.
pub fn definition(
    ctx: &Arc<SharedContext>,
    source: &Source,
    document: Option<&TypstDocument>,
    syntax: SyntaxClass,
) -> Option<Definition> {
    match syntax {
        // todo: field access
        SyntaxClass::VarAccess(node) => find_ident_definition(ctx, source, node),
        SyntaxClass::Callee(node) => find_ident_definition(ctx, source, VarClass::Ident(node)),
        SyntaxClass::ImportPath(path) | SyntaxClass::IncludePath(path) => {
            DefResolver::new(ctx, source)?.of_span(path.span())
        }
        SyntaxClass::Label {
            node: r,
            is_error: false,
        }
        | SyntaxClass::Ref(r) => {
            let ref_expr: ast::Expr = r.cast()?;
            let name = match ref_expr {
                ast::Expr::Ref(r) => r.target(),
                ast::Expr::Label(r) => r.get(),
                _ => return None,
            };

            let introspector = &document?.introspector();
            bib_definition(ctx, introspector, name)
                .or_else(|| ref_definition(introspector, name, ref_expr))
        }
        SyntaxClass::Label {
            node: _,
            is_error: true,
        }
        | SyntaxClass::Normal(..) => None,
    }
}

fn find_ident_definition(
    ctx: &Arc<SharedContext>,
    source: &Source,
    use_site: VarClass,
) -> Option<Definition> {
    // Lexical reference
    let ident_store = use_site.clone();
    let ident_ref = match ident_store.node().cast::<ast::Expr>()? {
        ast::Expr::Ident(ident) => ident.span(),
        ast::Expr::MathIdent(ident) => ident.span(),
        ast::Expr::FieldAccess(field_access) => return field_definition(ctx, field_access),
        _ => {
            crate::log_debug_ct!("unsupported kind {kind:?}", kind = use_site.node().kind());
            Span::detached()
        }
    };

    DefResolver::new(ctx, source)?.of_span(ident_ref)
}

fn field_definition(ctx: &Arc<SharedContext>, node: ast::FieldAccess) -> Option<Definition> {
    let span = node.span();
    let ty = ctx.type_of_span(span)?;
    crate::log_debug_ct!("find_field_definition[{span:?}]: {ty:?}");

    // todo multiple sources
    let mut srcs = ty.sources();
    srcs.sort();
    crate::log_debug_ct!("check type signature of ty: {ty:?} => {srcs:?}");
    let type_var = srcs.into_iter().next()?;
    match type_var {
        DocSource::Var(v) => {
            crate::log_debug_ct!("field var: {:?} {:?}", v.def, v.def.span());
            Some(Definition::new(v.def.clone(), None))
        }
        DocSource::Ins(v) if !v.span().is_detached() => {
            let s = v.span();
            let source = ctx.source_by_id(s.id()?).ok()?;
            DefResolver::new(ctx, &source)?.of_span(s)
        }
        DocSource::Ins(ins) => value_to_def(ins.val.clone(), || Some(node.field().get().into())),
        DocSource::Builtin(..) => None,
    }
}

fn bib_definition(
    ctx: &Arc<SharedContext>,
    introspector: &Introspector,
    key: &str,
) -> Option<Definition> {
    let bib_elem = BibliographyElem::find(introspector.track()).ok()?;
    let Value::Array(paths) = bib_elem.sources.clone().into_value() else {
        return None;
    };

    let bib_paths = paths.into_iter().flat_map(|path| path.cast().ok());
    let bib_info = ctx.analyze_bib(bib_elem.span(), bib_paths)?;

    let entry = bib_info.entries.get(key)?;
    crate::log_debug_ct!("find_bib_definition: {key} => {entry:?}");

    // todo: rename with regard to string format: yaml-key/bib etc.
    let decl = Decl::bib_entry(key.into(), entry.file_id, entry.range.clone());
    Some(Definition::new(decl.into(), None))
}

fn ref_definition(
    introspector: &Introspector,
    name: &str,
    ref_expr: ast::Expr,
) -> Option<Definition> {
    let label = Label::construct(name.into());
    let sel = Selector::Label(label);

    // if it is a label, we put the selection range to itself
    let (decl, ty) = match ref_expr {
        ast::Expr::Label(label) => (Decl::label(name, label.span()), None),
        ast::Expr::Ref(..) => {
            let elem = introspector.query_first(&sel)?;
            let span = elem.labelled_at();
            let decl = if !span.is_detached() {
                Decl::label(name, span)
            } else {
                // otherwise, it is estimated to the span of the pointed content
                Decl::content(elem.span())
            };
            (decl, Some(Ty::Value(InsTy::new(Value::Content(elem)))))
        }
        _ => return None,
    };

    Some(Definition::new(decl.into(), ty))
}

/// The call of a function with calling convention identified.
#[derive(Debug, Clone)]
pub enum CallConvention {
    /// A static function.
    Static(Func),
    /// A method call with a this.
    Method(Value, Func),
    /// A function call by with binding.
    With(Func),
    /// A function call by where binding.
    Where(Func),
}

impl CallConvention {
    /// Get the function pointer of the call.
    pub fn method_this(&self) -> Option<&Value> {
        match self {
            CallConvention::Static(_) => None,
            CallConvention::Method(this, _) => Some(this),
            CallConvention::With(_) => None,
            CallConvention::Where(_) => None,
        }
    }

    /// Get the function pointer of the call.
    pub fn callee(self) -> Func {
        match self {
            CallConvention::Static(func) => func,
            CallConvention::Method(_, func) => func,
            CallConvention::With(func) => func,
            CallConvention::Where(func) => func,
        }
    }
}

/// Resolve a call target to a function or a method with a this.
pub fn resolve_call_target(ctx: &Arc<SharedContext>, node: &SyntaxNode) -> Option<CallConvention> {
    let callee = (|| {
        let source = ctx.source_by_id(node.span().id()?).ok()?;
        let def = ctx.def_of_span(&source, None, node.span())?;
        let func_ptr = match def.term.and_then(|val| val.value()) {
            Some(Value::Func(func)) => Some(func),
            Some(Value::Type(ty)) => ty.constructor().ok(),
            _ => None,
        }?;

        Some((None, func_ptr))
    })();
    let callee = callee.or_else(|| {
        let values = ctx.analyze_expr(node);

        if let Some(access) = node.cast::<ast::FieldAccess>() {
            let target = access.target();
            let field = access.field().get();
            let values = ctx.analyze_expr(target.to_untyped());
            if let Some((this, func_ptr)) = values.into_iter().find_map(|(this, _styles)| {
                if let Some(Value::Func(func)) = this.ty().scope().get(field).map(|b| b.read()) {
                    return Some((this, func.clone()));
                }

                None
            }) {
                return Some((Some(this), func_ptr));
            }
        }

        if let Some(func) = values.into_iter().find_map(|v| v.0.to_func()) {
            return Some((None, func));
        };

        None
    })?;

    let (this, func_ptr) = callee;
    Some(match this {
        Some(Value::Func(func)) if is_same_native_func(*WITH_FUNC, &func_ptr) => {
            CallConvention::With(func)
        }
        Some(Value::Func(func)) if is_same_native_func(*WHERE_FUNC, &func_ptr) => {
            CallConvention::Where(func)
        }
        Some(this) => CallConvention::Method(this, func_ptr),
        None => CallConvention::Static(func_ptr),
    })
}

fn is_same_native_func(x: Option<&Func>, y: &Func) -> bool {
    let Some(x) = x else {
        return false;
    };

    use typst::foundations::func::Repr;
    match (x.inner(), y.inner()) {
        (Repr::Native(x), Repr::Native(y)) => x == y,
        (Repr::Element(x), Repr::Element(y)) => x == y,
        _ => false,
    }
}

static WITH_FUNC: LazyLock<Option<&'static Func>> = LazyLock::new(|| {
    let fn_ty = Type::of::<Func>();
    let bind = fn_ty.scope().get("with")?;
    let Value::Func(func) = bind.read() else {
        return None;
    };
    Some(func)
});

static WHERE_FUNC: LazyLock<Option<&'static Func>> = LazyLock::new(|| {
    let fn_ty = Type::of::<Func>();
    let bind = fn_ty.scope().get("where")?;
    let Value::Func(func) = bind.read() else {
        return None;
    };
    Some(func)
});

fn value_to_def(value: Value, name: impl FnOnce() -> Option<Interned<str>>) -> Option<Definition> {
    let val = Ty::Value(InsTy::new(value.clone()));
    Some(match value {
        Value::Func(func) => {
            let name = func.name().map(|name| name.into()).or_else(name)?;
            let mut s = SyntaxNode::leaf(SyntaxKind::Ident, &name);
            s.synthesize(func.span());

            let decl = Decl::func(s.cast().unwrap());
            Definition::new(decl.into(), Some(val))
        }
        Value::Module(module) => {
            Definition::new_var(Interned::new_str(module.name().unwrap()), val)
        }
        _v => Definition::new_var(name()?, val),
    })
}

struct DefResolver {
    ei: Arc<ExprInfo>,
}

impl DefResolver {
    fn new(ctx: &Arc<SharedContext>, source: &Source) -> Option<Self> {
        let ei = ctx.expr_stage(source);
        Some(Self { ei })
    }

    fn of_span(&mut self, span: Span) -> Option<Definition> {
        if span.is_detached() {
            return None;
        }

        let resolved = self.ei.resolves.get(&span).cloned()?;
        match (&resolved.root, &resolved.term) {
            (Some(expr), term) => self.of_expr(expr, term.as_ref()),
            (None, Some(term)) => self.of_term(term),
            (None, None) => None,
        }
    }

    fn of_expr(&mut self, expr: &Expr, term: Option<&Ty>) -> Option<Definition> {
        crate::log_debug_ct!("of_expr: {expr:?}");

        match expr {
            Expr::Decl(decl) => self.of_decl(decl, term),
            Expr::Ref(resolved) => {
                self.of_expr(resolved.root.as_ref()?, resolved.term.as_ref().or(term))
            }
            _ => None,
        }
    }

    fn of_term(&mut self, term: &Ty) -> Option<Definition> {
        crate::log_debug_ct!("of_term: {term:?}");

        // Get the type of the type node
        let better_def = match term {
            Ty::Value(v) => value_to_def(v.val.clone(), || None),
            // Ty::Var(..) => DeclKind::Var,
            // Ty::Func(..) => DeclKind::Func,
            // Ty::With(..) => DeclKind::Func,
            _ => None,
        };

        better_def.or_else(|| {
            let constant = Decl::constant(Span::detached());
            Some(Definition::new(constant.into(), Some(term.clone())))
        })
    }

    fn of_decl(&mut self, decl: &Interned<Decl>, term: Option<&Ty>) -> Option<Definition> {
        crate::log_debug_ct!("of_decl: {decl:?}");

        // todo:
        match decl.as_ref() {
            Decl::Import(..) | Decl::ImportAlias(..) => {
                let next = self.of_span(decl.span());
                Some(next.unwrap_or_else(|| Definition::new(decl.clone(), term.cloned())))
            }
            _ => Some(Definition::new(decl.clone(), term.cloned())),
        }
    }
}
