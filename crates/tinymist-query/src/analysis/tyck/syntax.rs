//! Type checking on source file

use super::*;
use crate::analysis::ParamAttrs;
use crate::docs::{SignatureDocsT, TypelessParamDocs, UntypedSymbolDocs};
use crate::syntax::{def::*, DocString, VarDoc};
use crate::ty::*;

static EMPTY_DOCSTRING: LazyLock<DocString> = LazyLock::new(DocString::default);
static EMPTY_VAR_DOC: LazyLock<VarDoc> = LazyLock::new(VarDoc::default);

impl<'a> TypeChecker<'a> {
    pub(crate) fn check_syntax(&mut self, root: &Expr) -> Option<Ty> {
        Some(match root {
            Expr::Defer(d) => self.check_defer(d),
            Expr::Seq(seq) => self.check_seq(seq),
            Expr::Array(array) => self.check_array(array),
            Expr::Dict(dict) => self.check_dict(dict),
            Expr::Args(args) => self.check_args(args),
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
    fn check_seq(&mut self, seq: &Interned<Vec<Expr>>) -> Ty {
        let mut joiner = Joiner::default();

        for child in seq.iter() {
            joiner.join(self.check(child));
        }

        joiner.finalize()
    }

    fn check_array(&mut self, array: &Interned<Vec<ArgExpr>>) -> Ty {
        let mut elements = Vec::new();

        for elem in array.iter() {
            match elem {
                ArgExpr::Pos(p) => {
                    elements.push(self.check(p));
                }
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
                }
                ArgExpr::NamedRt(..) | ArgExpr::Named(..) => unreachable!(),
            }
        }

        Ty::Tuple(elements.into())
    }

    fn check_dict(&mut self, dict: &Interned<Vec<ArgExpr>>) -> Ty {
        let mut fields = Vec::new();

        for elem in dict.iter() {
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

        Ty::Dict(RecordTy::new(fields))
    }

    fn check_args(&mut self, args: &Interned<Vec<ArgExpr>>) -> Ty {
        let mut args_res = Vec::new();
        let mut named = vec![];

        for arg in args.iter() {
            match arg {
                ArgExpr::Pos(p) => {
                    args_res.push(self.check(p));
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

    fn check_pattern_exp(&mut self, pattern: &Interned<Pattern>) -> Ty {
        self.check_pattern(None, pattern, &EMPTY_DOCSTRING)
    }

    fn check_pattern(
        &mut self,
        base: Option<&Interned<Decl>>,
        pattern: &Interned<Pattern>,
        docstring: &DocString,
    ) -> Ty {
        // todo: recursive doc constructing
        match pattern.as_ref() {
            Pattern::Expr(e) => self.check(e),
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
        pattern: &PatternSig,
        docstring: &DocString,
    ) -> (PatternTy, BTreeMap<Interned<str>, Ty>) {
        let mut pos_docs = vec![];
        let mut named_docs = BTreeMap::new();
        let mut rest_docs = None;

        let mut pos = vec![];
        let mut named = BTreeMap::new();
        let mut defaults = BTreeMap::new();
        let mut rest = None;

        // todo: combine with check_pattern
        for exp in pattern.pos.iter() {
            // pos.push(self.check_pattern(pattern, Ty::Any, docstring, root.clone()));
            let res = self.check_pattern_exp(exp);
            if let Pattern::Simple(ident) = exp.as_ref() {
                let name = ident.name().clone();

                let param_doc = docstring.get_var(&name).unwrap_or(&EMPTY_VAR_DOC);
                if let Some(annotated) = docstring.var_ty(&name) {
                    self.constrain(&res, annotated);
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
            pos.push(res);
        }

        for (decl, exp) in pattern.named.iter() {
            let name = decl.name().clone();
            let res = self.check_pattern_exp(exp);
            let var = self.get_var(decl);
            let v = Ty::Var(var.clone());
            if let Some(annotated) = docstring.var_ty(&name) {
                self.constrain(&v, annotated);
            }
            // todo: this is less efficient than v.lbs.push(exp), we may have some idea to
            // optimize it, so I put a todo here.
            self.constrain(&res, &v);
            named.insert(name.clone(), v);
            defaults.insert(name.clone(), res);

            let param_doc = docstring.get_var(&name).unwrap_or(&EMPTY_VAR_DOC);
            named_docs.insert(
                name.clone(),
                TypelessParamDocs {
                    name: name.clone(),
                    docs: param_doc.docs.clone(),
                    cano_type: (),
                    default: Some(eco_format!("{exp}")),
                    attrs: ParamAttrs::named(),
                },
            );
            self.info
                .var_docs
                .insert(decl.clone(), param_doc.to_untyped());
        }

        // todo: spread left/right
        if let Some((decl, _exp)) = &pattern.spread_right {
            let var = self.get_var(decl);
            let name = var.name.clone();
            let param_doc = docstring
                .get_var(&var.name.clone())
                .unwrap_or(&EMPTY_VAR_DOC);
            self.info
                .var_docs
                .insert(decl.clone(), param_doc.to_untyped());

            let exp = Ty::Builtin(BuiltinTy::Args);
            let v = Ty::Var(var);
            if let Some(annotated) = docstring.var_ty(&name) {
                self.constrain(&v, annotated);
            }
            self.constrain(&exp, &v);
            rest = Some(v);

            rest_docs = Some(TypelessParamDocs {
                name,
                docs: param_doc.docs.clone(),
                cano_type: (),
                default: None,
                attrs: ParamAttrs::variadic(),
            });
            // todo: ..(args)
        }

        let named: Vec<(Interned<str>, Ty)> = named.into_iter().collect();

        if let Some(base) = base {
            self.info.var_docs.insert(
                base.clone(),
                Arc::new(UntypedSymbolDocs::Function(Box::new(SignatureDocsT {
                    docs: docstring.docs.clone().unwrap_or_default(),
                    pos: pos_docs,
                    named: named_docs,
                    rest: rest_docs,
                    ret_ty: (),
                    def_docs: Default::default(),
                }))),
            );
        }

        (
            PatternTy::new(pos.into_iter(), named, None, rest, None),
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
        log::debug!("field access: {select:?}[{select_site:?}] => {ty:?}.{field:?}");

        // todo: move this to base
        let base = Ty::Select(SelectTy::new(ty.clone().into(), field.clone()));
        let mut worker = SelectFieldChecker {
            base: self,
            select_site,
            resultant: vec![base],
        };
        ty.select(&field, true, &mut worker);
        Ty::from_types(worker.resultant.into_iter())
    }

    fn check_apply(&mut self, apply: &Interned<ApplyExpr>) -> Ty {
        let args = self.check(&apply.args);
        let callee = self.check(&apply.callee);

        log::debug!("func_call: {callee:?} with {args:?}");

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

        log::debug!("check closure: {func:?} with docs {docstring:#?}");

        let (sig, defaults) = self.check_pattern_sig(Some(&def_id), &func.params, docstring);

        let body = self.check(&func.body);
        let res_ty = if let Some(annotated) = &docstring.res_ty {
            self.constrain(&body, annotated);
            Ty::Let(Interned::new(TypeBounds {
                lbs: eco_vec![body],
                ubs: eco_vec![annotated.clone()],
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
        let _selector = show.selector.as_ref().map(|sel| self.check(sel));
        // todo: infer it type by selector
        let _transform = self.check(&show.edit);

        Ty::Builtin(BuiltinTy::None)
    }

    // todo: merge with func call, and regard difference (may be here)
    fn check_set(&mut self, set: &Interned<SetExpr>) -> Ty {
        let callee = self.check(&set.target);
        let args = self.check(&set.args);
        let _cond = set.cond.as_ref().map(|cond| self.check(cond));

        log::debug!("set rule: {callee:?} with {args:?}");

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
        let of = of.or_else(|| r.val.clone());
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

    fn check_import(&mut self, _import: &Interned<ImportExpr>) -> Ty {
        Ty::Builtin(BuiltinTy::None)
    }

    fn check_include(&mut self, _include: &Interned<IncludeExpr>) -> Ty {
        Ty::Builtin(BuiltinTy::Content)
    }

    fn check_contextual(&mut self, expr: &Interned<Expr>) -> Ty {
        let body = self.check(expr);

        Ty::Unary(TypeUnary::new(UnaryOp::Context, body))
    }

    fn check_conditional(&mut self, i: &Interned<IfExpr>) -> Ty {
        let cond = self.check(&i.cond);
        let then = self.check(&i.then);
        let else_ = self.check(&i.else_);

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

    fn check_decl(&mut self, decl: &Interned<Decl>) -> Ty {
        let v = Ty::Var(self.get_var(decl));
        if let Decl::Label(..) = decl.as_ref() {
            self.constrain(&v, &Ty::Builtin(BuiltinTy::Label));
        }

        v
    }

    fn check_star(&mut self) -> Ty {
        Ty::Any
    }
}
