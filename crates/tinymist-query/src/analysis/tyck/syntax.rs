//! Type checking on source file

use std::{collections::BTreeMap, sync::LazyLock};

use typst::{
    foundations::{IntoValue, Module, Str, Type, Value},
    syntax::{
        ast::{self, AstNode},
        LinkedNode, SyntaxKind,
    },
};

use super::*;
use crate::{
    adt::interner::Interned,
    docs::{convert_docs, identify_func_docs},
    syntax::{find_docs_of, get_non_strict_def_target},
    ty::*,
};

#[derive(Debug)]
struct ParamDoc {
    // docs: String,
    ty: Option<Ty>,
    // default: Option<String>,
}

type ParamDocs = HashMap<String, ParamDoc>;

impl<'a, 'w> TypeChecker<'a, 'w> {
    pub(crate) fn check_syntax(&mut self, root: LinkedNode) -> Option<Ty> {
        Some(match root.kind() {
            SyntaxKind::Markup => return self.check_in_mode(root, InterpretMode::Markup),
            SyntaxKind::Math => return self.check_in_mode(root, InterpretMode::Math),
            SyntaxKind::Code => return self.check_in_mode(root, InterpretMode::Code),
            SyntaxKind::CodeBlock => return self.check_in_mode(root, InterpretMode::Code),
            SyntaxKind::ContentBlock => return self.check_in_mode(root, InterpretMode::Markup),

            // todo: space effect
            SyntaxKind::Space => Ty::Builtin(BuiltinTy::Space),
            SyntaxKind::Parbreak => Ty::Builtin(BuiltinTy::Space),

            SyntaxKind::Text => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Linebreak => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Escape => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Shorthand => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::SmartQuote => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Raw => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::RawLang => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::RawDelim => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::RawTrimmed => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Link => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Label => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::Ref => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::RefMarker => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::HeadingMarker => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::EnumMarker => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::ListMarker => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::TermMarker => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::MathAlignPoint => Ty::Builtin(BuiltinTy::Content),
            SyntaxKind::MathPrimes => Ty::Builtin(BuiltinTy::Content),

            SyntaxKind::Strong => return self.check_children(root),
            SyntaxKind::Emph => return self.check_children(root),
            SyntaxKind::Heading => return self.check_children(root),
            SyntaxKind::ListItem => return self.check_children(root),
            SyntaxKind::EnumItem => return self.check_children(root),
            SyntaxKind::TermItem => return self.check_children(root),
            SyntaxKind::Equation => return self.check_children(root),
            SyntaxKind::MathDelimited => return self.check_children(root),
            SyntaxKind::MathAttach => return self.check_children(root),
            SyntaxKind::MathFrac => return self.check_children(root),
            SyntaxKind::MathRoot => return self.check_children(root),

            SyntaxKind::LoopBreak => Ty::Builtin(BuiltinTy::None),
            SyntaxKind::LoopContinue => Ty::Builtin(BuiltinTy::None),
            SyntaxKind::FuncReturn => Ty::Builtin(BuiltinTy::None),
            SyntaxKind::Error => Ty::Builtin(BuiltinTy::None),
            SyntaxKind::Eof => Ty::Builtin(BuiltinTy::None),

            SyntaxKind::None => Ty::Builtin(BuiltinTy::None),
            SyntaxKind::Auto => Ty::Builtin(BuiltinTy::Auto),
            SyntaxKind::Break => Ty::Builtin(BuiltinTy::FlowNone),
            SyntaxKind::Continue => Ty::Builtin(BuiltinTy::FlowNone),
            SyntaxKind::Return => Ty::Builtin(BuiltinTy::FlowNone),
            SyntaxKind::Ident => return self.check_ident(root, InterpretMode::Code),
            SyntaxKind::MathIdent => return self.check_ident(root, InterpretMode::Math),
            SyntaxKind::Bool
            | SyntaxKind::Int
            | SyntaxKind::Float
            | SyntaxKind::Numeric
            | SyntaxKind::Str => {
                return self
                    .ctx
                    .mini_eval(root.cast()?)
                    .map(|v| (Ty::Value(InsTy::new(v))))
            }
            SyntaxKind::Parenthesized => return self.check_children(root),
            SyntaxKind::Array => return self.check_array(root),
            SyntaxKind::Dict => return self.check_dict(root),
            SyntaxKind::Unary => return self.check_unary(root),
            SyntaxKind::Binary => return self.check_binary(root),
            SyntaxKind::FieldAccess => return self.check_field_access(root),
            SyntaxKind::FuncCall => return self.check_func_call(root),
            SyntaxKind::Args => return self.check_args(root),
            SyntaxKind::Closure => return self.check_closure(root),
            SyntaxKind::LetBinding => return self.check_let(root),
            SyntaxKind::SetRule => return self.check_set(root),
            SyntaxKind::ShowRule => return self.check_show(root),
            SyntaxKind::Contextual => return self.check_contextual(root),
            SyntaxKind::Conditional => return self.check_conditional(root),
            SyntaxKind::WhileLoop => return self.check_while_loop(root),
            SyntaxKind::ForLoop => return self.check_for_loop(root),
            SyntaxKind::ModuleImport => return self.check_module_import(root),
            SyntaxKind::ModuleInclude => return self.check_module_include(root),
            SyntaxKind::Destructuring => return self.check_destructuring(root),
            SyntaxKind::DestructAssignment => return self.check_destruct_assign(root),

            // Rest all are clauses
            SyntaxKind::LineComment => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::BlockComment => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Named => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Keyed => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Spread => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Params => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::ImportItems => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::RenamedImportItem => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Hash => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::LeftBrace => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::RightBrace => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::LeftBracket => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::RightBracket => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::LeftParen => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::RightParen => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Comma => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Semicolon => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Colon => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Star => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Underscore => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Dollar => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Plus => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Minus => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Slash => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Hat => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Prime => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Dot => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Eq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::EqEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::ExclEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Lt => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::LtEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Gt => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::GtEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::PlusEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::HyphEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::StarEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::SlashEq => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Dots => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Arrow => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Root => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Not => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::And => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Or => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Let => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Set => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Show => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Context => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::If => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Else => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::For => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::In => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::While => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Import => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::Include => Ty::Builtin(BuiltinTy::Clause),
            SyntaxKind::As => Ty::Builtin(BuiltinTy::Clause),
        })
    }

