//! Type checking on source file

use typst::foundations::{Element, Type};

use super::*;
use crate::analysis::ParamAttrs;
use crate::docs::{DocString, SignatureDocsT, TypelessParamDocs, UntypedDefDocs, VarDoc};
use crate::syntax::def::*;
use crate::ty::*;

static EMPTY_DOCSTRING: LazyLock<DocString> = LazyLock::new(DocString::default);
static EMPTY_VAR_DOC: LazyLock<VarDoc> = LazyLock::new(VarDoc::default);
const MAX_BINARY_ATOMS: usize = 32;
const MAX_PRECISE_TUPLE_ELEMENTS: usize = 32;

#[derive(Clone, PartialEq, Eq)]
enum BinaryAtom {
    None,
    Boolean(Option<bool>),
    Type(Type),
    Builtin(BuiltinTy),
    Array,
    Dict,
    Args,
    Content,
    Unsupported,
}

#[derive(Default)]
struct BinaryAtoms {
    atoms: Vec<BinaryAtom>,
    unknown: bool,
}

struct BinaryCheck {
    ty: Ty,
    incompatible: bool,
    unknown: bool,
}

enum BinaryPairResult {
    Known(Ty),
    Unknown,
    Incompatible,
}

enum AtCallCheck {
    StaticKey(Ty),
    Default(Ty),
    None,
}

impl AtCallCheck {
    fn default(self) -> Option<Ty> {
        match self {
            Self::Default(ty) => Some(ty),
            Self::StaticKey(_) | Self::None => None,
        }
    }
}

impl BinaryAtoms {
    fn push(&mut self, atom: BinaryAtom) {
        if !self.atoms.contains(&atom) {
            self.atoms.push(atom);
        }
    }
}

impl BinaryAtom {
    fn from_ty(ty: &Ty) -> Self {
        match ty {
            Ty::Boolean(value) => Self::Boolean(*value),
            Ty::Value(ins) => Self::from_value(&ins.val),
            Ty::Builtin(BuiltinTy::None) => Self::None,
            Ty::Builtin(BuiltinTy::Type(ty)) => Self::Type(*ty),
            Ty::Builtin(BuiltinTy::Content(_)) => Self::Content,
            Ty::Builtin(ty) => Self::Builtin(ty.clone()),
            Ty::Array(_) | Ty::Tuple(_) => Self::Array,
            Ty::Dict(_) => Self::Dict,
            Ty::Args(_) => Self::Args,
            Ty::Any
            | Ty::Param(_)
            | Ty::Func(_)
            | Ty::With(_)
            | Ty::Apply(_)
            | Ty::Select(_)
            | Ty::Unary(_)
            | Ty::Binary(_)
            | Ty::If(_)
            | Ty::Union(_)
            | Ty::Let(_)
            | Ty::Var(_)
            | Ty::Pattern(_) => Self::Unsupported,
        }
    }

    fn from_value(value: &Value) -> Self {
        match BuiltinTy::from_value(value) {
            Ty::Boolean(value) => Self::Boolean(value),
            Ty::Builtin(BuiltinTy::None) => Self::None,
            Ty::Builtin(BuiltinTy::Type(ty)) => Self::Type(ty),
            Ty::Builtin(BuiltinTy::Content(_)) => Self::Content,
            Ty::Builtin(ty) => Self::Builtin(ty),
            Ty::Array(_) | Ty::Tuple(_) => Self::Array,
            Ty::Dict(_) => Self::Dict,
            Ty::Args(_) => Self::Args,
            _ => Self::Unsupported,
        }
    }

    fn to_ty(&self) -> Option<Ty> {
        Some(match self {
            Self::None => Ty::Builtin(BuiltinTy::None),
            Self::Boolean(value) => Ty::Boolean(*value),
            Self::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
            Self::Builtin(ty) => Ty::Builtin(ty.clone()),
            Self::Array => Ty::Array(Ty::Any.into()),
            Self::Dict => Ty::Dict(RecordTy::new(vec![])),
            Self::Args => Ty::Builtin(BuiltinTy::Args),
            Self::Content => Ty::Builtin(BuiltinTy::Content(None)),
            Self::Unsupported => return None,
        })
    }
}

