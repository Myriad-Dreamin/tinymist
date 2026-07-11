//! Type checking on source file

use typst::foundations::{Element, Type};

use super::*;
use crate::analysis::ParamAttrs;
use crate::docs::{DocString, SignatureDocsT, TypelessParamDocs, UntypedDefDocs, VarDoc};
use crate::syntax::def::*;
use crate::ty::*;

static EMPTY_DOCSTRING: LazyLock<DocString> = LazyLock::new(DocString::default);
static EMPTY_VAR_DOC: LazyLock<VarDoc> = LazyLock::new(VarDoc::default);

impl TypeChecker<'_> {
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
        let mut joiner = Joiner::default();

        for child in exprs.iter() {
            joiner.join(self.check(child));
        }

        joiner.finalize()
    }

    fn check_array(&mut self, arr_span: Span, elems: &[ArgExpr]) -> Ty {
        let mut elements = Vec::new();

        for elem in elems.iter() {
            match elem {
                ArgExpr::Pos(pos) => {
                    elements.push(self.check(pos));
                }
                ArgExpr::Spread(spread) => {
                    let spread = self.check(spread);
                    Self::push_tuple_spread(&mut elements, spread);
                }
                ArgExpr::NamedRt(..) | ArgExpr::Named(..) => unreachable!(),
            }
        }

        let res = Ty::Tuple(elements.into());
        self.info.witness_at_most(arr_span, res.clone());
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
                ArgExpr::Spread(spread) => {
                    let spread = self.check(spread);
                    Self::push_dict_spread(&mut fields, spread);
                }
                ArgExpr::Pos(..) => unreachable!(),
            }
        }

        let res = Ty::Dict(RecordTy::new(fields));
        self.info.witness_at_most(dict_span, res.clone());
        res
    }

    fn check_args(&mut self, args: &[ArgExpr]) -> Ty {
        let mut args_res = Vec::new();
        let mut named = vec![];
        let mut rest = None;

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
                ArgExpr::Spread(spread) => {
                    let spread = self.check(spread);
                    Self::push_arg_spread(&mut args_res, &mut named, &mut rest, spread);
                }
            }
        }

        let args = ArgsTy::new(args_res.into_iter(), named, None, rest, None);

        Ty::Args(args.into())
    }

    fn push_tuple_spread(elements: &mut Vec<Ty>, spread: Ty) {
        match spread {
            Ty::Tuple(elems) => elements.extend(elems.iter().cloned()),
            Ty::Args(args) => {
                elements.extend(args.positional_params().cloned());
                if let Some(rest) = args.rest_param() {
                    Self::push_tuple_spread(elements, rest.clone());
                }
            }
            ty => elements.push(Ty::Unary(TypeUnary::new(UnaryOp::Spread, ty))),
        }
    }

    fn push_dict_spread(fields: &mut Vec<(Interned<str>, Ty)>, spread: Ty) {
        match spread {
            Ty::Dict(record) => fields.extend(
                record
                    .interface()
                    .map(|(name, ty)| (name.clone(), ty.clone())),
            ),
            Ty::Args(args) => fields.extend(
                args.named_params()
                    .map(|(name, ty)| (name.clone(), ty.clone())),
            ),
            _ => {}
        }
    }

    fn push_arg_spread(
        args_res: &mut Vec<Ty>,
        named: &mut Vec<(Interned<str>, Ty)>,
        rest: &mut Option<Ty>,
        spread: Ty,
    ) {
        match spread {
            Ty::Tuple(elems) => {
                for elem in elems.iter() {
                    if let Ty::Unary(unary) = elem
                        && unary.op == UnaryOp::Spread
                    {
                        Self::push_arg_spread(args_res, named, rest, unary.lhs.clone());
                        continue;
                    }

                    args_res.push(elem.clone());
                }
            }
            Ty::Args(args) => {
                args_res.extend(args.positional_params().cloned());
                named.extend(
                    args.named_params()
                        .map(|(name, ty)| (name.clone(), ty.clone())),
                );
                if let Some(rest_ty) = args.rest_param() {
                    *rest = Some(rest_ty.clone());
                }
            }
            Ty::Dict(record) => {
                named.extend(
                    record
                        .interface()
                        .map(|(name, ty)| (name.clone(), ty.clone())),
                );
            }
            Ty::Array(elem) => *rest = Some(Ty::Array(elem)),
            ty => *rest = Some(ty),
        }
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
        Ty::Unary(TypeUnary::new(op, lhs))
    }

    fn check_binary(&mut self, binary: &Interned<BinExpr>) -> Ty {
        let op = binary.op;
        let [lhs, rhs] = binary.operands();
        let lhs = self.check(lhs);
        let rhs = self.check(rhs);

        match op {
            ast::BinOp::Add | ast::BinOp::Sub | ast::BinOp::Mul | ast::BinOp::Div => {}
            ast::BinOp::Eq | ast::BinOp::Neq | ast::BinOp::Leq | ast::BinOp::Geq => {
                self.check_comparable(&lhs, &rhs);
                self.possible_ever_be(&lhs, &rhs);
                self.possible_ever_be(&rhs, &lhs);
            }
            ast::BinOp::Lt | ast::BinOp::Gt => {
                self.check_comparable(&lhs, &rhs);
            }
            ast::BinOp::And | ast::BinOp::Or => {
                self.constrain(&lhs, &Ty::Boolean(None));
                self.constrain(&rhs, &Ty::Boolean(None));
            }
            ast::BinOp::NotIn | ast::BinOp::In => {
                self.check_containing(&rhs, &lhs, op == ast::BinOp::In);
            }
            ast::BinOp::Assign => {
                self.check_assignable(&lhs, &rhs);
                self.constrain_assignment(&lhs, &rhs);
            }
            ast::BinOp::AddAssign
            | ast::BinOp::SubAssign
            | ast::BinOp::MulAssign
            | ast::BinOp::DivAssign => {
                self.check_assignable(&lhs, &rhs);
            }
        }

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

        Ty::Binary(TypeBinary::new(op, lhs, rhs))
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
        let res = Ty::from_types(worker.resultant.into_iter());
        self.info.witness_at_least(select_site, res.clone());
        res
    }

    fn check_apply(&mut self, apply: &Interned<ApplyExpr>) -> Ty {
        let args = self.check(&apply.args);
        let callee = self.check(&apply.callee);
        self.summarize_mutation_call(&apply.callee, &args);

        // Treat `dict.at("key")` as `dict.key` when the key is a constant string.
        if let Expr::Select(select) = &apply.callee
            && select.key.name().as_ref() == "at"
            && let Ty::Args(args_ty) = &args
            && args_ty.positional_params().len() == 1
            && args_ty.named_params().len() == 0
            && args_ty.rest_param().is_none()
        {
            let key = args_ty
                .pos(0)
                .and_then(|ty| self.unique_const_string_key(ty));

            if let Some(key) = key {
                let base = self.check(&select.lhs);
                let res = Ty::Select(SelectTy::new(base.into(), key));
                self.info.witness_at_least(apply.span, res.clone());
                return res;
            }
        }

        crate::log_debug_ct!("func_call: {callee:?} with {args:?}");

        if let Ty::Args(args) = args {
            if Self::is_builtin_panic_callee(&callee) {
                let res = Ty::Builtin(BuiltinTy::Never);
                self.info.witness_at_least(apply.span, res.clone());
                return res;
            }

            let mut worker = ApplyTypeChecker {
                base: self,
                call_site: apply.callee.span(),
                call_raw_for_with: Some(callee.clone()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            let res = Ty::from_types(worker.resultant.into_iter());
            let res = self.materialize_tuple_spreads(res);
            self.info.witness_at_least(apply.span, res.clone());
            return res;
        }

        Ty::Any
    }

    fn is_builtin_panic_callee(callee: &Ty) -> bool {
        match callee {
            Ty::Value(ins_ty) => match &ins_ty.val {
                Value::Func(func) => func.name().is_some_and(|name| name == "panic"),
                _ => false,
            },
            Ty::Union(types) => types.iter().any(Self::is_builtin_panic_callee),
            _ => false,
        }
    }

    fn is_arguments_like(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Args(_) | Ty::Builtin(BuiltinTy::Args) => true,
            Ty::Var(var) => {
                let Some(bounds) = self.info.vars.get(&var.def) else {
                    return false;
                };
                let lbs = bounds.bounds.bounds().read().lbs.clone();
                lbs.iter().any(|lb| self.is_arguments_like(lb))
            }
            Ty::Let(bounds) => bounds.lbs.iter().any(|lb| self.is_arguments_like(lb)),
            Ty::Union(types) => types.iter().any(|ty| self.is_arguments_like(ty)),
            _ => false,
        }
    }

    fn materialize_tuple_spreads(&self, ty: Ty) -> Ty {
        match ty {
            Ty::Tuple(elems) => {
                let elems = elems
                    .iter()
                    .map(|elem| {
                        if let Ty::Unary(unary) = elem
                            && unary.op == UnaryOp::Spread
                        {
                            let lhs = match &unary.lhs {
                                Ty::Var(var) if self.live_input_vars.contains(&var.def) => {
                                    unary.lhs.clone()
                                }
                                lhs => self.info.simplify(lhs.clone(), true),
                            };
                            return Ty::Unary(TypeUnary::new(UnaryOp::Spread, lhs));
                        }

                        elem.clone()
                    })
                    .collect::<Vec<_>>();

                Ty::Tuple(elems.into())
            }
            ty => ty,
        }
    }

    fn summarize_mutation_call(&mut self, callee: &Expr, args: &Ty) {
        let Expr::Select(select) = callee else {
            return;
        };
        if select.key.name().as_ref() != "push" {
            return;
        }

        let Ty::Args(args) = args else {
            return;
        };
        if args.positional_params().len() != 1
            || args.named_params().len() != 0
            || args.rest_param().is_some()
        {
            return;
        }

        let Some(elem) = args.pos(0).cloned() else {
            return;
        };
        let receiver = self.check(&select.lhs);
        self.summarize_array_push(receiver, elem);
    }

    fn summarize_array_push(&mut self, receiver: Ty, elem: Ty) {
        let Ty::Var(var) = receiver else {
            return;
        };
        let elem = self.shallow_lower_bound(elem);
        let Some(bounds) = self.info.vars.get_mut(&var.def) else {
            return;
        };

        let mut bounds = bounds.bounds.bounds().write();
        let mut elems = vec![elem];
        let mut kept_lbs = Vec::with_capacity(bounds.lbs.size());

        for lb in bounds.lbs.iter() {
            if Self::collect_array_builder_elems(lb, &mut elems) {
                continue;
            }
            kept_lbs.push(lb.clone());
        }

        let elem = Ty::from_types(elems.into_iter());
        kept_lbs.push(Ty::Array(elem.into()));
        bounds.lbs = kept_lbs.into_iter().collect();
    }

    fn collect_array_builder_elems(ty: &Ty, elems: &mut Vec<Ty>) -> bool {
        match ty {
            Ty::Array(elem) => {
                elems.push(elem.as_ref().clone());
                true
            }
            Ty::Tuple(items) => {
                for item in items.iter() {
                    if let Ty::Unary(unary) = item
                        && unary.op == UnaryOp::Spread
                    {
                        Self::collect_array_builder_elems(&unary.lhs, elems);
                    } else {
                        elems.push(item.clone());
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn shallow_lower_bound(&self, ty: Ty) -> Ty {
        match ty {
            Ty::Var(var) => {
                let Some(bounds) = self.info.vars.get(&var.def) else {
                    return Ty::Var(var);
                };
                let lbs = bounds
                    .bounds
                    .bounds()
                    .read()
                    .lbs
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                if lbs.is_empty() {
                    return Ty::Var(var);
                }
                Ty::from_types(lbs.into_iter())
            }
            Ty::Let(bounds) if !bounds.lbs.is_empty() => Ty::from_types(bounds.lbs.iter().cloned()),
            Ty::Let(bounds) => Ty::Let(bounds),
            ty => ty,
        }
    }

    fn check_func(&mut self, func: &Interned<FuncExpr>) -> Ty {
        let def_id = func.decl.clone();
        let var = Ty::Var(self.get_var(&def_id));

        let docstring = self.check_docstring(&def_id);
        let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        crate::log_debug_ct!("check closure: {func:?} with docs {docstring:#?}");

        let (sig, defaults) = self.check_pattern_sig(Some(&def_id), &func.params, docstring);
        let input_bounds = self.snapshot_function_input_bounds(&sig);
        let input_defs = input_bounds
            .iter()
            .map(|(binder, _)| binder.def.clone())
            .collect::<Vec<_>>();
        let escaped_input_vars = self.live_input_vars.clone();
        for def in &input_defs {
            self.input_contract_bounds.remove(def);
            self.live_input_vars.insert(def.clone());
        }

        let body = Self::function_return_type(self.check(&func.body));
        let res_ty = if let Some(annotated) = &docstring.res_ty {
            self.constrain(&body, annotated);
            self.function_annotated_return_type(body, annotated)
        } else {
            body
        };
        let res_ty = self.close_function_resultant_type(res_ty, &input_bounds, &escaped_input_vars);
        let sig = self.rebind_overwritten_function_inputs(sig, input_bounds, &escaped_input_vars);
        self.live_input_vars = escaped_input_vars;
        for def in input_defs {
            self.input_contract_bounds.remove(&def);
        }

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

    fn function_return_type(ty: Ty) -> Ty {
        match ty {
            Ty::Unary(unary) if unary.op == UnaryOp::Return => unary.lhs.clone(),
            Ty::If(if_ty) => Ty::If(IfTy::new(
                if_ty.cond.clone(),
                Self::function_return_type(if_ty.then.as_ref().clone()).into(),
                Self::function_return_type(if_ty.else_.as_ref().clone()).into(),
            )),
            ty => ty,
        }
    }

    fn function_annotated_return_type(&self, body: Ty, annotated: &Ty) -> Ty {
        let body_is_open = matches!(
            &body,
            Ty::Any
                | Ty::Builtin(
                    BuiltinTy::None | BuiltinTy::FlowNone | BuiltinTy::Undef | BuiltinTy::Never,
                )
        );
        if !body_is_open || !self.annotation_has_information(annotated) {
            return body;
        }

        annotated.clone()
    }

    fn annotation_has_information(&self, annotated: &Ty) -> bool {
        match annotated {
            Ty::Any => false,
            Ty::Var(var) => self.info.vars.get(&var.def).is_some_and(|bounds| {
                let bounds = bounds.bounds.bounds().read();
                !bounds.lbs.is_empty() || !bounds.ubs.is_empty()
            }),
            Ty::Let(bounds) => !bounds.lbs.is_empty() || !bounds.ubs.is_empty(),
            _ => true,
        }
    }

    fn check_let(&mut self, let_expr: &Interned<LetExpr>) -> Ty {
        // todo: consistent pattern docs
        let docstring = self.check_docstring(&Decl::pattern(let_expr.span).into());
        let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        let term = match &let_expr.body {
            Some(expr) => self.check(expr),
            None => Ty::Builtin(BuiltinTy::None),
        };
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
        let of = self
            .info
            .vars
            .contains_key(&r.decl)
            .then(|| Ty::Var(self.get_var(&r.decl)));
        let of = of.or_else(|| r.root.as_ref().map(|of| self.check(of)));
        let of = of.or_else(|| r.term.clone());
        if let Some((s, of)) = s.zip(of.as_ref()) {
            self.info.witness_at_most(s, of.clone());
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

        Ty::Unary(TypeUnary::new(UnaryOp::Context, body))
    }

    fn check_conditional(&mut self, if_expr: &Interned<IfExpr>) -> Ty {
        let cond = self.check(&if_expr.cond);
        let branch_live_input_vars = self.live_input_vars.clone();
        let then = self.check(&if_expr.then);
        let then_live_input_vars = self.live_input_vars.clone();
        self.live_input_vars = branch_live_input_vars;
        let else_ = self.check(&if_expr.else_);
        // The current value can still be the input if either branch leaves it untouched.
        self.live_input_vars.extend(then_live_input_vars);

        Ty::If(IfTy::new(cond.into(), then.into(), else_.into()))
    }

    fn check_while_loop(&mut self, while_loop: &Interned<WhileExpr>) -> Ty {
        let _cond = self.check(&while_loop.cond);
        let loop_live_input_vars = self.live_input_vars.clone();
        let _body = self.check(&while_loop.body);
        // A loop body may execute zero times.
        self.live_input_vars.extend(loop_live_input_vars);

        Ty::Any
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
                Ty::Tuple(elems) => self.constrain_tuple_iter_pattern(elems, &pattern),
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
                                    self.constrain_tuple_iter_pattern(elems, &pattern);
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
                            Ty::Tuple(elems) => self.constrain_tuple_iter_pattern(elems, &pattern),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let loop_live_input_vars = self.live_input_vars.clone();
        let _body = self.check(&for_loop.body);
        // A loop body may execute zero times.
        self.live_input_vars.extend(loop_live_input_vars);

        Ty::Any
    }

    fn constrain_tuple_iter_pattern(&mut self, elems: &[Ty], pattern: &Ty) {
        if let Some(elem) = self.tuple_iter_element_type(elems) {
            self.constrain(&elem, pattern);
        }
    }

    fn tuple_iter_element_type(&self, elems: &[Ty]) -> Option<Ty> {
        let mut types = vec![];

        for elem in elems {
            if let Ty::Unary(unary) = elem
                && unary.op == UnaryOp::Spread
            {
                if let Some(elem) = self.spread_iter_element_type(&unary.lhs) {
                    types.push(elem);
                }
                continue;
            }

            types.push(elem.clone());
        }

        (!types.is_empty()).then(|| Ty::from_types(types.into_iter()))
    }

    fn spread_iter_element_type(&self, source: &Ty) -> Option<Ty> {
        match source {
            Ty::Array(elem) => Some(elem.as_ref().clone()),
            Ty::Tuple(elems) => self.tuple_iter_element_type(elems),
            Ty::Args(args) => self.args_iter_element_type(args),
            Ty::Var(var) if self.is_arguments_like(source) => Some(Ty::Unary(TypeUnary::new(
                UnaryOp::ElementOf,
                Ty::Var(var.clone()),
            ))),
            Ty::Var(var) => {
                let bounds = self.info.vars.get(&var.def)?;
                let lbs = bounds.bounds.bounds().read().lbs.clone();
                let elems = lbs
                    .iter()
                    .filter_map(|lb| self.spread_iter_element_type(lb))
                    .collect::<Vec<_>>();
                (!elems.is_empty()).then(|| Ty::from_types(elems.into_iter()))
            }
            Ty::Let(bounds) => {
                let elems = bounds
                    .lbs
                    .iter()
                    .filter_map(|lb| self.spread_iter_element_type(lb))
                    .collect::<Vec<_>>();
                (!elems.is_empty()).then(|| Ty::from_types(elems.into_iter()))
            }
            _ => Some(Ty::Unary(TypeUnary::new(
                UnaryOp::ElementOf,
                source.clone(),
            ))),
        }
    }

    fn args_iter_element_type(&self, args: &ArgsTy) -> Option<Ty> {
        let mut elems = args.positional_params().cloned().collect::<Vec<_>>();
        if let Some(rest) = args.rest_param()
            && let Some(elem) = self.spread_iter_element_type(rest)
        {
            elems.push(elem);
        }

        (!elems.is_empty()).then(|| Ty::from_types(elems.into_iter()))
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