    fn check_in_mode(&mut self, root: LinkedNode, into_mode: InterpretMode) -> Option<Ty> {
        let mode = self.mode;
        self.mode = into_mode;
        let res = self.check_children(root);
        self.mode = mode;
        res
    }

    fn check_children(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let mut joiner = Joiner::default();

        for child in root.children() {
            joiner.join(self.check(child));
        }
        Some(joiner.finalize())
    }

    fn check_ident(&mut self, root: LinkedNode<'_>, mode: InterpretMode) -> Option<Ty> {
        let ident: ast::Ident = root.cast()?;
        let ident_ref = IdentRef {
            name: ident.get().clone(),
            range: root.range(),
        };

        self.get_var(root.span(), ident_ref).or_else(|| {
            let s = root.span();
            let v = resolve_global_value(self.ctx, root, mode == InterpretMode::Math)?;
            Some(Ty::Value(InsTy::new_at(v, s)))
        })
    }

    fn check_array(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let _arr: ast::Array = root.cast()?;

        let mut elements = Vec::new();

        for elem in root.children() {
            let ty = self.check(elem);
            if matches!(ty, Ty::Builtin(BuiltinTy::Clause | BuiltinTy::Space)) {
                continue;
            }
            elements.push(ty);
        }

        Some(Ty::Tuple(elements.into()))
    }

    fn check_dict(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let dict: ast::Dict = root.cast()?;

        let mut fields = Vec::new();

        for field in dict.items() {
            match field {
                ast::DictItem::Named(n) => {
                    let name = n.name().into();
                    let value = self.check_expr_in(n.expr().span(), root.clone());
                    fields.push((name, value, n.span()));
                }
                ast::DictItem::Keyed(k) => {
                    let key = self.ctx.const_eval(k.key());
                    if let Some(Value::Str(key)) = key {
                        let value = self.check_expr_in(k.expr().span(), root.clone());
                        fields.push((key.into(), value, k.span()));
                    }
                }
                // todo: var dict union
                ast::DictItem::Spread(_s) => {}
            }
        }

        Some(Ty::Dict(RecordTy::new(fields)))
    }

    fn check_unary(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let unary: ast::Unary = root.cast()?;

        if let Some(constant) = self.ctx.mini_eval(ast::Expr::Unary(unary)) {
            return Some(Ty::Value(InsTy::new(constant)));
        }

        let op = unary.op();

        let lhs = self.check_expr_in(unary.expr().span(), root).into();
        let op = match op {
            ast::UnOp::Pos => UnaryOp::Pos,
            ast::UnOp::Neg => UnaryOp::Neg,
            ast::UnOp::Not => UnaryOp::Not,
        };

        Some(Ty::Unary(TypeUnary::new(op, lhs)))
    }

