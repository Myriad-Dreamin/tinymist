//! Type checking on source file

use super::*;
use crate::analysis::ParamAttrs;
use crate::docs::{SignatureDocsT, TypelessParamDocs, UntypedSymbolDocs};
use crate::syntax::expr::*;
use crate::ty::*;

static EMPTY_DOCSTRING: LazyLock<DocString> = LazyLock::new(DocString::default);
static EMPTY_VAR_DOC: LazyLock<VarDoc> = LazyLock::new(VarDoc::default);

impl<'a> TypeChecker<'a> {
    pub(crate) fn check_syntax(&mut self, root: &Expr) -> Option<Ty> {
        Some(match root {
            Expr::Seq(seq) => self.check_seq(seq),
            Expr::Array(array) => self.check_array(array),
            Expr::Dict(dict) => self.check_dict(dict),
            Expr::Args(args) => self.check_args(args),
            Expr::Pattern(pattern) => self.check_pattern(pattern),
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
                ArgExpr::Named(..) => unreachable!(),
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
                ArgExpr::Spread(..) => {
                    // todo: handle spread args
                }
            }
        }

        let args = ArgsTy::new(args_res.into_iter(), named, None, None, None);

        Ty::Args(args.into())
    }

    fn check_pattern(&mut self, pattern: &Interned<Pattern>) -> Ty {
        let pos = pattern
            .pos
            .iter()
            .map(|p| self.check(p))
            .collect::<Vec<_>>();
        let named = pattern
            .named
            .iter()
            .map(|(n, v)| (n.name().clone(), self.check(v)))
            .collect::<Vec<_>>();
        let spread_left = pattern.spread_left.as_ref().map(|p| self.check(&p.1));
        let spread_right = pattern.spread_right.as_ref().map(|p| self.check(&p.1));

        // pattern: ast::Pattern<'_>,
        // value: Ty,
        // docs: &DocString,
        // root: LinkedNode<'_>,

        // let var = self.get_var(&root, ident)?;
        // let def_id = var.def;
        // let docstring = docs.get_var(&var.name).unwrap_or(&EMPTY_VAR_DOC);
        // let var = Ty::Var(var);
        // log::debug!("check pattern: {ident:?} with {value:?} and docs
        // {docstring:?}"); if let Some(annotated) = docstring.ty.as_ref() {
        //     self.constrain(&var, annotated);
        // }
        // self.constrain(&value, &var);

        // self.info.var_docs.insert(def_id, docstring.to_untyped());
        // var

        let args = PatternTy::new(pos.into_iter(), named, spread_left, spread_right, None);
        Ty::Pattern(args.into())
    }

    fn check_element(&mut self, element: &Interned<ElementExpr>) -> Ty {
        for content in element.content.iter() {
            self.check(content);
        }

        Ty::Builtin(BuiltinTy::Element(element.elem))
    }

    fn check_unary(&mut self, unary: &Interned<UnExpr>) -> Ty {
        // if let Some(constant) = self.ctx.mini_eval(ast::Expr::Unary(unary)) {
        //     return Some(Ty::Value(InsTy::new(constant)));
        // }

        let op = unary.op;
        let lhs = self.check(&unary.lhs);
        Ty::Unary(TypeUnary::new(op, lhs))
    }

    fn check_binary(&mut self, binary: &Interned<BinExpr>) -> Ty {
        // if let Some(constant) = self.ctx.mini_eval(ast::Expr::Binary(binary)) {
        //     return Some(Ty::Value(InsTy::new(constant)));
        // }

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
        // let field_access: ast::FieldAccess = root.cast()?;

        let select_site = select.key.span().unwrap_or_else(Span::detached);
        let ty = self.check(&select.lhs);
        let field = select.key.name().clone();
        log::debug!("field access: {select:?} => {ty:?}.{field:?}");

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
                // call_site: func_call.callee().span(),
                // todo: callee span
                call_site: Span::detached(),
                call_raw_for_with: Some(callee.clone()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            return Ty::from_types(worker.resultant.into_iter());
        }

        Ty::Any
    }

