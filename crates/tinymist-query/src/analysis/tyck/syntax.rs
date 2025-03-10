//! Type checking on source file

use typst::foundations::{Element, Type};

use super::*;
use crate::analysis::ParamAttrs;
use crate::docs::{SignatureDocsT, TypelessParamDocs, UntypedDefDocs};
use crate::syntax::{def::*, DocString, VarDoc};
use crate::ty::*;

static EMPTY_DOCSTRING: LazyLock<DocString> = LazyLock::new(DocString::default);
static EMPTY_VAR_DOC: LazyLock<VarDoc> = LazyLock::new(VarDoc::default);

impl TypeChecker<'_> {
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
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
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
                ArgExpr::NamedRt(_n) => {
                    // todo: handle non constant keys
                }
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
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
                ArgExpr::NamedRt(_n) => {
                    // todo: handle non constant keys
                }
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
                }
            }
        }

        let args = ArgsTy::new(args_res.into_iter(), named, None, None, None);

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

        Ty::Builtin(BuiltinTy::Element(element.elem))
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
                self.possible_ever_be(&lhs, &rhs);
            }
            ast::BinOp::AddAssign
            | ast::BinOp::SubAssign
            | ast::BinOp::MulAssign
            | ast::BinOp::DivAssign => {
                self.check_assignable(&lhs, &rhs);
            }
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

        crate::log_debug_ct!("func_call: {callee:?} with {args:?}");

        if let Ty::Args(args) = args {
            let mut worker = ApplyTypeChecker {
                base: self,
                call_site: apply.callee.span(),
                call_raw_for_with: Some(callee.clone()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            let res = Ty::from_types(worker.resultant.into_iter());
            self.info.witness_at_least(apply.span, res.clone());
            return res;
        }

        Ty::Any
    }

    fn check_func(&mut self, func: &Interned<FuncExpr>) -> Ty {
        let def_id = func.decl.clone();
        let var = Ty::Var(self.get_var(&def_id));

        let docstring = self.check_docstring(&def_id);
        let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        crate::log_debug_ct!("check closure: {func:?} with docs {docstring:#?}");

        let (sig, defaults) = self.check_pattern_sig(Some(&def_id), &func.params, docstring);

        let body = self.check(&func.body);
        let res_ty = if let Some(annotated) = &docstring.res_ty {
            self.constrain(&body, annotated);
            Ty::Let(Interned::new(TypeBounds {
                lbs: vec![body],
                ubs: vec![annotated.clone()],
            }))
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
            None => Ty::Builtin(BuiltinTy::Content),
        };

        let show_fact = Ty::Func(SigTy::unary(selected, Ty::Any));
        crate::log_debug_ct!("check show_fact type {show_fact:?} value: {transform:?}");
        self.constrain(&transform, &show_fact);

        Some(())
    }

    fn content_by_selector(selector: Ty) -> Option<Ty> {
        crate::log_debug_ct!("check selector {selector:?}");

        Some(match selector {
            Ty::With(with) => return Self::content_by_selector(with.sig.as_ref().clone()),
            Ty::Builtin(BuiltinTy::Type(ty)) => {
                if ty == Type::of::<typst::foundations::Regex>() {
                    Ty::Builtin(BuiltinTy::Element(Element::of::<typst::text::TextElem>()))
                } else {
                    return None;
                }
            }
            Ty::Builtin(BuiltinTy::Element(..)) => selector,
            Ty::Value(ins_ty) => match &ins_ty.val {
                Value::Str(..) => {
                    Ty::Builtin(BuiltinTy::Element(Element::of::<typst::text::TextElem>()))
                }
                Value::Content(c) => Ty::Builtin(BuiltinTy::Element(c.elem())),
                Value::Func(f) => {
                    if let Some(elem) = f.element() {
                        Ty::Builtin(BuiltinTy::Element(elem))
                    } else {
                        return None;
                    }
                }
                Value::Dyn(value) => {
                    if value.ty() == Type::of::<typst::foundations::Regex>() {
                        Ty::Builtin(BuiltinTy::Element(Element::of::<typst::text::TextElem>()))
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
        let of = r.root.as_ref().map(|of| self.check(of));
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
        Ty::Builtin(BuiltinTy::Content)
    }

    fn check_import(&mut self, import: &Interned<ImportExpr>) -> Ty {
        self.check_ref(&import.decl);
        Ty::Builtin(BuiltinTy::None)
    }

    fn check_include(&mut self, _include: &Interned<IncludeExpr>) -> Ty {
        Ty::Builtin(BuiltinTy::Content)
    }

    fn check_contextual(&mut self, expr: &Interned<Expr>) -> Ty {
        let body = self.check(expr);

        Ty::Unary(TypeUnary::new(UnaryOp::Context, body))
    }

    fn check_conditional(&mut self, if_expr: &Interned<IfExpr>) -> Ty {
        let cond = self.check(&if_expr.cond);
        let then = self.check(&if_expr.then);
        let else_ = self.check(&if_expr.else_);

        Ty::If(IfTy::new(cond.into(), then.into(), else_.into()))
    }

    fn check_while_loop(&mut self, while_loop: &Interned<WhileExpr>) -> Ty {
        let _cond = self.check(&while_loop.cond);
        let _body = self.check(&while_loop.body);

        Ty::Any
    }

    fn check_for_loop(&mut self, for_loop: &Interned<ForExpr>) -> Ty {
        let _iter = self.check(&for_loop.iter);
        let _pattern = self.check_pattern_exp(&for_loop.pattern);
        let _body = self.check(&for_loop.body);

        Ty::Any
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
