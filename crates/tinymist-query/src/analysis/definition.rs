//! Linked definition analysis

use typst::foundations::{IntoValue, Label, Selector, Type};
use typst::introspection::Introspector;
use typst::model::BibliographyElem;

use super::{prelude::*, InsTy, SharedContext};
use crate::syntax::{Decl, DeclExpr, DerefTarget, Expr, ExprInfo};
use crate::ty::DocSource;
use crate::VersionedDocument;

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
    pub(crate) fn def_at(&self, ctx: &SharedContext) -> Option<(TypstFileId, Range<usize>)> {
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
    document: Option<&VersionedDocument>,
    deref_target: DerefTarget,
) -> Option<Definition> {
    match deref_target {
        // todi: field access
        DerefTarget::VarAccess(node) | DerefTarget::Callee(node) => {
            find_ident_definition(ctx, source, node)
        }
        DerefTarget::ImportPath(path) | DerefTarget::IncludePath(path) => {
            DefResolver::new(ctx, source)?.of_span(path.span())
        }
        DerefTarget::Label(r) | DerefTarget::Ref(r) => {
            let ref_expr: ast::Expr = r.cast()?;
            let name = match ref_expr {
                ast::Expr::Ref(r) => r.target(),
                ast::Expr::Label(r) => r.get(),
                _ => return None,
            };

            let introspector = &document?.document.introspector;
            find_bib_definition(ctx, introspector, name)
                .or_else(|| find_ref_definition(introspector, name, ref_expr))
        }
        DerefTarget::Normal(..) => None,
    }
}

fn find_ident_definition(
    ctx: &Arc<SharedContext>,
    source: &Source,
    use_site: LinkedNode,
) -> Option<Definition> {
    // Lexical reference
    let ident_store = use_site.clone();
    let ident_ref = match ident_store.cast::<ast::Expr>()? {
        ast::Expr::Ident(e) => e.span(),
        ast::Expr::MathIdent(e) => e.span(),
        ast::Expr::FieldAccess(s) => return find_field_definition(ctx, s),
        _ => {
            log::debug!("unsupported kind {kind:?}", kind = use_site.kind());
            Span::detached()
        }
    };

    DefResolver::new(ctx, source)?.of_span(ident_ref)
}

fn find_field_definition(ctx: &Arc<SharedContext>, fa: ast::FieldAccess<'_>) -> Option<Definition> {
    let span = fa.span();
    let ty = ctx.type_of_span(span)?;
    log::debug!("find_field_definition[{span:?}]: {ty:?}");

    // todo multiple sources
    let mut srcs = ty.sources();
    srcs.sort();
    log::debug!("check type signature of ty: {ty:?} => {srcs:?}");
    let type_var = srcs.into_iter().next()?;
    match type_var {
        DocSource::Var(v) => {
            log::debug!("field var: {:?} {:?}", v.def, v.def.span());
            Some(Definition::new(v.def.clone(), None))
        }
        DocSource::Ins(v) if !v.span().is_detached() => {
            let s = v.span();
            let source = ctx.source_by_id(s.id()?).ok()?;
            DefResolver::new(ctx, &source)?.of_span(s)
        }
        DocSource::Builtin(..) | DocSource::Ins(..) => None,
    }
}

fn find_bib_definition(
    ctx: &Arc<SharedContext>,
    introspector: &Introspector,
    key: &str,
) -> Option<Definition> {
    let bib_elem = BibliographyElem::find(introspector.track()).ok()?;
    let Value::Array(arr) = bib_elem.path().clone().into_value() else {
        return None;
    };

    let bib_paths = arr.into_iter().map(Value::cast).flat_map(|e| e.ok());
    let bib_info = ctx.analyze_bib(bib_elem.span(), bib_paths)?;

    let entry = bib_info.entries.get(key)?;
    log::debug!("find_bib_definition: {key} => {entry:?}");

    // todo: rename with regard to string format: yaml-key/bib etc.
    let decl = Decl::bib_entry(key.into(), entry.file_id, entry.span.clone());
    Some(Definition::new(decl.into(), None))
}