    fn check_binary(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let binary: ast::Binary = root.cast()?;

        if let Some(constant) = self.ctx.mini_eval(ast::Expr::Binary(binary)) {
            return Some(Ty::Value(InsTy::new(constant)));
        }

        let op = binary.op();
        let lhs_span = binary.lhs().span();
        let lhs = self.check_expr_in(lhs_span, root.clone());
        let rhs_span = binary.rhs().span();
        let rhs = self.check_expr_in(rhs_span, root);

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

        let res = Ty::Binary(TypeBinary::new(op, lhs.into(), rhs.into()));

        Some(res)
    }

    fn check_field_access(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let field_access: ast::FieldAccess = root.cast()?;

        let select_site = field_access.target().span();
        let ty = self.check_expr_in(select_site, root.clone());
        let field = Interned::new_str(field_access.field().get());

        // todo: move this to base
        let base = Ty::Select(SelectTy::new(ty.clone().into(), field.clone()));
        let mut worker = SelectFieldChecker {
            base: self,
            select_site,
            key: &field,
            resultant: vec![base],
        };
        ty.select(&field, true, &mut worker);
        Some(Ty::from_types(worker.resultant.into_iter()))
    }

    fn check_func_call(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let func_call: ast::FuncCall = root.cast()?;

        let args = self.check_expr_in(func_call.args().span(), root.clone());
        let callee = self.check_expr_in(func_call.callee().span(), root.clone());

        log::debug!("func_call: {callee:?} with {args:?}");

        if let Ty::Args(args) = args {
            let mut worker = ApplyTypeChecker {
                base: self,
                call_site: func_call.callee().span(),
                args: Some(func_call.args()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            return Some(Ty::from_types(worker.resultant.into_iter()));
        }

        None
    }

    fn check_args(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let args: ast::Args = root.cast()?;

        let mut args_res = Vec::new();
        let mut named = vec![];

        for arg in args.items() {
            match arg {
                ast::Arg::Pos(e) => {
                    args_res.push(self.check_expr_in(e.span(), root.clone()));
                }
                ast::Arg::Named(n) => {
                    let value = self.check_expr_in(n.expr().span(), root.clone());
                    named.push((n.name().into(), value));
                }
                // todo
                ast::Arg::Spread(_w) => {}
            }
        }

        let args = ArgsTy::new(args_res, named, None, None);

        Some(Ty::Args(args.into()))
    }

    fn check_closure(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let closure: ast::Closure = root.cast()?;

        let docs = None.or_else(|| {
            // use parent of params, todo: reliable way to get the def target
            let def = get_non_strict_def_target(root.clone())?;
            find_docs_of(&self.source, def)
        });

        let parsed = docs.and_then(|docs| {
            let documenting_id = closure
                .name()
                .and_then(|n| self.get_def_id(n.span(), &to_ident_ref(&root, n)?))?;

            let converted = convert_docs(self.ctx.world(), &docs).ok()?;
            let converted = identify_func_docs(&converted).ok()?;
            let module = self.ctx.module_by_str(docs)?;

            // Wrap a docs scope
            self.with_docs_scope(|this| {
                this.documenting_id = Some(documenting_id);
                let mut params = ParamDocs::new();
                for param in converted.params.into_iter() {
                    params.insert(
                        param.name,
                        ParamDoc {
                            // docs: param.docs,
                            ty: this.check_doc_types(&module, param.types),
                            // default: param.default,
                        },
                    );
                }

                Some((
                    Some(converted.docs),
                    params,
                    converted
                        .return_ty
                        .map(|ty| this.check_doc_types(&module, ty)),
                ))
            })
        });
        let (_docs, param_docs, _ret) = parsed.unwrap_or_default();

        log::debug!("check closure: {:?} -> {param_docs:#?}", closure.name());

        let mut pos = vec![];
        let mut named = BTreeMap::new();
        let mut defaults = BTreeMap::new();
        let mut rest = None;

        for param in closure.params().children() {
            match param {
                ast::Param::Pos(pattern) => {
                    pos.push(self.check_pattern(pattern, Ty::Any, &param_docs, root.clone()));
                }
                ast::Param::Named(e) => {
                    let name = e.name().get();
                    let exp = self.check_expr_in(e.expr().span(), root.clone());
                    let v = self.get_var(e.name().span(), to_ident_ref(&root, e.name())?)?;
                    let anno = param_docs.get(name.as_str()).and_then(|p| p.ty.clone());
                    log::debug!("check closure param: {name} with {exp:?} and annotation {anno:?}");
                    if let Some(anno) = anno {
                        self.constrain(&v, &anno);
                    }
                    // todo: this is less efficient than v.lbs.push(exp), we may have some idea to
                    // optimize it, so I put a todo here.
                    self.constrain(&exp, &v);
                    named.insert(name.into(), v);
                    defaults.insert(name.into(), exp);
                }
                // todo: spread left/right
                ast::Param::Spread(a) => {
                    if let Some(e) = a.sink_ident() {
                        let exp = Ty::Builtin(BuiltinTy::Args);
                        let v = self.get_var(e.span(), to_ident_ref(&root, e)?)?;
                        let anno = param_docs.get(e.get().as_str()).and_then(|p| p.ty.clone());
                        if let Some(anno) = anno {
                            self.constrain(&v, &anno);
                        }
                        self.constrain(&exp, &v);
                        rest = Some(v);
                    }
                    // todo: ..(args)
                }
            }
        }

        let body = self.check_expr_in(closure.body().span(), root);

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

        let sig = SigTy::new(pos, named, rest, Some(body)).into();
        let sig = Ty::Func(sig);
        if defaults.is_empty() {
            return Some(sig);
        }

        let defaults: Vec<(Interned<str>, Ty)> = defaults.into_iter().collect();
        let with_defaults = SigWithTy {
            sig: sig.into(),
            with: ArgsTy::new(vec![], defaults, None, None).into(),
        };
        Some(Ty::With(with_defaults.into()))
    }

    fn check_let(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let let_binding: ast::LetBinding = root.cast()?;

        match let_binding.kind() {
            ast::LetBindingKind::Closure(c) => {
                // let _name = let_binding.name().get().to_string();
                let value = let_binding
                    .init()
                    .map(|init| self.check_expr_in(init.span(), root.clone()))
                    .unwrap_or_else(|| Ty::Builtin(BuiltinTy::Infer));

                let v = self.get_var(c.span(), to_ident_ref(&root, c)?)?;
                self.constrain(&value, &v);
                // todo lbs is the lexical signature.
            }
            ast::LetBindingKind::Normal(pattern) => {
                // let _name = let_binding.name().get().to_string();
                let value = let_binding
                    .init()
                    .map(|init| self.check_expr_in(init.span(), root.clone()))
                    .unwrap_or_else(|| Ty::Builtin(BuiltinTy::Infer));

                self.check_pattern(pattern, value, &ParamDocs::default(), root.clone());
            }
        }

        Some(Ty::Any)
    }

    // todo: merge with func call, and regard difference (may be here)
    fn check_set(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let set_rule: ast::SetRule = root.cast()?;

        let callee = self.check_expr_in(set_rule.target().span(), root.clone());
        let args = self.check_expr_in(set_rule.args().span(), root.clone());
        let _cond = set_rule
            .condition()
            .map(|cond| self.check_expr_in(cond.span(), root.clone()));

        log::debug!("set rule: {callee:?} with {args:?}");

        if let Ty::Args(args) = args {
            let mut worker = ApplyTypeChecker {
                base: self,
                call_site: set_rule.target().span(),
                args: Some(set_rule.args()),
                resultant: vec![],
            };
            callee.call(&args, true, &mut worker);
            return Some(Ty::from_types(worker.resultant.into_iter()));
        }

        None
    }

    fn check_show(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let show_rule: ast::ShowRule = root.cast()?;

        let _selector = show_rule
            .selector()
            .map(|sel| self.check_expr_in(sel.span(), root.clone()));
        let t = show_rule.transform();
        // todo: infer it type by selector
        let _transform = self.check_expr_in(t.span(), root.clone());

        Some(Ty::Any)
    }

    // currently we do nothing on contextual
    fn check_contextual(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let contextual: ast::Contextual = root.cast()?;

        let body = self.check_expr_in(contextual.body().span(), root);

        Some(Ty::Unary(TypeUnary::new(UnaryOp::Context, body.into())))
    }

    fn check_conditional(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let conditional: ast::Conditional = root.cast()?;

        let cond = self.check_expr_in(conditional.condition().span(), root.clone());
        let then = self.check_expr_in(conditional.if_body().span(), root.clone());
        let else_ = conditional
            .else_body()
            .map(|else_body| self.check_expr_in(else_body.span(), root.clone()))
            .unwrap_or(Ty::Builtin(BuiltinTy::None));

        Some(Ty::If(IfTy::new(cond.into(), then.into(), else_.into())))
    }

    fn check_while_loop(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let while_loop: ast::WhileLoop = root.cast()?;

        let _cond = self.check_expr_in(while_loop.condition().span(), root.clone());
        let _body = self.check_expr_in(while_loop.body().span(), root);

        Some(Ty::Any)
    }

    fn check_for_loop(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let for_loop: ast::ForLoop = root.cast()?;

        let _iter = self.check_expr_in(for_loop.iterable().span(), root.clone());
        let _pattern = self.check_expr_in(for_loop.pattern().span(), root.clone());
        let _body = self.check_expr_in(for_loop.body().span(), root);

        Some(Ty::Any)
    }

    fn check_module_import(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let _module_import: ast::ModuleImport = root.cast()?;

        // check all import items

        Some(Ty::Builtin(BuiltinTy::None))
    }

    fn check_module_include(&mut self, _root: LinkedNode<'_>) -> Option<Ty> {
        Some(Ty::Builtin(BuiltinTy::Content))
    }

    fn check_destructuring(&mut self, _root: LinkedNode<'_>) -> Option<Ty> {
        Some(Ty::Any)
    }

    fn check_destruct_assign(&mut self, _root: LinkedNode<'_>) -> Option<Ty> {
        Some(Ty::Builtin(BuiltinTy::None))
    }
    fn check_expr_in(&mut self, span: Span, root: LinkedNode<'_>) -> Ty {
        root.find(span)
            .map(|node| self.check(node))
            .unwrap_or(Ty::Builtin(BuiltinTy::Undef))
    }

    fn check_pattern(
        &mut self,
        pattern: ast::Pattern<'_>,
        value: Ty,
        param_docs: &ParamDocs,
        root: LinkedNode<'_>,
    ) -> Ty {
        self.check_pattern_(pattern, value, param_docs, root)
            .unwrap_or(Ty::Builtin(BuiltinTy::Undef))
    }

    fn check_pattern_(
        &mut self,
        pattern: ast::Pattern<'_>,
        value: Ty,
        param_docs: &ParamDocs,
        root: LinkedNode<'_>,
    ) -> Option<Ty> {
        Some(match pattern {
            ast::Pattern::Normal(ast::Expr::Ident(ident)) => {
                let v = self.get_var(ident.span(), to_ident_ref(&root, ident)?)?;
                let anno = param_docs
                    .get(ident.get().as_str())
                    .and_then(|p| p.ty.clone());
                log::debug!("check pattern: {ident:?} with {value:?} and annotation {anno:?}");
                if let Some(anno) = anno {
                    self.constrain(&v, &anno);
                }
                self.constrain(&value, &v);
                v
            }
            ast::Pattern::Normal(_) => Ty::Any,
            ast::Pattern::Placeholder(_) => Ty::Any,
            ast::Pattern::Parenthesized(exp) => {
                self.check_pattern(exp.pattern(), value, param_docs, root)
            }
            // todo: pattern
            ast::Pattern::Destructuring(_destruct) => Ty::Any,
        })
    }

    fn check_doc_types(&mut self, m: &Module, strs: String) -> Option<Ty> {
        let mut types = vec![];
        for name in strs.split(",").map(|e| e.trim()) {
            let Some(ty) = self.check_doc_type_ident(m, name) else {
                continue;
            };
            types.push(ty);
        }

        Some(Ty::from_types(types.into_iter()))
    }

    fn check_doc_type_ident(&mut self, m: &Module, name: &str) -> Option<Ty> {
        static TYPE_REPRS: LazyLock<HashMap<&'static str, Ty>> = LazyLock::new(|| {
            let values = Vec::from_iter(
                [
                    Value::None,
                    Value::Auto,
                    // Value::Bool(Default::default()),
                    Value::Int(Default::default()),
                    Value::Float(Default::default()),
                    Value::Length(Default::default()),
                    Value::Angle(Default::default()),
                    Value::Ratio(Default::default()),
                    Value::Relative(Default::default()),
                    Value::Fraction(Default::default()),
                    Value::Str(Default::default()),
                ]
                .map(|v| v.ty())
                .into_iter()
                .chain([
                    Type::of::<typst::visualize::Color>(),
                    Type::of::<typst::visualize::Gradient>(),
                    Type::of::<typst::visualize::Pattern>(),
                    Type::of::<typst::symbols::Symbol>(),
                    Type::of::<typst::foundations::Version>(),
                    Type::of::<typst::foundations::Bytes>(),
                    Type::of::<typst::foundations::Label>(),
                    Type::of::<typst::foundations::Datetime>(),
                    Type::of::<typst::foundations::Duration>(),
                    Type::of::<typst::foundations::Content>(),
                    Type::of::<typst::foundations::Styles>(),
                    Type::of::<typst::foundations::Array>(),
                    Type::of::<typst::foundations::Dict>(),
                    Type::of::<typst::foundations::Func>(),
                    Type::of::<typst::foundations::Args>(),
                    Type::of::<typst::foundations::Type>(),
                    Type::of::<typst::foundations::Module>(),
                ]),
            );

            let shorts = values
                .clone()
                .into_iter()
                .map(|ty| (ty.short_name(), Ty::Builtin(BuiltinTy::Type(ty))));
            let longs = values
                .into_iter()
                .map(|ty| (ty.long_name(), Ty::Builtin(BuiltinTy::Type(ty))));
            let builtins = [
                ("any", Ty::Any),
                ("bool", Ty::Boolean(None)),
                ("boolean", Ty::Boolean(None)),
                ("false", Ty::Boolean(Some(false))),
                ("true", Ty::Boolean(Some(true))),
            ];
            HashMap::from_iter(shorts.chain(longs).chain(builtins))
        });

        let builtin_ty = TYPE_REPRS.get(name).cloned();
        builtin_ty.or_else(|| self.check_doc_type_anno(m, name))
    }