impl TypeChecker<'_> {
    fn mark_doc_annotated(&mut self, decl: &Interned<Decl>) {
        self.info.doc_annotated_vars.insert(decl.clone());
    }

    #[typst_macros::time(span = expr.span())]
    pub(crate) fn check_syntax(&mut self, expr: &Expr) -> Option<Ty> {
        Some(match expr {
            Expr::Block(exprs) => self.check_block(exprs),
            Expr::Array(elems) => self.check_array(elems.span, &elems.args),
            Expr::Dict(elems) => self.check_dict(elems.span, &elems.args),
            Expr::Args(args) => self.check_args(&args.args),
            // todo: check pattern correctly
            Expr::Pattern(pattern) => self.check_pattern_exp(pattern),
            Expr::Element(element) => self.check_element(element),
            Expr::Unary(unary) => self.check_unary(unary),
            Expr::Binary(binary) => self.check_binary(binary),
            Expr::Apply(apply) => self.check_apply(apply),
            Expr::Func(func) => self.check_func(func),
            Expr::Let(let_expr) => self.check_let(let_expr),
            Expr::Show(show) => self.check_show(show),
            Expr::Set(set) => self.check_set(set),
            Expr::Ref(reference) => self.check_ref(reference),
            Expr::ContentRef(content_ref) => self.check_content_ref(content_ref),
            Expr::Select(select) => self.check_select(select),
            Expr::Import(import) => self.check_import(import),
            Expr::Include(include) => self.check_include(include),
            Expr::Contextual(contextual) => self.check_contextual(contextual),
            Expr::Conditional(conditional) => self.check_conditional(conditional),
            Expr::WhileLoop(while_loop) => self.check_while_loop(while_loop),
            Expr::ForLoop(for_loop) => self.check_for_loop(for_loop),
            Expr::Type(ty) => self.check_type(ty),
            Expr::Decl(decl) => self.check_decl(decl),
            Expr::Star => self.check_star(),
        })
    }

    fn unique_const_string_key(&self, ty: &Ty) -> Option<Interned<str>> {
        fn unify(acc: &mut Option<Interned<str>>, next: Interned<str>) -> Option<()> {
            if acc.as_ref().is_some_and(|prev| prev != &next) {
                return None;
            }
            *acc = Some(next);
            Some(())
        }

        fn visit_lbs<'a>(
            this: &TypeChecker<'_>,
            lbs: impl IntoIterator<Item = &'a Ty>,
            acc: &mut Option<Interned<str>>,
        ) -> Option<()> {
            let mut any = false;
            for lb in lbs {
                any = true;
                visit(this, lb, acc)?;
            }
            any.then_some(())
        }

        fn visit(this: &TypeChecker<'_>, ty: &Ty, acc: &mut Option<Interned<str>>) -> Option<()> {
            match ty {
                Ty::Value(ins) => match &ins.val {
                    Value::Str(s) => unify(acc, Interned::new_str(s.as_str())),
                    _ => None,
                },
                Ty::Var(v) => {
                    let bounds = this.info.vars.get(&v.def)?;
                    let bounds_guard = bounds.bounds.bounds().read();
                    visit_lbs(this, bounds_guard.lbs.iter(), acc)
                }
                Ty::Let(bounds) => visit_lbs(this, bounds.lbs.iter(), acc),
                Ty::Union(types) => {
                    for ty in types.iter() {
                        visit(this, ty, acc)?;
                    }
                    Some(())
                }
                Ty::Param(p) => visit(this, &p.ty, acc),
                _ => None,
            }
        }

        let mut acc = None;
        visit(self, ty, &mut acc)?;
        acc
    }

    fn check_block(&mut self, exprs: &Interned<Vec<Expr>>) -> Ty {
        let content_like = exprs.iter().any(Self::expr_is_content_like);
        let mut joiner = if Self::is_markup_block(exprs) {
            Joiner::new_markup(content_like)
        } else {
            Joiner::new(content_like)
        };

        for child in exprs.iter() {
            let child_ty = self.check(child);
            if let Expr::Func(func) = child
                && matches!(func.decl.as_ref(), Decl::Func(..))
            {
                continue;
            }
            joiner.join(child_ty);
        }

        joiner.finalize()
    }

    fn is_markup_block(exprs: &[Expr]) -> bool {
        matches!(
            exprs.last(),
            Some(Expr::Type(Ty::Builtin(BuiltinTy::Content(None))))
        )
    }

    fn expr_is_content_like(expr: &Expr) -> bool {
        match expr {
            Expr::Element(_) | Expr::ContentRef(_) => true,
            Expr::Include(_) => true,
            Expr::Type(Ty::Builtin(BuiltinTy::Content(_) | BuiltinTy::Space)) => true,
            Expr::Type(Ty::Value(ins)) if matches!(ins.val, Value::Content(_)) => true,
            Expr::Block(exprs) => exprs.iter().any(Self::expr_is_content_like),
            Expr::Contextual(expr) => Self::expr_is_content_like(expr),
            Expr::Conditional(if_expr) => {
                Self::expr_is_content_like(&if_expr.then)
                    || Self::expr_is_content_like(&if_expr.else_)
            }
            Expr::WhileLoop(while_loop) => Self::expr_is_content_like(&while_loop.body),
            Expr::ForLoop(for_loop) => Self::expr_is_content_like(&for_loop.body),
            _ => false,
        }
    }

    fn check_array(&mut self, arr_span: Span, elems: &[ArgExpr]) -> Ty {
        let mut elements = Vec::new();

        for elem in elems.iter() {
            match elem {
                ArgExpr::Pos(pos) => {
                    elements.push(self.check(pos));
                }
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
                }
                ArgExpr::NamedRt(..) | ArgExpr::Named(..) => unreachable!(),
            }
        }

        let res = if elements.len() > MAX_PRECISE_TUPLE_ELEMENTS {
            let elem = Ty::from_types(
                elements
                    .into_iter()
                    .take(MAX_PRECISE_TUPLE_ELEMENTS)
                    .map(|ty| ty.compact_deferred_operand())
                    .collect::<Vec<_>>()
                    .into_iter(),
            );
            Ty::Array(elem.into())
        } else {
            Ty::Tuple(elements.into())
        };
        self.witness_at_most(arr_span, res.clone());
        res
    }

    fn check_dict(&mut self, dict_span: Span, elems: &[ArgExpr]) -> Ty {
        let mut fields = Vec::new();

        for elem in elems.iter() {
            match elem {
                ArgExpr::Named(n) => {
                    let (name, value) = n.as_ref();
                    let name = name.name().clone();
                    let val = self.check(value);
                    fields.push((name, val));
                }
                ArgExpr::NamedRt(n) => {
                    let (name, value) = n.as_ref();
                    let key = self.check(name);
                    let val = self.check(value);
                    if let Some(const_key) = self.unique_const_string_key(&key) {
                        fields.push((const_key, val));
                    }
                }
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
                }
                ArgExpr::Pos(..) => unreachable!(),
            }
        }

        let res = Ty::Dict(RecordTy::new(fields));
        self.witness_at_most(dict_span, res.clone());
        res
    }

    fn check_args(&mut self, args: &[ArgExpr]) -> Ty {
        let mut args_res = Vec::new();
        let mut named = vec![];

        for arg in args.iter() {
            match arg {
                ArgExpr::Pos(pos) => {
                    args_res.push(self.check(pos));
                }
                ArgExpr::Named(n) => {
                    let (name, value) = n.as_ref();
                    let name = name.name().clone();
                    let val = self.check(value);
                    named.push((name, val));
                }
                ArgExpr::NamedRt(n) => {
                    let (name, value) = n.as_ref();
                    let key = self.check(name);
                    let val = self.check(value);

                    if let Some(const_key) = self.unique_const_string_key(&key) {
                        named.push((const_key, val));
                    }
                }
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
                }
            }
        }

        let rest = if args_res.len() > MAX_PRECISE_TUPLE_ELEMENTS {
            args_res.truncate(MAX_PRECISE_TUPLE_ELEMENTS);
            Some(Ty::Array(Ty::Any.into()))
        } else {
            None
        };
        let args = ArgsTy::new(args_res.into_iter(), named, None, rest, None);

        Ty::Args(args.into())
    }

    fn check_pattern_exp(&mut self, pat: &Interned<Pattern>) -> Ty {
        self.check_pattern(None, pat, &EMPTY_DOCSTRING)
    }

    fn check_pattern(
        &mut self,
        base: Option<&Interned<Decl>>,
        pat: &Interned<Pattern>,
        docstring: &DocString,
    ) -> Ty {
        // todo: recursive doc constructing
        match pat.as_ref() {
            Pattern::Expr(expr) => self.check(expr),
            Pattern::Simple(decl) => {
                let ret = self.check_decl(decl);
                let var_doc = docstring.as_var();

                if let Some(annotated) = var_doc.ty.as_ref() {
                    self.mark_doc_annotated(decl);
                    self.constrain(&ret, annotated);
                }
                self.info
                    .var_docs
                    .insert(decl.clone(), var_doc.to_untyped());

                ret
            }
            Pattern::Sig(sig) => Ty::Pattern(self.check_pattern_sig(base, sig, docstring).0.into()),
        }
    }

    fn check_pattern_sig(
        &mut self,
        base: Option<&Interned<Decl>>,
        pat: &PatternSig,
        docstring: &DocString,
    ) -> (PatternTy, BTreeMap<Interned<str>, Ty>) {
        let mut pos_docs = vec![];
        let mut named_docs = BTreeMap::new();
        let mut rest_docs = None;

        let mut pos_all = vec![];
        let mut named_all = BTreeMap::new();
        let mut defaults = BTreeMap::new();
        let mut spread_right = None;

        // todo: combine with check_pattern
        for pos_expr in pat.pos.iter() {
            // pos.push(self.check_pattern(pattern, Ty::Any, docstring, root.clone()));
            let pos_ty = self.check_pattern_exp(pos_expr);
            if let Pattern::Simple(ident) = pos_expr.as_ref() {
                let name = ident.name().clone();

                let param_doc = docstring.get_var(&name).unwrap_or(&EMPTY_VAR_DOC);
                if let Some(annotated) = docstring.var_ty(&name) {
                    self.mark_doc_annotated(ident);
                    self.constrain(&pos_ty, annotated);
                }
                pos_docs.push(TypelessParamDocs {
                    name,
                    docs: param_doc.docs.clone(),
                    cano_type: (),
                    default: None,
                    attrs: ParamAttrs::positional(),
                });
            } else {
                pos_docs.push(TypelessParamDocs {
                    name: "_".into(),
                    docs: Default::default(),
                    cano_type: (),
                    default: None,
                    attrs: ParamAttrs::positional(),
                });
            }
            pos_all.push(pos_ty);
        }

        for (decl, named_expr) in pat.named.iter() {
            let name = decl.name().clone();
            let named_ty = self.check_pattern_exp(named_expr);
            let var = self.get_var(decl);
            let var_ty = Ty::Var(var.clone());
            if let Some(annotated) = docstring.var_ty(&name) {
                self.mark_doc_annotated(decl);
                self.constrain(&var_ty, annotated);
            }
            // todo: this is less efficient than v.lbs.push(exp), we may have some idea to
            // optimize it, so I put a todo here.
            self.constrain(&named_ty, &var_ty);
            named_all.insert(name.clone(), var_ty);
            defaults.insert(name.clone(), named_ty);

            let param_doc = docstring.get_var(&name).unwrap_or(&EMPTY_VAR_DOC);
            named_docs.insert(
                name.clone(),
                TypelessParamDocs {
                    name: name.clone(),
                    docs: param_doc.docs.clone(),
                    cano_type: (),
                    default: Some(named_expr.repr()),
                    attrs: ParamAttrs::named(),
                },
            );
            self.info
                .var_docs
                .insert(decl.clone(), param_doc.to_untyped());
        }

        // todo: spread left/right
        if let Some((decl, _spread_expr)) = &pat.spread_right {
            let var = self.get_var(decl);
            let name = var.name.clone();
            let param_doc = docstring
                .get_var(&var.name.clone())
                .unwrap_or(&EMPTY_VAR_DOC);
            self.info
                .var_docs
                .insert(decl.clone(), param_doc.to_untyped());

            let term = Ty::Builtin(BuiltinTy::Args);
            let var_ty = Ty::Var(var);
            if let Some(annotated) = docstring.var_ty(&name) {
                self.mark_doc_annotated(decl);
                self.constrain(&var_ty, annotated);
            }
            self.constrain(&term, &var_ty);
            spread_right = Some(var_ty);

            rest_docs = Some(TypelessParamDocs {
                name,
                docs: param_doc.docs.clone(),
                cano_type: (),
                default: None,
                attrs: ParamAttrs::variadic(),
            });
            // todo: ..(args)
        }

        let named: Vec<(Interned<str>, Ty)> = named_all.into_iter().collect();

        if let Some(base) = base {
            self.info.var_docs.insert(
                base.clone(),
                Arc::new(UntypedDefDocs::Function(Box::new(SignatureDocsT {
                    docs: docstring.docs.clone().unwrap_or_default(),
                    pos: pos_docs,
                    named: named_docs,
                    rest: rest_docs,
                    ret_ty: (),
                    hover_docs: Default::default(),
                }))),
            );
        }

        (
            PatternTy::new(pos_all.into_iter(), named, None, spread_right, None),
            defaults,
        )
    }

    fn check_element(&mut self, element: &Interned<ElementExpr>) -> Ty {
        for content in element.content.iter() {
            self.check(content);
        }

        Ty::Builtin(BuiltinTy::Content(Some(element.elem)))
    }

    fn check_unary(&mut self, unary: &Interned<UnExpr>) -> Ty {
        let op = unary.op;
        let lhs = self.check(&unary.lhs);
        Self::map_normal_flow(lhs, |lhs| {
            if op == UnaryOp::TypeOf {
                lhs.type_of_result()
            } else {
                Ty::Unary(TypeUnary::new(op, lhs))
            }
        })
    }

    fn check_binary(&mut self, binary: &Interned<BinExpr>) -> Ty {
        let op = binary.op;
        let [lhs, rhs] = binary.operands();
        let lhs = self.check(lhs);
        let rhs = self.check(rhs);

        let ty = match op {
            ast::BinOp::Add | ast::BinOp::Sub | ast::BinOp::Mul | ast::BinOp::Div => {
                if op == ast::BinOp::Add
                    && let Ty::Value(lhs_val) = &lhs
                    && let Ty::Value(rhs_val) = &rhs
                    && let Value::Str(lhs_str) = &lhs_val.val
                    && let Value::Str(rhs_str) = &rhs_val.val
                {
                    let mut combined = EcoString::with_capacity(lhs_str.len() + rhs_str.len());
                    combined.push_str(lhs_str.as_str());
                    combined.push_str(rhs_str.as_str());
                    return Ty::Value(InsTy::new(Value::Str(combined.into())));
                }

                let check = self.check_arithmetic_binary(op, &lhs, &rhs);
                if check.incompatible {
                    Self::warn_incompatible_binary_once();
                }
                if matches!(check.ty, Ty::Any)
                    && !check.incompatible
                    && (check.unknown
                        || self.has_deferred_binary_operand(&lhs)
                        || self.has_deferred_binary_operand(&rhs))
                {
                    Ty::Binary(TypeBinary::new(op, lhs, rhs))
                } else {
                    check.ty
                }
            }
            ast::BinOp::Eq | ast::BinOp::Neq | ast::BinOp::Leq | ast::BinOp::Geq => {
                self.check_comparable(&lhs, &rhs);
                if op == ast::BinOp::Neq
                    || (op == ast::BinOp::Eq
                        && (Self::is_typeof_operand(&lhs) || Self::is_typeof_operand(&rhs)))
                {
                    self.possible_ever_be(&lhs, &rhs);
                    self.possible_ever_be(&rhs, &lhs);
                }
                if matches!(op, ast::BinOp::Leq | ast::BinOp::Geq)
                    && self.check_ordering_binary(&lhs, &rhs).incompatible
                {
                    Self::warn_incompatible_binary_once();
                }
                self.fold_deferred_binary(op, lhs, rhs)
            }
            ast::BinOp::Lt | ast::BinOp::Gt => {
                self.check_comparable(&lhs, &rhs);
                if self.check_ordering_binary(&lhs, &rhs).incompatible {
                    Self::warn_incompatible_binary_once();
                }
                Ty::Boolean(None)
            }
            ast::BinOp::And | ast::BinOp::Or => {
                self.constrain(&lhs, &Ty::Boolean(None));
                self.constrain(&rhs, &Ty::Boolean(None));
                if self.check_boolean_binary(&lhs, &rhs).incompatible {
                    Self::warn_incompatible_binary_once();
                }
                self.fold_deferred_binary(op, lhs, rhs)
            }
            ast::BinOp::NotIn | ast::BinOp::In => {
                self.check_containing(&rhs, &lhs, op == ast::BinOp::In);
                Ty::Boolean(None)
            }
            ast::BinOp::Assign => {
                self.check_assignable(&lhs, &rhs);
                self.possible_ever_be(&lhs, &rhs);
                Ty::Builtin(BuiltinTy::None)
            }
            ast::BinOp::AddAssign
            | ast::BinOp::SubAssign
            | ast::BinOp::MulAssign
            | ast::BinOp::DivAssign => {
                self.check_assignable(&lhs, &rhs);
                let op = match op {
                    ast::BinOp::AddAssign => ast::BinOp::Add,
                    ast::BinOp::SubAssign => ast::BinOp::Sub,
                    ast::BinOp::MulAssign => ast::BinOp::Mul,
                    ast::BinOp::DivAssign => ast::BinOp::Div,
                    _ => unreachable!(),
                };
                if self.check_arithmetic_binary(op, &lhs, &rhs).incompatible {
                    Self::warn_incompatible_binary_once();
                }
                Ty::Builtin(BuiltinTy::None)
            }
        };

        ty
    }

    fn check_arithmetic_binary(&self, op: ast::BinOp, lhs: &Ty, rhs: &Ty) -> BinaryCheck {
        self.check_binary_pairs(lhs, rhs, |lhs, rhs| {
            Self::arithmetic_pair_result(op, lhs, rhs)
        })
    }

    fn check_ordering_binary(&self, lhs: &Ty, rhs: &Ty) -> BinaryCheck {
        self.check_binary_pairs(lhs, rhs, Self::ordering_pair_result)
    }

    fn check_boolean_binary(&self, lhs: &Ty, rhs: &Ty) -> BinaryCheck {
        self.check_binary_pairs(lhs, rhs, |lhs, rhs| {
            if matches!((lhs, rhs), (BinaryAtom::Boolean(_), BinaryAtom::Boolean(_))) {
                BinaryPairResult::Known(Ty::Boolean(None))
            } else {
                BinaryPairResult::Incompatible
            }
        })
    }

    fn check_binary_pairs(
        &self,
        lhs: &Ty,
        rhs: &Ty,
        mut f: impl FnMut(&BinaryAtom, &BinaryAtom) -> BinaryPairResult,
    ) -> BinaryCheck {
        let lhs = self.collect_binary_atoms(lhs);
        let rhs = self.collect_binary_atoms(rhs);
        let mut result = Vec::new();
        let mut unknown = lhs.unknown || rhs.unknown;

        for lhs in &lhs.atoms {
            for rhs in &rhs.atoms {
                match f(lhs, rhs) {
                    BinaryPairResult::Known(ty) => result.push(ty),
                    BinaryPairResult::Unknown => unknown = true,
                    BinaryPairResult::Incompatible => {}
                }
            }
        }

        if unknown {
            return BinaryCheck {
                ty: Ty::Any,
                incompatible: false,
                unknown,
            };
        }

        BinaryCheck {
            incompatible: result.is_empty(),
            ty: Self::union_binary_results(result),
            unknown,
        }
    }

    fn arithmetic_pair_result(
        op: ast::BinOp,
        lhs: &BinaryAtom,
        rhs: &BinaryAtom,
    ) -> BinaryPairResult {
        match op {
            ast::BinOp::Add => Self::add_pair_result(lhs, rhs),
            ast::BinOp::Sub => Self::sub_pair_result(lhs, rhs),
            ast::BinOp::Mul => Self::mul_pair_result(lhs, rhs),
            ast::BinOp::Div => Self::div_pair_result(lhs, rhs),
            _ => unreachable!("not an arithmetic binary operation"),
        }
    }

    fn add_pair_result(lhs: &BinaryAtom, rhs: &BinaryAtom) -> BinaryPairResult {
        if matches!(lhs, BinaryAtom::None) {
            return rhs
                .to_ty()
                .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
        }
        if matches!(rhs, BinaryAtom::None) {
            return lhs
                .to_ty()
                .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
        }
        if let Some(ty) = Self::numeric_result(lhs, rhs, false) {
            return BinaryPairResult::Known(ty);
        }
        if Self::is_relative_length_pair(lhs, rhs) {
            return BinaryPairResult::Known(Ty::Any);
        }
        if Self::is_stroke_pair(lhs, rhs) {
            return BinaryPairResult::Known(Ty::Builtin(BuiltinTy::Stroke));
        }
        if Self::same_named_pair(lhs, rhs, Self::is_addable_same_name) {
            return lhs
                .to_ty()
                .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
        }
        if Self::is_content_text_pair(lhs, rhs) {
            return BinaryPairResult::Known(Ty::Builtin(BuiltinTy::Content(None)));
        }

        BinaryPairResult::Incompatible
    }

    fn sub_pair_result(lhs: &BinaryAtom, rhs: &BinaryAtom) -> BinaryPairResult {
        if let Some(ty) = Self::numeric_result(lhs, rhs, false) {
            return BinaryPairResult::Known(ty);
        }
        if Self::is_relative_length_pair(lhs, rhs) {
            return BinaryPairResult::Known(Ty::Any);
        }
        if Self::same_named_pair(lhs, rhs, Self::is_subtractable_same_name) {
            return lhs
                .to_ty()
                .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
        }
        if Self::same_name(lhs, rhs).is_some_and(|name| name == "datetime") {
            return BinaryPairResult::Known(Ty::Any);
        }

        BinaryPairResult::Incompatible
    }

    fn mul_pair_result(lhs: &BinaryAtom, rhs: &BinaryAtom) -> BinaryPairResult {
        if let Some(ty) = Self::numeric_result(lhs, rhs, false) {
            return BinaryPairResult::Known(ty);
        }
        if Self::is_numeric_atom(lhs) {
            if Self::is_scalable_by_number(rhs) {
                return rhs
                    .to_ty()
                    .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
            }
            if Self::is_repeatable_by_int(rhs) && Self::atom_name(lhs).is_some_and(|n| n == "int") {
                return rhs
                    .to_ty()
                    .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
            }
        }
        if Self::is_numeric_atom(rhs) {
            if Self::is_scalable_by_number(lhs) {
                return lhs
                    .to_ty()
                    .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
            }
            if Self::is_repeatable_by_int(lhs) && Self::atom_name(rhs).is_some_and(|n| n == "int") {
                return lhs
                    .to_ty()
                    .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
            }
        }

        BinaryPairResult::Incompatible
    }

    fn div_pair_result(lhs: &BinaryAtom, rhs: &BinaryAtom) -> BinaryPairResult {
        if Self::is_numeric_atom(lhs) && Self::is_numeric_atom(rhs) {
            return BinaryPairResult::Known(Ty::Builtin(BuiltinTy::Float));
        }
        if Self::is_scalable_by_number(lhs) && Self::is_numeric_atom(rhs) {
            return lhs
                .to_ty()
                .map_or(BinaryPairResult::Unknown, BinaryPairResult::Known);
        }
        if Self::is_scalable_by_number(lhs) && Self::same_name(lhs, rhs).is_some() {
            return BinaryPairResult::Known(Ty::Builtin(BuiltinTy::Float));
        }

        BinaryPairResult::Incompatible
    }

    fn ordering_pair_result(lhs: &BinaryAtom, rhs: &BinaryAtom) -> BinaryPairResult {
        if Self::is_numeric_atom(lhs) && Self::is_numeric_atom(rhs) {
            return BinaryPairResult::Known(Ty::Boolean(None));
        }
        if Self::same_named_pair(lhs, rhs, Self::is_orderable_same_name) {
            return BinaryPairResult::Known(Ty::Boolean(None));
        }

        BinaryPairResult::Incompatible
    }

    fn collect_binary_atoms(&self, ty: &Ty) -> BinaryAtoms {
        let mut atoms = BinaryAtoms::default();
        self.collect_binary_atoms_(&mut atoms, ty, 0);
        atoms
    }

    fn collect_binary_atoms_(&self, atoms: &mut BinaryAtoms, ty: &Ty, depth: usize) {
        if atoms.atoms.len() >= MAX_BINARY_ATOMS || depth >= MAX_BINARY_ATOMS {
            atoms.unknown = true;
            return;
        }

        match ty {
            Ty::Any => atoms.unknown = true,
            Ty::Union(types) => {
                for ty in types.iter() {
                    self.collect_binary_atoms_(atoms, ty, depth + 1);
                    if atoms.atoms.len() >= MAX_BINARY_ATOMS {
                        atoms.unknown = true;
                        return;
                    }
                }
            }
            Ty::Let(bounds) => {
                if bounds.lbs.is_empty() {
                    atoms.unknown = true;
                    return;
                }
                for ty in bounds.lbs.iter() {
                    self.collect_binary_atoms_(atoms, ty, depth + 1);
                    if atoms.atoms.len() >= MAX_BINARY_ATOMS {
                        atoms.unknown = true;
                        return;
                    }
                }
            }
            Ty::Var(var) => {
                if let Some(local) = self.local_bind_of(var) {
                    self.collect_binary_atoms_(atoms, &local, depth + 1);
                    return;
                }

                let Some(bounds) = self.info.vars.get(&var.def) else {
                    atoms.unknown = true;
                    return;
                };
                let bounds = bounds.bounds.bounds().read();
                if bounds.lbs.is_empty() {
                    atoms.unknown = true;
                    return;
                }
                for ty in bounds.lbs.iter() {
                    self.collect_binary_atoms_(atoms, ty, depth + 1);
                    if atoms.atoms.len() >= MAX_BINARY_ATOMS {
                        atoms.unknown = true;
                        return;
                    }
                }
            }
            Ty::Param(param) => self.collect_binary_atoms_(atoms, &param.ty, depth + 1),
            Ty::Apply(_) | Ty::Select(_) | Ty::Unary(_) | Ty::Binary(_) | Ty::If(_) => {
                atoms.unknown = true
            }
            ty => atoms.push(BinaryAtom::from_ty(ty)),
        }
    }

    fn union_binary_results(types: Vec<Ty>) -> Ty {
        if types.is_empty() {
            return Ty::Any;
        }
        if types.iter().any(|ty| matches!(ty, Ty::Any)) {
            return Ty::Any;
        }

        let mut unique = Vec::new();
        for ty in types {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }

        Ty::from_types(unique.into_iter())
    }

    fn numeric_result(lhs: &BinaryAtom, rhs: &BinaryAtom, div: bool) -> Option<Ty> {
        if !Self::is_numeric_atom(lhs) || !Self::is_numeric_atom(rhs) {
            return None;
        }
        if div
            || [lhs, rhs]
                .into_iter()
                .any(|atom| Self::atom_name(atom).is_some_and(|name| name == "float"))
        {
            return Some(Ty::Builtin(BuiltinTy::Float));
        }
        if let Some(decimal) = [lhs, rhs]
            .into_iter()
            .find(|atom| Self::atom_name(atom).is_some_and(|name| name == "decimal"))
        {
            return decimal.to_ty();
        }
        lhs.to_ty()
    }

    fn same_named_pair(lhs: &BinaryAtom, rhs: &BinaryAtom, predicate: fn(&str) -> bool) -> bool {
        Self::same_name(lhs, rhs).is_some_and(predicate)
    }

    fn same_name(lhs: &BinaryAtom, rhs: &BinaryAtom) -> Option<&'static str> {
        let lhs = Self::atom_name(lhs)?;
        let rhs = Self::atom_name(rhs)?;
        (lhs == rhs).then_some(lhs)
    }

    fn atom_name(atom: &BinaryAtom) -> Option<&'static str> {
        match atom {
            BinaryAtom::None => Some("none"),
            BinaryAtom::Boolean(_) => Some("bool"),
            BinaryAtom::Type(ty) => Some(ty.short_name()),
            BinaryAtom::Builtin(BuiltinTy::Auto) => Some("auto"),
            BinaryAtom::Builtin(BuiltinTy::Args) => Some("arguments"),
            BinaryAtom::Builtin(BuiltinTy::Color) => Some("color"),
            BinaryAtom::Builtin(BuiltinTy::Length) => Some("length"),
            BinaryAtom::Builtin(BuiltinTy::Float) => Some("float"),
            BinaryAtom::Builtin(BuiltinTy::Stroke) => Some("stroke"),
            BinaryAtom::Array => Some("array"),
            BinaryAtom::Dict => Some("dictionary"),
            BinaryAtom::Args => Some("arguments"),
            BinaryAtom::Content => Some("content"),
            BinaryAtom::Builtin(_) | BinaryAtom::Unsupported => None,
        }
    }

    fn is_numeric_atom(atom: &BinaryAtom) -> bool {
        Self::atom_name(atom).is_some_and(|name| matches!(name, "int" | "float" | "decimal"))
    }

    fn is_relative_length_pair(lhs: &BinaryAtom, rhs: &BinaryAtom) -> bool {
        let lhs = Self::atom_name(lhs);
        let rhs = Self::atom_name(rhs);
        matches!(
            (lhs, rhs),
            (
                Some("length" | "ratio" | "relative length"),
                Some("length" | "ratio" | "relative length")
            )
        )
    }

    fn is_stroke_pair(lhs: &BinaryAtom, rhs: &BinaryAtom) -> bool {
        matches!(
            (Self::atom_name(lhs), Self::atom_name(rhs)),
            (Some("color" | "gradient" | "tiling"), Some("length"))
                | (Some("length"), Some("color" | "gradient" | "tiling"))
        )
    }

    fn is_content_text_pair(lhs: &BinaryAtom, rhs: &BinaryAtom) -> bool {
        matches!(
            (Self::atom_name(lhs), Self::atom_name(rhs)),
            (Some("content"), Some("str" | "symbol")) | (Some("str" | "symbol"), Some("content"))
        )
    }

    fn is_addable_same_name(name: &str) -> bool {
        matches!(
            name,
            "int"
                | "float"
                | "decimal"
                | "angle"
                | "length"
                | "ratio"
                | "relative length"
                | "fraction"
                | "symbol"
                | "str"
                | "bytes"
                | "content"
                | "array"
                | "dictionary"
                | "arguments"
                | "duration"
        )
    }

    fn is_subtractable_same_name(name: &str) -> bool {
        matches!(
            name,
            "int"
                | "float"
                | "decimal"
                | "angle"
                | "length"
                | "ratio"
                | "relative length"
                | "fraction"
                | "duration"
        )
    }

    fn is_orderable_same_name(name: &str) -> bool {
        matches!(
            name,
            "int"
                | "float"
                | "decimal"
                | "angle"
                | "length"
                | "ratio"
                | "relative length"
                | "fraction"
                | "str"
                | "bytes"
                | "version"
                | "datetime"
                | "duration"
        )
    }

    fn is_scalable_by_number(atom: &BinaryAtom) -> bool {
        Self::atom_name(atom).is_some_and(|name| {
            matches!(
                name,
                "angle" | "length" | "ratio" | "relative length" | "fraction" | "duration"
            )
        })
    }

    fn is_repeatable_by_int(atom: &BinaryAtom) -> bool {
        Self::atom_name(atom).is_some_and(|name| matches!(name, "str" | "array" | "content"))
    }

    fn warn_incompatible_binary_once() {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            log::warn!(
                "experimental type checker found incompatible binary operands; suppressing further warnings"
            );
        });
    }

    fn check_select(&mut self, select: &Interned<SelectExpr>) -> Ty {
        let select_site = select.span;
        let ty = self.check(&select.lhs);
        let field = select.key.name().clone();
        crate::log_debug_ct!("field access: {select:?}[{select_site:?}] => {ty:?}.{field:?}");

        // todo: move this to base
        let base = Ty::Select(SelectTy::new(ty.clone().into(), field.clone()));
        let mut worker = SelectFieldChecker {
            base: self,
            resultant: vec![base],
        };
        ty.select(&field, true, &mut worker);
        let mut resultants = worker.resultant;
        if resultants.len() == 1
            && matches!(resultants.first(), Some(Ty::Select(..)))
            && let Some(method) = self.select_unresolved_builtin_method(&ty, &field)
        {
            resultants.push(method);
        }
        let resultants = Self::dedup_select_resultants(resultants);
        let res = Ty::from_types(resultants.into_iter());
        self.witness_at_least(select_site, res.clone());
        res
    }

    pub(super) fn dedup_select_resultants(resultants: Vec<Ty>) -> Vec<Ty> {
        let mut unique = Vec::with_capacity(resultants.len());
        for ty in resultants {
            if unique
                .iter()
                .any(|prev| Self::same_select_resultant(prev, &ty))
            {
                continue;
            }
            unique.push(ty);
        }
        unique
    }

    fn same_select_resultant(lhs: &Ty, rhs: &Ty) -> bool {
        lhs == rhs || matches!((lhs, rhs), (Ty::Value(lhs), Ty::Value(rhs)) if lhs.val == rhs.val)
    }

    fn select_unresolved_builtin_method(&mut self, receiver: &Ty, field: &str) -> Option<Ty> {
        if field != "split" {
            return None;
        }

        let str_ty = typst::foundations::Type::of::<typst::foundations::Str>();
        let method = str_ty.scope().get(field)?;
        self.constrain(receiver, &Ty::Builtin(BuiltinTy::Type(str_ty)));
        Some(Ty::Value(InsTy::new_at(
            method.read().clone(),
            method.span(),
        )))
    }

    fn check_apply(&mut self, apply: &Interned<ApplyExpr>) -> Ty {
        let args = self.check(&apply.args);
        let callee = self.check(&apply.callee);

        let at_default = self.check_at_call_static_key(apply, &args);
        if let AtCallCheck::StaticKey(res) = at_default {
            return res;
        }
        let at_default = at_default.default();

        crate::log_debug_ct!("func_call: {callee:?} with {args:?}");

        if let Ty::Args(args) = args {
            let resultants = {
                let mut worker = ApplyTypeChecker {
                    base: self,
                    call_site: apply.callee.span(),
                    allow_deferred_calls: true,
                    call_raw_for_with: Some(callee.clone()),
                    resultant: vec![],
                };
                callee.call(&args, true, &mut worker);
                worker.resultant
            };
            let mut res = if resultants.is_empty() && self.should_preserve_unresolved_apply(&callee)
            {
                Ty::Apply(ApplyTy::new(callee.clone().into(), args.clone()))
            } else {
                Self::call_resultant(resultants)
            };
            if let Some(default) = at_default {
                res = Self::at_result_with_default(res, default);
            }
            self.witness_at_least(apply.span, res.clone());
            return res;
        }

        Ty::Any
    }

    fn check_at_call_static_key(&mut self, apply: &Interned<ApplyExpr>, args: &Ty) -> AtCallCheck {
        let Expr::Select(select) = &apply.callee else {
            return AtCallCheck::None;
        };
        if select.key.name().as_ref() != "at" {
            return AtCallCheck::None;
        }
        let Ty::Args(args_ty) = args else {
            return AtCallCheck::None;
        };
        if args_ty.positional_params().len() != 1 || args_ty.rest_param().is_some() {
            return AtCallCheck::None;
        }

        let default = {
            let mut named = args_ty.named_params();
            match named.next() {
                None => None,
                Some((name, default)) if name.as_ref() == "default" && named.next().is_none() => {
                    Some(default.clone())
                }
                _ => return AtCallCheck::None,
            }
        };

        let key = args_ty
            .pos(0)
            .and_then(|ty| self.unique_const_string_key(ty));
        let Some(key) = key else {
            return default
                .map(AtCallCheck::Default)
                .unwrap_or(AtCallCheck::None);
        };

        let base = self.check(&select.lhs);
        let selected = if default.is_some() && self.is_unknown_at_base(&base) {
            Ty::Any
        } else {
            Ty::Select(SelectTy::new(base.into(), key))
        };
        let res = default
            .map(|default| Self::at_result_with_default(selected.clone(), default))
            .unwrap_or(selected);
        self.witness_at_least(apply.span, res.clone());
        AtCallCheck::StaticKey(res)
    }

    fn is_unknown_at_base(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Any => true,
            Ty::Param(param) => self.is_unknown_at_base(&param.ty),
            Ty::Var(var) => {
                let Some(bounds) = self.info.vars.get(&var.def) else {
                    return true;
                };
                let bounds = bounds.bounds.bounds().read();
                bounds.lbs.is_empty() || bounds.lbs.iter().all(|ty| matches!(ty, Ty::Any))
            }
            Ty::Let(bounds) => {
                bounds.lbs.is_empty() || bounds.lbs.iter().all(|ty| matches!(ty, Ty::Any))
            }
            _ => false,
        }
    }

    fn at_result_with_default(result: Ty, default: Ty) -> Ty {
        if matches!(result, Ty::Any) {
            let mut lbs = vec![Ty::Any, default];
            lbs.sort();
            lbs.dedup();
            return Ty::Let(TypeBounds { lbs, ubs: vec![] }.into());
        }

        Ty::from_types([result, default].into_iter())
    }

    fn check_func(&mut self, func: &Interned<FuncExpr>) -> Ty {
        self.check_func_sig(func)
    }

    pub(super) fn check_func_sig(&mut self, func: &Interned<FuncExpr>) -> Ty {
        self.check_func_sig_(func, true)
    }

    pub(super) fn check_func_sig_shallow(&mut self, func: &Interned<FuncExpr>) -> Ty {
        self.check_func_sig_(func, false)
    }

    fn check_func_sig_(&mut self, func: &Interned<FuncExpr>, infer_body: bool) -> Ty {
        let def_id = func.decl.clone();
        let var = Ty::Var(self.get_var(&def_id));

        let docstring = self.check_docstring(&def_id);
        let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        crate::log_debug_ct!("check closure: {func:?} with docs {docstring:#?}");

        let (sig, defaults) = self.check_pattern_sig(Some(&def_id), &func.params, docstring);

        let body = if let Some(body) = self.deferred_func_returns.get(&def_id).cloned() {
            body
        } else {
            let body = self.fresh_return_var(&def_id);
            self.deferred_func_returns
                .insert(def_id.clone(), body.clone());
            if infer_body {
                self.defer_func_body(func.clone(), body.clone());
            }
            body
        };
        let res_ty = if let Some(annotated) = &docstring.res_ty {
            self.constrain(&body, annotated);
            body
        } else {
            body
        };

        // freeze the signature
        for inp in sig.inputs.iter() {
            self.weaken(inp);
        }

        let sig = sig.with_body(res_ty).into();
        let sig = if defaults.is_empty() {
            Ty::Func(sig)
        } else {
            let defaults: Vec<(Interned<str>, Ty)> = defaults.into_iter().collect();
            let with_defaults = SigWithTy {
                sig: Ty::Func(sig).into(),
                with: ArgsTy::new([].into_iter(), defaults, None, None, None).into(),
            };
            Ty::With(with_defaults.into())
        };

        self.constrain(&sig, &var);
        sig
    }

    fn check_let(&mut self, let_expr: &Interned<LetExpr>) -> Ty {
        // todo: consistent pattern docs
        let docstring = self.check_docstring(&Decl::pattern(let_expr.span).into());
        let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        let direct_func_value = match (let_expr.pattern.as_ref(), let_expr.body.as_ref()) {
            (Pattern::Simple(_), Some(Expr::Func(func))) => Some(func.decl.clone()),
            _ => None,
        };
        let term = match &let_expr.body {
            Some(expr) => self.check(expr),
            None => Ty::Builtin(BuiltinTy::None),
        };
        if let Some(def) = direct_func_value {
            self.mark_deferred_func_body_default(&def);
        }
        if let Some(annotated) = &docstring.res_ty {
            self.constrain(&term, annotated);
        }
        let value = docstring.res_ty.clone().unwrap_or(term);

        let pat = self.check_pattern(None, &let_expr.pattern, docstring);
        self.constrain(&value, &pat);

        Ty::Builtin(BuiltinTy::None)
    }

    fn check_show(&mut self, show: &Interned<ShowExpr>) -> Ty {
        let selector = show.selector.as_ref().map(|sel| self.check(sel));
        let transform = self.check(&show.edit);

        self.constraint_show(selector, transform);
        Ty::Builtin(BuiltinTy::None)
    }

    fn constraint_show(&mut self, selector: Option<Ty>, transform: Ty) -> Option<()> {
        crate::log_debug_ct!("show on {selector:?}, transform {transform:?}");

        let selected = match selector {
            Some(selector) => Self::content_by_selector(selector)?,
            None => Ty::Builtin(BuiltinTy::Content(None)),
        };

        let show_fact = Ty::Func(SigTy::unary(selected, Ty::Any));
        crate::log_debug_ct!("check show_fact type {show_fact:?} value: {transform:?}");
        self.constrain(&transform, &show_fact);

        Some(())
    }

    fn content_by_selector(selector: Ty) -> Option<Ty> {
        #[inline(always)]
        fn text_type() -> Ty {
            Ty::Builtin(BuiltinTy::Content(Some(
                Element::of::<typst::text::TextElem>(),
            )))
        }

        crate::log_debug_ct!("check selector {selector:?}");

        Some(match selector {
            Ty::With(with) => return Self::content_by_selector(with.sig.as_ref().clone()),
            Ty::Builtin(BuiltinTy::Type(ty)) => {
                if ty == Type::of::<typst::foundations::Regex>() {
                    text_type()
                } else {
                    return None;
                }
            }
            Ty::Builtin(BuiltinTy::Element(ty)) => Ty::Builtin(BuiltinTy::Content(Some(ty))),
            Ty::Value(ins_ty) => match &ins_ty.val {
                Value::Str(..) => text_type(),
                Value::Content(c) => Ty::Builtin(BuiltinTy::Content(Some(c.elem()))),
                Value::Func(f) => {
                    if let Some(elem) = f.to_element() {
                        Ty::Builtin(BuiltinTy::Content(Some(elem)))
                    } else {
                        return None;
                    }
                }
                Value::Dyn(value) => {
                    if value.ty() == Type::of::<typst::foundations::Regex>() {
                        text_type()
                    } else {
                        return None;
                    }
                }
                _ => return None,
            },
            _ => return None,
        })
    }

    // todo: merge with func call, and regard difference (may be here)
    fn check_set(&mut self, set: &Interned<SetExpr>) -> Ty {
        let callee = self.check(&set.target);
        let args = self.check(&set.args);
        let _cond = set.cond.as_ref().map(|cond| self.check(cond));

        crate::log_debug_ct!("set rule: {callee:?} with {args:?}");

        if let Ty::Args(args) = args {
            let mut worker = ApplyTypeChecker {
                base: self,
                // todo: call site
                call_site: Span::detached(),
                allow_deferred_calls: false,
                // call_site: set_rule.target().span(),
                call_raw_for_with: Some(callee.clone()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            return Ty::from_types(worker.resultant.into_iter());
        }

        Ty::Any
    }

    fn check_ref(&mut self, r: &Interned<RefExpr>) -> Ty {
        let s = r.decl.span();
        let s = (!s.is_detached()).then_some(s);
        let of = r.root.as_ref().map(|of| self.check(of));
        let of = of.or_else(|| r.term.clone());
        if let Some((s, of)) = s.zip(of.as_ref()) {
            self.witness_at_most(s, of.clone());
        }

        of.unwrap_or(Ty::Any)
    }

    fn check_content_ref(&mut self, content_ref: &Interned<ContentRefExpr>) -> Ty {
        if let Some(body) = content_ref.body.as_ref() {
            self.check(body);
        }
        Ty::Builtin(BuiltinTy::Content(None))
    }

    fn check_path_source(&mut self, source: &Expr) -> Ty {
        let ty = self.check(source);
        self.constrain(
            &ty,
            &Ty::Builtin(BuiltinTy::Path(PathKind::Source {
                allow_package: true,
            })),
        );
        ty
    }

    fn check_import(&mut self, import: &Interned<ImportExpr>) -> Ty {
        self.check_path_source(&import.source);
        self.check_ref(&import.decl);
        Ty::Builtin(BuiltinTy::None)
    }

    fn check_include(&mut self, include: &Interned<IncludeExpr>) -> Ty {
        self.check_path_source(&include.source);
        Ty::Builtin(BuiltinTy::Content(None))
    }

    fn check_contextual(&mut self, expr: &Interned<Expr>) -> Ty {
        let body = self.check(expr);

        Self::map_normal_flow(body, |body| {
            Ty::Unary(TypeUnary::new(UnaryOp::Context, body))
        })
    }

    fn check_conditional(&mut self, if_expr: &Interned<IfExpr>) -> Ty {
        let cond = self.check(&if_expr.cond);
        let then = self.check(&if_expr.then);
        let else_ = self.check(&if_expr.else_);

        let (controls, cond) = Self::split_control_flow(cond);
        let normal = cond.map(|cond| match Self::known_bool(&cond) {
            Some(true) => then,
            Some(false) => else_,
            None => Ty::from_types([then, else_].into_iter()),
        });
        Self::merge_control_flow(controls, normal)
    }

    pub(super) fn fold_deferred_binary(&self, op: ast::BinOp, lhs: Ty, rhs: Ty) -> Ty {
        match op {
            ast::BinOp::Add | ast::BinOp::Sub | ast::BinOp::Mul | ast::BinOp::Div => {
                if op == ast::BinOp::Add
                    && let Ty::Value(lhs_val) = &lhs
                    && let Ty::Value(rhs_val) = &rhs
                    && let Value::Str(lhs_str) = &lhs_val.val
                    && let Value::Str(rhs_str) = &rhs_val.val
                {
                    let mut combined = EcoString::with_capacity(lhs_str.len() + rhs_str.len());
                    combined.push_str(lhs_str.as_str());
                    combined.push_str(rhs_str.as_str());
                    return Ty::Value(InsTy::new(Value::Str(combined.into())));
                }

                let check = self.check_arithmetic_binary(op, &lhs, &rhs);
                if check.incompatible {
                    Self::warn_incompatible_binary_once();
                }
                if matches!(check.ty, Ty::Any)
                    && !check.incompatible
                    && (check.unknown
                        || self.has_deferred_binary_operand(&lhs)
                        || self.has_deferred_binary_operand(&rhs))
                {
                    Ty::Binary(TypeBinary::new(op, lhs, rhs))
                } else {
                    check.ty
                }
            }
            ast::BinOp::Eq | ast::BinOp::Neq => {
                if let Some(eq) = Self::known_equal(&lhs, &rhs) {
                    return Ty::Boolean(Some(if op == ast::BinOp::Eq { eq } else { !eq }));
                }
                if self.has_deferred_binary_operand(&lhs) || self.has_deferred_binary_operand(&rhs)
                {
                    Ty::Binary(TypeBinary::new(op, lhs, rhs))
                } else {
                    Ty::Boolean(None)
                }
            }
            ast::BinOp::Leq | ast::BinOp::Geq | ast::BinOp::Lt | ast::BinOp::Gt => {
                Ty::Boolean(None)
            }
            ast::BinOp::And | ast::BinOp::Or => {
                match (Self::known_bool(&lhs), Self::known_bool(&rhs), op) {
                    (Some(lhs), Some(rhs), ast::BinOp::And) => Ty::Boolean(Some(lhs && rhs)),
                    (Some(lhs), Some(rhs), ast::BinOp::Or) => Ty::Boolean(Some(lhs || rhs)),
                    _ if self.has_deferred_binary_operand(&lhs)
                        || self.has_deferred_binary_operand(&rhs) =>
                    {
                        Ty::Binary(TypeBinary::new(op, lhs, rhs))
                    }
                    _ => Ty::Boolean(None),
                }
            }
            ast::BinOp::In | ast::BinOp::NotIn => Ty::Boolean(None),
            ast::BinOp::Assign
            | ast::BinOp::AddAssign
            | ast::BinOp::SubAssign
            | ast::BinOp::MulAssign
            | ast::BinOp::DivAssign => Ty::Builtin(BuiltinTy::None),
        }
    }

    fn known_equal(lhs: &Ty, rhs: &Ty) -> Option<bool> {
        match (lhs, rhs) {
            (Ty::Value(lhs), Ty::Value(rhs)) => Some(lhs.val == rhs.val),
            (Ty::Value(lhs), Ty::Boolean(Some(rhs))) | (Ty::Boolean(Some(rhs)), Ty::Value(lhs)) => {
                match &lhs.val {
                    Value::Bool(lhs) => Some(lhs == rhs),
                    _ => Some(false),
                }
            }
            (Ty::Boolean(Some(lhs)), Ty::Boolean(Some(rhs))) => Some(lhs == rhs),
            (Ty::Builtin(BuiltinTy::None), Ty::Value(rhs))
            | (Ty::Value(rhs), Ty::Builtin(BuiltinTy::None)) => Some(rhs.val == Value::None),
            (Ty::Builtin(BuiltinTy::None), Ty::Builtin(BuiltinTy::None)) => Some(true),
            _ => None,
        }
    }

    fn is_typeof_operand(ty: &Ty) -> bool {
        match ty {
            Ty::Unary(unary) if unary.op == UnaryOp::TypeOf => true,
            Ty::Param(param) => Self::is_typeof_operand(&param.ty),
            Ty::Union(types) => types.iter().any(Self::is_typeof_operand),
            Ty::Let(bounds) => bounds
                .lbs
                .iter()
                .chain(bounds.ubs.iter())
                .any(Self::is_typeof_operand),
            _ => false,
        }
    }

    fn has_deferred_binary_operand(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Var(var) => self
                .local_bind_of(var)
                .as_ref()
                .is_none_or(|local| self.has_deferred_binary_operand(local)),
            Ty::Param(param) => self.has_deferred_binary_operand(&param.ty),
            Ty::Array(elem) => self.has_deferred_binary_operand(elem),
            Ty::Tuple(elems) | Ty::Union(elems) => {
                elems.iter().any(|ty| self.has_deferred_binary_operand(ty))
            }
            Ty::Dict(record) => record
                .interface()
                .any(|(_, ty)| self.has_deferred_binary_operand(ty)),
            Ty::Func(sig) | Ty::Args(sig) | Ty::Pattern(sig) => {
                sig.inputs().any(|ty| self.has_deferred_binary_operand(ty))
                    || sig
                        .body
                        .as_ref()
                        .is_some_and(|ty| self.has_deferred_binary_operand(ty))
            }
            Ty::With(with) => {
                self.has_deferred_binary_operand(&with.sig)
                    || with
                        .with
                        .inputs()
                        .any(|ty| self.has_deferred_binary_operand(ty))
                    || with
                        .with
                        .body
                        .as_ref()
                        .is_some_and(|ty| self.has_deferred_binary_operand(ty))
            }
            Ty::Apply(apply) => {
                self.has_deferred_binary_operand(&apply.callee)
                    || apply
                        .args
                        .inputs()
                        .any(|ty| self.has_deferred_binary_operand(ty))
                    || apply
                        .args
                        .body
                        .as_ref()
                        .is_some_and(|ty| self.has_deferred_binary_operand(ty))
            }
            Ty::Select(select) => self.has_deferred_binary_operand(&select.ty),
            Ty::Unary(unary) => self.has_deferred_binary_operand(&unary.lhs),
            Ty::Binary(binary) => {
                let [lhs, rhs] = binary.operands();
                self.has_deferred_binary_operand(lhs) || self.has_deferred_binary_operand(rhs)
            }
            Ty::If(if_ty) => {
                self.has_deferred_binary_operand(&if_ty.cond)
                    || self.has_deferred_binary_operand(&if_ty.then)
                    || self.has_deferred_binary_operand(&if_ty.else_)
            }
            Ty::Let(bounds) => bounds
                .lbs
                .iter()
                .chain(bounds.ubs.iter())
                .any(|ty| self.has_deferred_binary_operand(ty)),
            Ty::Any | Ty::Boolean(_) | Ty::Builtin(_) | Ty::Value(_) => false,
        }
    }

    pub(super) fn known_bool(ty: &Ty) -> Option<bool> {
        match ty {
            Ty::Boolean(value) => *value,
            Ty::Value(ins) => match &ins.val {
                Value::Bool(value) => Some(*value),
                _ => None,
            },
            _ => None,
        }
    }

    fn check_while_loop(&mut self, while_loop: &Interned<WhileExpr>) -> Ty {
        let cond = self.check(&while_loop.cond);
        let body = self.check(&while_loop.body);
        if matches!(Self::known_bool(&cond), Some(false)) {
            return Ty::Builtin(BuiltinTy::None);
        }

        Self::loop_body_result(body, true)
    }

    fn check_for_loop(&mut self, for_loop: &Interned<ForExpr>) -> Ty {
        let iter = self.check(&for_loop.iter);
        let pattern = self.check_pattern_exp(&for_loop.pattern);

        // todo: This doesn't fully utilize the existing checkers. We have a better way
        // of implementing this check, add a constraint `array(iter) <: pattern`.
        // Note: this is not implemented yet in `TypeChecker::constrain`, so we need to
        // implement similar logic as following checking specific to loop
        // variables.
        if matches!(for_loop.pattern.as_ref(), Pattern::Simple(..)) {
            match &iter {
                Ty::Array(elem) => self.constrain(elem, &pattern),
                Ty::Tuple(elems) => {
                    for elem in elems.iter() {
                        self.constrain(elem, &pattern);
                    }
                }
                Ty::Var(var) => {
                    if let Some(bounds) = self.info.vars.get(&var.def) {
                        let lbs = bounds.bounds.bounds().read().lbs.clone();
                        for lb in lbs.iter() {
                            match lb {
                                Ty::Array(elem) => {
                                    self.constrain(elem, &pattern);
                                    self.constrain(&iter, &Ty::Array(pattern.clone().into()));
                                }
                                Ty::Tuple(elems) => {
                                    for elem in elems.iter() {
                                        self.constrain(elem, &pattern);
                                    }
                                    let tuple = Ty::Tuple(Interned::new(
                                        elems.iter().map(|_| pattern.clone()).collect::<Vec<_>>(),
                                    ));
                                    self.constrain(&iter, &tuple);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ty::Let(bounds) => {
                    for lb in bounds.lbs.iter() {
                        match lb {
                            Ty::Array(elem) => self.constrain(elem, &pattern),
                            Ty::Tuple(elems) => {
                                for elem in elems.iter() {
                                    self.constrain(elem, &pattern);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let body = self.check(&for_loop.body);

        Self::loop_body_result(body, true)
    }

    fn loop_body_result(body: Ty, maybe_zero_iterations: bool) -> Ty {
        fn normalize_control(ty: Ty) -> Ty {
            match ty {
                Ty::Builtin(BuiltinTy::Break | BuiltinTy::Continue | BuiltinTy::FlowNone) => {
                    Ty::Builtin(BuiltinTy::None)
                }
                Ty::Union(types) => Ty::from_types(
                    types
                        .iter()
                        .cloned()
                        .map(normalize_control)
                        .collect::<Vec<_>>()
                        .into_iter(),
                ),
                ty => ty,
            }
        }

        let body = normalize_control(body);
        if Self::loop_body_is_unknown_or_undef(&body) {
            return Ty::Any;
        }
        let body = Self::loop_joined_body_result(body);
        if !maybe_zero_iterations
            || matches!(body, Ty::Builtin(BuiltinTy::None))
            || matches!(&body, Ty::Unary(unary) if unary.op == UnaryOp::Return)
        {
            return body;
        }

        Ty::from_types([Ty::Builtin(BuiltinTy::None), body].into_iter())
    }

    fn loop_joined_body_result(body: Ty) -> Ty {
        match body {
            Ty::Union(types) => {
                let types = types
                    .iter()
                    .cloned()
                    .map(Self::loop_joined_body_result)
                    .collect::<Vec<_>>();
                Ty::from_types(types.into_iter())
            }
            Ty::Tuple(elems) => {
                let elem = Ty::from_types(
                    elems
                        .iter()
                        .cloned()
                        .map(|ty| ty.compact_deferred_resultant())
                        .collect::<Vec<_>>()
                        .into_iter(),
                );
                Ty::Array(elem.into())
            }
            Ty::Array(elem) => Ty::Array(elem.compact_deferred_resultant().into()),
            ty => ty,
        }
    }

    fn loop_body_is_unknown_or_undef(body: &Ty) -> bool {
        match body {
            Ty::Any | Ty::Builtin(BuiltinTy::Undef) => true,
            Ty::Union(types) => types.iter().any(Self::loop_body_is_unknown_or_undef),
            _ => false,
        }
    }

    fn check_type(&mut self, ty: &Ty) -> Ty {
        ty.clone()
    }

    pub(crate) fn check_decl(&mut self, decl: &Interned<Decl>) -> Ty {
        let v = Ty::Var(self.get_var(decl));
        match decl.kind() {
            DefKind::Reference => {
                self.constrain(&v, &Ty::Builtin(BuiltinTy::Label));
            }
            DefKind::Module => {
                let ty = if decl.is_def() {
                    Some(Ty::Builtin(BuiltinTy::Module(decl.clone())))
                } else {
                    self.ei.get_def(decl).map(|expr| self.check(&expr))
                };
                if let Some(ty) = ty {
                    self.constrain(&v, &ty);
                }
            }
            _ => {}
        }

        v
    }

    fn check_star(&mut self) -> Ty {
        Ty::Any
    }
}