fn find_ref_definition(
    introspector: &Introspector,
    name: &str,
    ref_expr: ast::Expr,
) -> Option<Definition> {
    let label = Label::new(name);
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
            CallConvention::Static(f) => f,
            CallConvention::Method(_, f) => f,
            CallConvention::With(f) => f,
            CallConvention::Where(f) => f,
        }
    }
}

/// Resolve a call target to a function or a method with a this.
pub fn resolve_call_target(ctx: &Arc<SharedContext>, node: &SyntaxNode) -> Option<CallConvention> {
    let callee = (|| {
        let source = ctx.source_by_id(node.span().id()?).ok()?;
        let def = ctx.def_of_span(&source, None, node.span())?;
        let func_ptr = match def.term.and_then(|val| val.value()) {
            Some(Value::Func(f)) => Some(f),
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
                if let Some(Value::Func(f)) = this.ty().scope().get(field) {
                    return Some((this, f.clone()));
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
    let Some(Value::Func(f)) = fn_ty.scope().get("with") else {
        return None;
    };
    Some(f)
});

static WHERE_FUNC: LazyLock<Option<&'static Func>> = LazyLock::new(|| {
    let fn_ty = Type::of::<Func>();
    let Some(Value::Func(f)) = fn_ty.scope().get("where") else {
        return None;
    };
    Some(f)
});

fn value_to_def(
    value: Value,
    name: impl FnOnce() -> Option<Interned<str>>,
    name_range: Option<Range<usize>>,
) -> Option<Definition> {
    let val = Ty::Value(InsTy::new(value.clone()));
    // DefKind::Closure | DefKind::Func => {
    // let value = def_fid.and_then(|fid| {
    //     let def_source = ctx.source_by_id(fid).ok()?;
    //     let root = LinkedNode::new(def_source.root());
    //     let def_name = root.find(def?.span()?)?;

    //     log::info!("def_name for function: {def_name:?}");
    //     let values = ctx.analyze_expr(def_name.get());
    //     let func = values
    //         .into_iter()
    //         .find(|v| matches!(v.0, Value::Func(..)))?;
    //     log::info!("okay for function: {func:?}");
    //     Some(func.0)
    // });

    Some(match value {
        Value::Func(func) => {
            let name = func.name().map(|e| e.into()).or_else(name)?;
            let mut s = SyntaxNode::leaf(SyntaxKind::Ident, &name);
            s.synthesize(func.span());

            let decl = Decl::func(s.cast().unwrap());
            Definition::new(decl.into(), Some(val))
        }
        Value::Module(module) => Definition::new_var(module.name().into(), val),
        _v => {
            // todo name_range
            let _ = name_range;
            Definition::new_var(name()?, val)
        }
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

        let expr = self.ei.resolves.get(&span).cloned()?;
        match (&expr.root, &expr.val) {
            (Some(expr), ty) => self.of_expr(expr, ty.as_ref()),
            (None, Some(term)) => self.of_term(term),
            (None, None) => None,
        }
    }

    fn of_expr(&mut self, expr: &Expr, term: Option<&Ty>) -> Option<Definition> {
        log::debug!("of_expr: {expr:?}");

        match expr {
            Expr::Decl(decl) => self.of_decl(decl, term),
            Expr::Ref(r) => self.of_expr(r.root.as_ref()?, r.val.as_ref().or(term)),
            _ => None,
        }
    }

    fn of_term(&mut self, term: &Ty) -> Option<Definition> {
        log::debug!("of_term: {term:?}");

        // Get the type of the type node
        let better_def = match term {
            Ty::Value(v) => value_to_def(v.val.clone(), || None, None),
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
        log::debug!("of_decl: {decl:?}");

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