    fn check_doc_type_anno(&mut self, m: &Module, name: &str) -> Option<Ty> {
        if let Some(v) = self.docs_scope.get(name) {
            return v.clone();
        }

        let v = m.scope().get(name)?;
        log::debug!("check doc type annotation: {name:?}");
        if let Value::Content(c) = v {
            let anno = c.clone().unpack::<typst::text::RawElem>().ok()?;
            let text = anno.text().clone().into_value().cast::<Str>().ok()?;
            let code = typst::syntax::parse_code(&text.as_str().replace('\'', "θ"));
            let mut exprs = code.cast::<ast::Code>()?.exprs();
            let ret = self.check_doc_type_expr(m, exprs.next()?);
            self.docs_scope.insert(name.into(), ret.clone());
            ret
        } else {
            None
        }
    }

    fn check_doc_type_expr(&mut self, m: &Module, s: ast::Expr) -> Option<Ty> {
        log::debug!("check doc type expr: {s:?}");
        match s {
            ast::Expr::Ident(i) => self.check_doc_type_ident(m, i.get().as_str()),
            ast::Expr::Closure(c) => {
                log::debug!("check doc closure annotation: {c:?}");
                let mut pos = vec![];
                let mut named = BTreeMap::new();
                let mut rest = None;

                for param in c.params().children() {
                    match param {
                        ast::Param::Pos(ast::Pattern::Normal(ast::Expr::Ident(i))) => {
                            let base_ty = self.docs_scope.get(i.get().as_str()).cloned();
                            pos.push(base_ty.flatten().unwrap_or(Ty::Any));
                        }
                        ast::Param::Pos(_) => {
                            pos.push(Ty::Any);
                        }
                        ast::Param::Named(e) => {
                            let exp = self.check_doc_type_expr(m, e.expr()).unwrap_or(Ty::Any);
                            named.insert(e.name().into(), exp);
                        }
                        // todo: spread left/right
                        ast::Param::Spread(s) => {
                            let Some(i) = s.sink_ident() else {
                                continue;
                            };
                            let name = i.get().clone();
                            let rest_ty = self
                                .docs_scope
                                .get(i.get().as_str())
                                .cloned()
                                .flatten()
                                .unwrap_or_else(|| {
                                    self.generate_var(
                                        name.as_str().into(),
                                        self.documenting_id.unwrap(),
                                    )
                                });
                            self.docs_scope.insert(name, Some(rest_ty.clone()));
                            rest = Some(rest_ty);
                        }
                    }
                }

                let body = self.check_doc_type_expr(m, c.body())?;
                let sig = SigTy::new(pos, named, rest, Some(body)).into();

                Some(Ty::Func(sig))
            }
            ast::Expr::Dict(d) => {
                log::debug!("check doc dict annotation: {d:?}");
                None
            }
            _ => None,
        }
    }
}
