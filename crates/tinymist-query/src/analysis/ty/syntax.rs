//! Type checking on source file

use std::collections::BTreeMap;

use ecow::EcoString;
use typst::{
    foundations::Value,
    syntax::{
        ast::{self, AstNode},
        LinkedNode, SyntaxKind,
    },
};

use super::*;
use crate::{adt::interner::Interned, ty::*};

impl<'a, 'w> TypeChecker<'a, 'w> {
    pub(crate) fn check_syntax(&mut self, root: LinkedNode) -> Option<Ty> {
        Some(match root.kind() {
            SyntaxKind::Markup => return self.check_in_mode(root, InterpretMode::Markup),
            SyntaxKind::Math => return self.check_in_mode(root, InterpretMode::Math),
            SyntaxKind::Code => return self.check_in_mode(root, InterpretMode::Code),
            SyntaxKind::CodeBlock => return self.check_in_mode(root, InterpretMode::Code),
            SyntaxKind::ContentBlock => return self.check_in_mode(root, InterpretMode::Markup),

            // todo: space effect
            SyntaxKind::Space => Ty::Space,
            SyntaxKind::Parbreak => Ty::Space,

            SyntaxKind::Text => Ty::Content,
            SyntaxKind::Linebreak => Ty::Content,
            SyntaxKind::Escape => Ty::Content,
            SyntaxKind::Shorthand => Ty::Content,
            SyntaxKind::SmartQuote => Ty::Content,
            SyntaxKind::Raw => Ty::Content,
            SyntaxKind::RawLang => Ty::Content,
            SyntaxKind::RawDelim => Ty::Content,
            SyntaxKind::RawTrimmed => Ty::Content,
            SyntaxKind::Link => Ty::Content,
            SyntaxKind::Label => Ty::Content,
            SyntaxKind::Ref => Ty::Content,
            SyntaxKind::RefMarker => Ty::Content,
            SyntaxKind::HeadingMarker => Ty::Content,
            SyntaxKind::EnumMarker => Ty::Content,
            SyntaxKind::ListMarker => Ty::Content,
            SyntaxKind::TermMarker => Ty::Content,
            SyntaxKind::MathAlignPoint => Ty::Content,
            SyntaxKind::MathPrimes => Ty::Content,

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

            SyntaxKind::LoopBreak => Ty::None,
            SyntaxKind::LoopContinue => Ty::None,
            SyntaxKind::FuncReturn => Ty::None,
            SyntaxKind::Error => Ty::None,
            SyntaxKind::Eof => Ty::None,

            SyntaxKind::None => Ty::None,
            SyntaxKind::Auto => Ty::Auto,
            SyntaxKind::Break => Ty::FlowNone,
            SyntaxKind::Continue => Ty::FlowNone,
            SyntaxKind::Return => Ty::FlowNone,
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
            SyntaxKind::LineComment => Ty::Clause,
            SyntaxKind::BlockComment => Ty::Clause,
            SyntaxKind::Named => Ty::Clause,
            SyntaxKind::Keyed => Ty::Clause,
            SyntaxKind::Spread => Ty::Clause,
            SyntaxKind::Params => Ty::Clause,
            SyntaxKind::ImportItems => Ty::Clause,
            SyntaxKind::RenamedImportItem => Ty::Clause,
            SyntaxKind::Hash => Ty::Clause,
            SyntaxKind::LeftBrace => Ty::Clause,
            SyntaxKind::RightBrace => Ty::Clause,
            SyntaxKind::LeftBracket => Ty::Clause,
            SyntaxKind::RightBracket => Ty::Clause,
            SyntaxKind::LeftParen => Ty::Clause,
            SyntaxKind::RightParen => Ty::Clause,
            SyntaxKind::Comma => Ty::Clause,
            SyntaxKind::Semicolon => Ty::Clause,
            SyntaxKind::Colon => Ty::Clause,
            SyntaxKind::Star => Ty::Clause,
            SyntaxKind::Underscore => Ty::Clause,
            SyntaxKind::Dollar => Ty::Clause,
            SyntaxKind::Plus => Ty::Clause,
            SyntaxKind::Minus => Ty::Clause,
            SyntaxKind::Slash => Ty::Clause,
            SyntaxKind::Hat => Ty::Clause,
            SyntaxKind::Prime => Ty::Clause,
            SyntaxKind::Dot => Ty::Clause,
            SyntaxKind::Eq => Ty::Clause,
            SyntaxKind::EqEq => Ty::Clause,
            SyntaxKind::ExclEq => Ty::Clause,
            SyntaxKind::Lt => Ty::Clause,
            SyntaxKind::LtEq => Ty::Clause,
            SyntaxKind::Gt => Ty::Clause,
            SyntaxKind::GtEq => Ty::Clause,
            SyntaxKind::PlusEq => Ty::Clause,
            SyntaxKind::HyphEq => Ty::Clause,
            SyntaxKind::StarEq => Ty::Clause,
            SyntaxKind::SlashEq => Ty::Clause,
            SyntaxKind::Dots => Ty::Clause,
            SyntaxKind::Arrow => Ty::Clause,
            SyntaxKind::Root => Ty::Clause,
            SyntaxKind::Not => Ty::Clause,
            SyntaxKind::And => Ty::Clause,
            SyntaxKind::Or => Ty::Clause,
            SyntaxKind::Let => Ty::Clause,
            SyntaxKind::Set => Ty::Clause,
            SyntaxKind::Show => Ty::Clause,
            SyntaxKind::Context => Ty::Clause,
            SyntaxKind::If => Ty::Clause,
            SyntaxKind::Else => Ty::Clause,
            SyntaxKind::For => Ty::Clause,
            SyntaxKind::In => Ty::Clause,
            SyntaxKind::While => Ty::Clause,
            SyntaxKind::Import => Ty::Clause,
            SyntaxKind::Include => Ty::Clause,
            SyntaxKind::As => Ty::Clause,
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
            name: ident.get().to_string(),
            range: root.range(),
        };

        let Some(var) = self.get_var(root.span(), ident_ref) else {
            let s = root.span();
            let v = resolve_global_value(self.ctx, root, mode == InterpretMode::Math)?;
            return Some(Ty::Value(InsTy::new_at(v, s)));
        };

        Some(var.as_type())
    }

    fn check_array(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let _arr: ast::Array = root.cast()?;

        let mut elements = Vec::new();

        for elem in root.children() {
            let ty = self.check(elem);
            if matches!(ty, Ty::Clause | Ty::Space) {
                continue;
            }
            elements.push(ty);
        }

        Some(Ty::Tuple(Interned::new(elements)))
    }

    fn check_dict(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let dict: ast::Dict = root.cast()?;

        let mut fields = Vec::new();

        for field in dict.items() {
            match field {
                ast::DictItem::Named(n) => {
                    let name = n.name().get().clone();
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

        let lhs = Interned::new(self.check_expr_in(unary.expr().span(), root));
        let op = match op {
            ast::UnOp::Pos => UnaryOp::Pos,
            ast::UnOp::Neg => UnaryOp::Neg,
            ast::UnOp::Not => UnaryOp::Not,
        };

        Some(Ty::Unary(Interned::new(TypeUnary { op, lhs })))
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

        let res = Ty::Binary(Interned::new(TypeBinary {
            op,
            operands: Interned::new((lhs, rhs)),
        }));

        Some(res)
    }

    fn check_field_access(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let field_access: ast::FieldAccess = root.cast()?;

        let ty = self.check_expr_in(field_access.target().span(), root.clone());
        let field = field_access.field().get().clone();

        Some(Ty::Select(Interned::new(SelectTy {
            ty: Interned::new(ty),
            select: Interned::new_str(&field),
        })))
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
                args: func_call.args(),
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
                    let name = n.name().get().clone();
                    let value = self.check_expr_in(n.expr().span(), root.clone());
                    named.push((name, value));
                }
                // todo
                ast::Arg::Spread(_w) => {}
            }
        }

        let args = ArgsTy::new(args_res.into_iter(), named.into_iter(), None, None);

        Some(Ty::Args(Interned::new(args)))
    }

    fn check_closure(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let closure: ast::Closure = root.cast()?;

        // let _params = self.check_expr_in(closure.params().span(), root.clone());

        let mut pos = vec![];
        let mut named = BTreeMap::new();
        let mut rest = None;

        for param in closure.params().children() {
            match param {
                ast::Param::Pos(pattern) => {
                    pos.push(self.check_pattern(pattern, Ty::Any, root.clone()));
                }
                ast::Param::Named(e) => {
                    let exp = self.check_expr_in(e.expr().span(), root.clone());
                    let v = self.get_var(e.name().span(), to_ident_ref(&root, e.name())?)?;
                    v.ever_be(exp);
                    named.insert(e.name().get().clone(), v.as_type());
                }
                // todo: spread left/right
                ast::Param::Spread(a) => {
                    if let Some(e) = a.sink_ident() {
                        let exp = Ty::Builtin(BuiltinTy::Args);
                        let v = self.get_var(e.span(), to_ident_ref(&root, e)?)?;
                        v.ever_be(exp);
                        rest = Some(v.as_type());
                    }
                    // todo: ..(args)
                }
            }
        }

        let body = self.check_expr_in(closure.body().span(), root);

        let named: Vec<(EcoString, Ty)> = named.into_iter().collect();

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

        let sig = SigTy::new(pos.into_iter(), named.into_iter(), rest, Some(body));
        Some(Ty::Func(Interned::new(sig)))
    }

    fn check_let(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let let_binding: ast::LetBinding = root.cast()?;

        match let_binding.kind() {
            ast::LetBindingKind::Closure(c) => {
                // let _name = let_binding.name().get().to_string();
                let value = let_binding
                    .init()
                    .map(|init| self.check_expr_in(init.span(), root.clone()))
                    .unwrap_or_else(|| Ty::Infer);

                let v = self.get_var(c.span(), to_ident_ref(&root, c)?)?;
                v.ever_be(value);
                // todo lbs is the lexical signature.
            }
            ast::LetBindingKind::Normal(pattern) => {
                // let _name = let_binding.name().get().to_string();
                let value = let_binding
                    .init()
                    .map(|init| self.check_expr_in(init.span(), root.clone()))
                    .unwrap_or_else(|| Ty::Infer);

                self.check_pattern(pattern, value, root.clone());
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
                args: set_rule.args(),
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

        Some(Ty::Unary(Interned::new(TypeUnary {
            op: UnaryOp::Context,
            lhs: Interned::new(body),
        })))
    }

    fn check_conditional(&mut self, root: LinkedNode<'_>) -> Option<Ty> {
        let conditional: ast::Conditional = root.cast()?;

        let cond = self.check_expr_in(conditional.condition().span(), root.clone());
        let then = self.check_expr_in(conditional.if_body().span(), root.clone());
        let else_ = conditional
            .else_body()
            .map(|else_body| self.check_expr_in(else_body.span(), root.clone()))
            .unwrap_or(Ty::None);

        let cond = Interned::new(cond);
        let then = Interned::new(then);
        let else_ = Interned::new(else_);
        Some(Ty::If(Interned::new(IfTy { cond, then, else_ })))
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

        Some(Ty::None)
    }

    fn check_module_include(&mut self, _root: LinkedNode<'_>) -> Option<Ty> {
        Some(Ty::Content)
    }

    fn check_destructuring(&mut self, _root: LinkedNode<'_>) -> Option<Ty> {
        Some(Ty::Any)
    }

    fn check_destruct_assign(&mut self, _root: LinkedNode<'_>) -> Option<Ty> {
        Some(Ty::None)
    }
    fn check_expr_in(&mut self, span: Span, root: LinkedNode<'_>) -> Ty {
        root.find(span)
            .map(|node| self.check(node))
            .unwrap_or(Ty::Undef)
    }

    fn check_pattern(&mut self, pattern: ast::Pattern<'_>, value: Ty, root: LinkedNode<'_>) -> Ty {
        self.check_pattern_(pattern, value, root)
            .unwrap_or(Ty::Undef)
    }

    fn check_pattern_(
        &mut self,
        pattern: ast::Pattern<'_>,
        value: Ty,
        root: LinkedNode<'_>,
    ) -> Option<Ty> {
        Some(match pattern {
            ast::Pattern::Normal(ast::Expr::Ident(ident)) => {
                let v = self.get_var(ident.span(), to_ident_ref(&root, ident)?)?;
                v.ever_be(value);
                v.as_type()
            }
            ast::Pattern::Normal(_) => Ty::Any,
            ast::Pattern::Placeholder(_) => Ty::Any,
            ast::Pattern::Parenthesized(exp) => self.check_pattern(exp.pattern(), value, root),
            // todo: pattern
            ast::Pattern::Destructuring(_destruct) => Ty::Any,
        })
    }
}