    fn check_func(&mut self, func: &Interned<FuncExpr>) -> Ty {
        // let closure: ast::Closure = root.cast()?;
        // let def_id = closure
        //     .name()
        //     .and_then(|n| self.get_def_id(n.span(), &to_ident_ref(&root, n)?));
        let def_id = func.decl.clone();

        // todo: docstring
        // let docstring = self.check_docstring(&root, DocStringKind::Function, def_id);
        let docstring = None::<Arc<DocString>>;
        let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        log::debug!("check closure: {:?} -> {docstring:#?}", def_id.name());

        let mut pos_docs = vec![];
        let mut named_docs = BTreeMap::new();
        let mut rest_docs = None;

        let mut pos = vec![];
        let mut named = BTreeMap::new();
        let mut defaults = BTreeMap::new();
        let mut rest = None;

        for exp in func.params.pos.iter() {
            // pos.push(self.check_pattern(pattern, Ty::Any, docstring, root.clone()));
            pos.push(self.check(exp));
            if let Expr::Decl(ident) = exp {
                let name = ident.name().clone();

                let param_doc = docstring.get_var(&name).unwrap_or(&EMPTY_VAR_DOC);
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
        }

        for (decl, exp) in func.params.named.iter() {
            let name = decl.name().clone();
            let exp = self.check(exp);
            let var = self.get_var(decl);
            let v = Ty::Var(var.clone());
            if let Some(annotated) = docstring.var_ty(&name) {
                self.constrain(&v, annotated);
            }
            // todo: this is less efficient than v.lbs.push(exp), we may have some idea to
            // optimize it, so I put a todo here.
            self.constrain(&exp, &v);
            named.insert(name.clone(), v);
            defaults.insert(name.clone(), exp);

            let param_doc = docstring.get_var(&name).unwrap_or(&EMPTY_VAR_DOC);
            named_docs.insert(
                name.clone(),
                TypelessParamDocs {
                    name: name.clone(),
                    docs: param_doc.docs.clone(),
                    cano_type: (),
                    // default: Some(e.expr().to_untyped().clone().into_text()),
                    // todo
                    default: None,
                    attrs: ParamAttrs::named(),
                },
            );
            self.info
                .var_docs
                .insert(decl.clone(), param_doc.to_untyped());
        }

        // todo: spread left/right
        if let Some((decl, _exp)) = &func.params.spread_right {
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

        self.info.var_docs.insert(
            def_id,
            Arc::new(UntypedSymbolDocs::Function(Box::new(SignatureDocsT {
                docs: docstring.docs.clone().unwrap_or_default(),
                pos: pos_docs,
                named: named_docs,
                rest: rest_docs,
                ret_ty: (),
                def_docs: Default::default(),
            }))),
        );

        let body = self.check_defer(&func.body);
        let res_ty = if let Some(annotated) = &docstring.res_ty {
            self.constrain(&body, annotated);
            Ty::Let(Interned::new(TypeBounds {
                lbs: eco_vec![body],
                ubs: eco_vec![annotated.clone()],
            }))
        } else {
            body
        };

        let named: Vec<(Interned<str>, Ty)> = named.into_iter().collect();

        // freeze the signature
        for pos in pos.iter() {
            self.weaken(pos);
        }
        for (_, named) in named.iter() {
            self.weaken(named);
        }
        if let Some(rest) = &rest {
            self.weaken(rest);
        }

        let sig = SigTy::new(pos.into_iter(), named, None, rest, Some(res_ty)).into();
        let sig = Ty::Func(sig);
        if defaults.is_empty() {
            return sig;
        }

        let defaults: Vec<(Interned<str>, Ty)> = defaults.into_iter().collect();
        let with_defaults = SigWithTy {
            sig: sig.into(),
            with: ArgsTy::new([].into_iter(), defaults, None, None, None).into(),
        };
        Ty::With(with_defaults.into())
    }

    fn check_let(&mut self, let_expr: &Interned<LetExpr>) -> Ty {
        // pub pattern: Expr,
        // pub body: Expr,

        // todo: docstring
        // let docstring = self.check_var_docs(&root);
        // let docstring = docstring.as_deref().unwrap_or(&EMPTY_DOCSTRING);

        let value = match &let_expr.body {
            Some(expr) => self.check_defer(expr),
            None => Ty::Builtin(BuiltinTy::None),
        };
        // todo
        // if let Some(annotated) = &docstring.res_ty {
        //     self.constrain(&value, annotated);
        // }
        // let value = docstring.res_ty.clone().unwrap_or(value);

        let pat = self.check(&let_expr.pattern);
        self.constrain(&value, &pat);

        // self.check_pattern(pattern, value, docstring, root.clone());

        Ty::Builtin(BuiltinTy::None)
    }

    fn check_show(&mut self, show: &Interned<ShowExpr>) -> Ty {
        let _selector = show.selector.as_ref().map(|sel| self.check(sel));
        // todo: infer it type by selector
        let _transform = self.check_defer(&show.edit);

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
        let s = r.ident.span();
        let of = r.of.as_ref().map(|of| self.check(of));
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
        let _pattern = self.check(&for_loop.pattern);
        let _body = self.check(&for_loop.body);

        Ty::Any
    }

    fn check_type(&mut self, ty: &Ty) -> Ty {
        ty.clone()
    }

    fn check_decl(&mut self, decl: &Interned<Decl>) -> Ty {
        // self.get_var(&root, root.cast()?).map(Ty::Var).or_else(|| {
        //     let s = root.span();
        //     let v = resolve_global_value(self.ctx, root, mode ==
        // InterpretMode::Math)?;     Some(Ty::Value(InsTy::new_at(v, s)))
        // })
        Ty::Any
    }

    fn check_star(&mut self) -> Ty {
        Ty::Any
    }
}
