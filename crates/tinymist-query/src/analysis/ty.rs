//! Type checking on source file

use std::{collections::HashMap, sync::Arc};

use reflexo::vector::ir::DefId;
use typst::syntax::{
        ast::{self, AstNode},
        LinkedNode, Source, Span, SyntaxKind,
};

use crate::analysis::Ty;
use crate::{adt::interner::Interned, analysis::TypeCheckInfo, ty::TypeInterace, AnalysisContext};

use super::{
    resolve_global_value, BuiltinTy, DefUseInfo, FlowVarKind, IdentRef, TypeBounds, TypeVar,
    TypeVarBounds,
};

mod apply;
mod post_check;
mod syntax;

pub(crate) use apply::*;
pub(crate) use post_check::*;

/// Type checking at the source unit level.
pub(crate) fn type_check(ctx: &mut AnalysisContext, source: Source) -> Option<Arc<TypeCheckInfo>> {
    let mut info = TypeCheckInfo::default();

    // Retrieve def-use information for the source.
    let def_use_info = ctx.def_use(source.clone())?;

    let mut type_checker = TypeChecker {
        ctx,
        source: source.clone(),
        def_use_info,
        info: &mut info,
        externals: HashMap::new(),
        mode: InterpretMode::Markup,
    };
    let lnk = LinkedNode::new(source.root());

    let type_check_start = std::time::Instant::now();
    type_checker.check(lnk);
    let elapsed = type_check_start.elapsed();
    log::info!("Type checking on {:?} took {elapsed:?}", source.id());

    // todo: cross-file unit type checking
    let _ = type_checker.source;

    Some(Arc::new(info))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterpretMode {
    Markup,
    Code,
    Math,
}

struct TypeChecker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    source: Source,
    def_use_info: Arc<DefUseInfo>,

    info: &'a mut TypeCheckInfo,
    externals: HashMap<DefId, Option<Ty>>,
    mode: InterpretMode,
}

impl<'a, 'w> TypeChecker<'a, 'w> {
    fn check(&mut self, root: LinkedNode) -> Ty {
        let should_record = matches!(root.kind(), SyntaxKind::FuncCall).then(|| root.span());
        let w = self.check_syntax(root).unwrap_or(Ty::Undef);

        if let Some(s) = should_record {
            self.info.witness_at_least(s, w.clone());
        }

        w
    }

    fn check_inner(&mut self, root: LinkedNode) -> Option<Ty> {
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
                resultant: None,
            };
            callee.call(&args, true, &mut worker);
            return worker.resultant;
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
                resultant: None,
            };
            callee.call(&args, true, &mut worker);
            return worker.resultant;
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

    fn get_var(&mut self, s: Span, r: IdentRef) -> Option<&mut TypeVarBounds> {
        let def_id = self
            .def_use_info
            .get_ref(&r)
            .or_else(|| Some(self.def_use_info.get_def(s.id()?, &r)?.0))?;

        // todo: false positive of clippy
        #[allow(clippy::map_entry)]
        if !self.info.vars.contains_key(&def_id) {
            let def = self.check_external(def_id);
            let init_expr = self.init_var(def);
            self.info.vars.insert(
                def_id,
                TypeVarBounds::new(
                    TypeVar {
                    name: Interned::new_str(&r.name),
                        def: def_id,
                        syntax: None,
                },
                    init_expr,
                ),
            );
        }

        let var = self.info.vars.get_mut(&def_id).unwrap();
        TypeCheckInfo::witness_(s, var.as_type(), &mut self.info.mapping);
        Some(var)
    }

    fn check_external(&mut self, def_id: DefId) -> Option<Ty> {
        if let Some(ty) = self.externals.get(&def_id) {
            return ty.clone();
        }

        let (def_id, def_pos) = self.def_use_info.get_def_by_id(def_id)?;
        if def_id == self.source.id() {
            return None;
        }

        let source = self.ctx.source_by_id(def_id).ok()?;
        let ext_def_use_info = self.ctx.def_use(source.clone())?;
        let ext_type_info = self.ctx.type_check(source)?;
        let (ext_def_id, _) = ext_def_use_info.get_def(
            def_id,
            &IdentRef {
                name: def_pos.name.clone(),
                range: def_pos.range.clone(),
            },
        )?;
        let ext_ty = ext_type_info.vars.get(&ext_def_id)?.get_ref();
        Some(ext_type_info.simplify(ext_ty, false))
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
                v.get_ref()
            }
            ast::Pattern::Normal(_) => Ty::Any,
            ast::Pattern::Placeholder(_) => Ty::Any,
            ast::Pattern::Parenthesized(exp) => self.check_pattern(exp.pattern(), value, root),
            // todo: pattern
            ast::Pattern::Destructuring(_destruct) => Ty::Any,
        })
    }

    fn check_apply(
        &mut self,
        callee: FlowType,
        callee_span: Span,
        args: &FlowArgs,
        syntax_args: &ast::Args,
        candidates: &mut Vec<FlowType>,
    ) -> Option<()> {
        log::debug!("check func callee {callee:?}");

        match &callee {
            FlowType::Var(v) => {
                let w = self.info.vars.get(&v.0).cloned()?;
                match &w.kind {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        // It is instantiated here by clone.
                        let w = w.read().clone();
                        for lb in w.lbs.iter() {
                            self.check_apply(
                                lb.clone(),
                                callee_span,
                                args,
                                syntax_args,
                                candidates,
                            )?;
                        }
                        for ub in w.ubs.iter() {
                            self.check_apply(
                                ub.clone(),
                                callee_span,
                                args,
                                syntax_args,
                                candidates,
                            )?;
                        }
                    }
                }
            }
            FlowType::Func(v) => {
                self.info.witness_at_least(callee_span, callee.clone());

                let f = v.as_ref();
                let mut pos = f.pos.iter();
                // let mut named = f.named.clone();
                // let mut rest = f.rest.clone();

                for pos_in in args.start_match() {
                    let pos_ty = pos.next().unwrap_or(&FlowType::Any);
                    self.constrain(pos_in, pos_ty);
                }

                for (name, named_in) in &args.named {
                    let named_ty = f.named.iter().find(|(n, _)| n == name).map(|(_, ty)| ty);
                    if let Some(named_ty) = named_ty {
                        self.constrain(named_in, named_ty);
                    }
                }

                // log::debug!("check applied {v:?}");

                candidates.push(f.ret.clone());
            }
            FlowType::Dict(_v) => {}
            FlowType::Tuple(_v) => {}
            FlowType::Array(_v) => {}
            // todo: with
            FlowType::With(_e) => {}
            FlowType::Args(_e) => {}
            FlowType::Union(_e) => {}
            FlowType::Field(_e) => {}
            FlowType::Let(_) => {}
            FlowType::Value(f) => {
                if let Value::Func(f) = &f.0 {
                    self.check_apply_runtime(f, callee_span, args, syntax_args, candidates);
                }
            }
            FlowType::ValueDoc(f) => {
                if let Value::Func(f) = &f.0 {
                    self.check_apply_runtime(f, callee_span, args, syntax_args, candidates);
                }
            }

            FlowType::Clause => {}
            FlowType::Undef => {}
            FlowType::Content => {}
            FlowType::Any => {}
            FlowType::None => {}
            FlowType::Infer => {}
            FlowType::FlowNone => {}
            FlowType::Space => {}
            FlowType::Auto => {}
            FlowType::Builtin(_) => {}
            FlowType::Boolean(_) => {}
            FlowType::At(e) => {
                let primary_type = self.check_primary_type(e.0 .0.clone());
                self.check_apply_method(
                    primary_type,
                    callee_span,
                    e.0 .1.clone(),
                    args,
                    candidates,
                );
            }
            FlowType::Unary(_) => {}
            FlowType::Binary(_) => {}
            FlowType::If(_) => {}
            FlowType::Element(_elem) => {}
        }

        Some(())
    }

    fn constrain(&mut self, lhs: &FlowType, rhs: &FlowType) {
        static FLOW_STROKE_DICT_TYPE: Lazy<FlowType> =
            Lazy::new(|| FlowType::Dict(FLOW_STROKE_DICT.clone()));
        static FLOW_MARGIN_DICT_TYPE: Lazy<FlowType> =
            Lazy::new(|| FlowType::Dict(FLOW_MARGIN_DICT.clone()));
        static FLOW_INSET_DICT_TYPE: Lazy<FlowType> =
            Lazy::new(|| FlowType::Dict(FLOW_INSET_DICT.clone()));
        static FLOW_OUTSET_DICT_TYPE: Lazy<FlowType> =
            Lazy::new(|| FlowType::Dict(FLOW_OUTSET_DICT.clone()));
        static FLOW_RADIUS_DICT_TYPE: Lazy<FlowType> =
            Lazy::new(|| FlowType::Dict(FLOW_RADIUS_DICT.clone()));

        match (lhs, rhs) {
            (FlowType::Var(v), FlowType::Var(w)) => {
                if v.0 .0 == w.0 .0 {
                    return;
                }

                // todo: merge

                let _ = v.0 .0;
                let _ = w.0 .0;
            }
            (FlowType::Var(v), rhs) => {
                log::debug!("constrain var {v:?} ⪯ {rhs:?}");
                let w = self.info.vars.get_mut(&v.0).unwrap();
                // strict constraint on upper bound
                let bound = rhs.clone();
                match &w.kind {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let mut w = w.write();
                        w.ubs.push(bound);
                    }
                }
            }
            (lhs, FlowType::Var(v)) => {
                let w = self.info.vars.get(&v.0).unwrap();
                let bound = self.weaken_constraint(lhs, &w.kind);
                log::debug!("constrain var {v:?} ⪰ {bound:?}");
                match &w.kind {
                    FlowVarKind::Strong(v) | FlowVarKind::Weak(v) => {
                        let mut v = v.write();
                        v.lbs.push(bound);
                    }
                }
            }
            (FlowType::Union(v), rhs) => {
                for e in v.iter() {
                    self.constrain(e, rhs);
                }
            }
            (lhs, FlowType::Union(v)) => {
                for e in v.iter() {
                    self.constrain(lhs, e);
                }
            }
            (lhs, FlowType::Builtin(FlowBuiltinType::Stroke)) => {
                // empty array is also a constructing dict but we can safely ignore it during
                // type checking, since no fields are added yet.
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_STROKE_DICT_TYPE);
                }
            }
            (FlowType::Builtin(FlowBuiltinType::Stroke), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_STROKE_DICT_TYPE, rhs);
                }
            }
            (lhs, FlowType::Builtin(FlowBuiltinType::Margin)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_MARGIN_DICT_TYPE);
                }
            }
            (FlowType::Builtin(FlowBuiltinType::Margin), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_MARGIN_DICT_TYPE, rhs);
                }
            }
            (lhs, FlowType::Builtin(FlowBuiltinType::Inset)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_INSET_DICT_TYPE);
                }
            }
            (FlowType::Builtin(FlowBuiltinType::Inset), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_INSET_DICT_TYPE, rhs);
                }
            }
            (lhs, FlowType::Builtin(FlowBuiltinType::Outset)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_OUTSET_DICT_TYPE);
                }
            }
            (FlowType::Builtin(FlowBuiltinType::Outset), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_OUTSET_DICT_TYPE, rhs);
                }
            }
            (lhs, FlowType::Builtin(FlowBuiltinType::Radius)) => {
                if lhs.is_dict() {
                    self.constrain(lhs, &FLOW_RADIUS_DICT_TYPE);
                }
            }
            (FlowType::Builtin(FlowBuiltinType::Radius), rhs) => {
                if rhs.is_dict() {
                    self.constrain(&FLOW_RADIUS_DICT_TYPE, rhs);
                }
            }
            (FlowType::Dict(lhs), FlowType::Dict(rhs)) => {
                for ((key, lhs, sl), (_, rhs, sr)) in lhs.intersect_keys(rhs) {
                    log::debug!("constrain record item {key} {lhs:?} ⪯ {rhs:?}");
                    self.constrain(lhs, rhs);
                    if !sl.is_detached() {
                        self.info.witness_at_most(*sl, rhs.clone());
                    }
                    if !sr.is_detached() {
                        self.info.witness_at_least(*sr, lhs.clone());
                    }
                }
            }
            (FlowType::Value(lhs), rhs) => {
                log::debug!("constrain value {lhs:?} ⪯ {rhs:?}");
                if !lhs.1.is_detached() {
                    self.info.witness_at_most(lhs.1, rhs.clone());
                }
            }
            (lhs, FlowType::Value(rhs)) => {
                log::debug!("constrain value {lhs:?} ⪯ {rhs:?}");
                if !rhs.1.is_detached() {
                    self.info.witness_at_least(rhs.1, lhs.clone());
                }
            }
            _ => {
                log::debug!("constrain {lhs:?} ⪯ {rhs:?}");
            }
        }
    }

    fn check_primary_type(&self, e: FlowType) -> FlowType {
        match &e {
            FlowType::Var(v) => {
                let w = self.info.vars.get(&v.0).unwrap();
                match &w.kind {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();
                        if !w.ubs.is_empty() {
                            return w.ubs[0].clone();
                        }
                        if !w.lbs.is_empty() {
                            return w.lbs[0].clone();
                        }
                        FlowType::Any
                    }
                }
            }
            FlowType::Func(..) => e,
            FlowType::Dict(..) => e,
            FlowType::With(..) => e,
            FlowType::Args(..) => e,
            FlowType::Union(..) => e,
            FlowType::Let(_) => e,
            FlowType::Value(..) => e,
            FlowType::ValueDoc(..) => e,

            FlowType::Tuple(..) => e,
            FlowType::Array(..) => e,
            FlowType::Field(..) => e,
            FlowType::Clause => e,
            FlowType::Undef => e,
            FlowType::Content => e,
            FlowType::Any => e,
            FlowType::None => e,
            FlowType::Infer => e,
            FlowType::FlowNone => e,
            FlowType::Space => e,
            FlowType::Auto => e,
            FlowType::Builtin(_) => e,
            FlowType::At(e) => self.check_primary_type(e.0 .0.clone()),
            FlowType::Unary(_) => e,
            FlowType::Binary(_) => e,
            FlowType::Boolean(_) => e,
            FlowType::If(_) => e,
            FlowType::Element(_) => e,
        }
    }

    fn check_apply_method(
        &mut self,
        primary_type: FlowType,
        callee_span: Span,
        method_name: EcoString,
        args: &FlowArgs,
        _candidates: &mut Vec<FlowType>,
    ) -> Option<()> {
        log::debug!("check method at {method_name:?} on {primary_type:?}");
        self.info
            .witness_at_least(callee_span, primary_type.clone());
        match primary_type {
            FlowType::Func(v) => match method_name.as_str() {
                // todo: process where specially
                "with" | "where" => {
                    // log::debug!("check method at args: {v:?}.with({args:?})");

                    let f = v.as_ref();
                    let mut pos = f.pos.iter();
                    // let mut named = f.named.clone();
                    // let mut rest = f.rest.clone();

                    for pos_in in args.start_match() {
                        let pos_ty = pos.next().unwrap_or(&FlowType::Any);
                        self.constrain(pos_in, pos_ty);
                    }

                    for (name, named_in) in &args.named {
                        let named_ty = f.named.iter().find(|(n, _)| n == name).map(|(_, ty)| ty);
                        if let Some(named_ty) = named_ty {
                            self.constrain(named_in, named_ty);
                        }
                    }

                    _candidates.push(self.partial_apply(f, args));
                }
                _ => {}
            },
            FlowType::Array(..) => {}
            FlowType::Dict(..) => {}
            _ => {}
        }

        Some(())
    }

    fn check_apply_runtime(
        &mut self,
        f: &Func,
        callee_span: Span,
        args: &FlowArgs,
        syntax_args: &ast::Args,
        candidates: &mut Vec<FlowType>,
    ) -> Option<()> {
        // todo: hold signature
        self.info.witness_at_least(
            callee_span,
            FlowType::Value(Box::new((Value::Func(f.clone()), Span::detached()))),
        );
        let sig = analyze_dyn_signature(self.ctx, f.clone());

        log::debug!("check runtime func {f:?} at args: {args:?}");

        let mut pos = sig
            .primary()
            .pos
            .iter()
            .map(|e| e.infer_type.as_ref().unwrap_or(&FlowType::Any));
        let mut syntax_pos = syntax_args.items().filter_map(|arg| match arg {
            ast::Arg::Pos(e) => Some(e),
            _ => None,
        });

        for pos_in in args.start_match() {
            let pos_ty = pos.next().unwrap_or(&FlowType::Any);
            self.constrain(pos_in, pos_ty);
            if let Some(syntax_pos) = syntax_pos.next() {
                self.info
                    .witness_at_least(syntax_pos.span(), pos_ty.clone());
            }
        }

        for (name, named_in) in &args.named {
            let named_ty = sig
                .primary()
                .named
                .get(name.as_ref())
                .and_then(|e| e.infer_type.as_ref());
            let syntax_named = syntax_args
                .items()
                .filter_map(|arg| match arg {
                    ast::Arg::Named(n) => Some(n),
                    _ => None,
                })
                .find(|n| n.name().get() == name.as_ref());
            if let Some(named_ty) = named_ty {
                self.constrain(named_in, named_ty);
                if let Some(syntax_named) = syntax_named {
                    self.info
                        .witness_at_least(syntax_named.span(), named_ty.clone());
                    self.info
                        .witness_at_least(syntax_named.expr().span(), named_ty.clone());
                }
            }
        }

        candidates.push(sig.primary().ret_ty.clone().unwrap_or(FlowType::Any));

        Some(())
    }

    fn partial_apply(&self, f: &FlowSignature, args: &FlowArgs) -> FlowType {
        FlowType::With(Box::new((
            FlowType::Func(Box::new(f.clone())),
            vec![args.clone()],
        )))
    }

    fn check_comparable(&self, lhs: &FlowType, rhs: &FlowType) {
        let _ = lhs;
        let _ = rhs;
    }

    fn check_assignable(&self, lhs: &FlowType, rhs: &FlowType) {
        let _ = lhs;
        let _ = rhs;
    }

    fn check_containing(&self, container: &FlowType, elem: &FlowType, expected_in: bool) {
        let _ = container;
        let _ = elem;
        let _ = expected_in;
    }

    fn possible_ever_be(&mut self, lhs: &FlowType, rhs: &FlowType) {
        // todo: instantiataion
        match rhs {
            FlowType::Undef
            | FlowType::Content
            | FlowType::None
            | FlowType::FlowNone
            | FlowType::Auto
            | FlowType::Element(..)
            | FlowType::Builtin(..)
            | FlowType::Value(..)
            | FlowType::Boolean(..)
            | FlowType::ValueDoc(..) => {
                self.constrain(rhs, lhs);
            }
            _ => {}
        }
    }

    fn init_var(&mut self, def: Option<FlowType>) -> FlowVarStore {
        let mut store = FlowVarStore::default();

        let Some(def) = def else {
            return store;
        };

        match def {
            FlowType::Var(v) => {
                let w = self.info.vars.get(&v.0).unwrap();
                match &w.kind {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();
                        store.lbs.extend(w.lbs.iter().cloned());
                        store.ubs.extend(w.ubs.iter().cloned());
                    }
                }
            }
            FlowType::Let(v) => {
                store.lbs.extend(v.lbs.iter().cloned());
                store.ubs.extend(v.ubs.iter().cloned());
            }
            _ => {
                store.ubs.push(def);
            }
        }

        store
    }

    fn weaken(&mut self, v: &FlowType) {
        match v {
            FlowType::Var(v) => {
                let w = self.info.vars.get_mut(&v.0).unwrap();
                w.weaken();
            }
            FlowType::Clause
            | FlowType::Undef
            | FlowType::Content
            | FlowType::Any
            | FlowType::Space
            | FlowType::None
            | FlowType::Infer
            | FlowType::FlowNone
            | FlowType::Auto
            | FlowType::Boolean(_)
            | FlowType::Builtin(_)
            | FlowType::Value(_) => {}
            FlowType::Element(_) => {}
            FlowType::ValueDoc(_) => {}
            FlowType::Field(v) => {
                self.weaken(&v.1);
            }
            FlowType::Func(v) => {
                for ty in v.pos.iter() {
                    self.weaken(ty);
                }
                for (_, ty) in v.named.iter() {
                    self.weaken(ty);
                }
                if let Some(ty) = &v.rest {
                    self.weaken(ty);
                }
                self.weaken(&v.ret);
            }
            FlowType::Dict(v) => {
                for (_, ty, _) in v.fields.iter() {
                    self.weaken(ty);
                }
            }
            FlowType::Array(v) => {
                self.weaken(v);
            }
            FlowType::Tuple(v) => {
                for ty in v.iter() {
                    self.weaken(ty);
                }
            }
            FlowType::With(v) => {
                self.weaken(&v.0);
                for args in v.1.iter() {
                    for ty in args.args.iter() {
                        self.weaken(ty);
                    }
                    for (_, ty) in args.named.iter() {
                        self.weaken(ty);
                    }
                }
            }
            FlowType::Args(v) => {
                for ty in v.args.iter() {
                    self.weaken(ty);
                }
                for (_, ty) in v.named.iter() {
                    self.weaken(ty);
                }
            }
            FlowType::At(v) => {
                self.weaken(&v.0 .0);
            }
            FlowType::Unary(v) => {
                self.weaken(&v.lhs);
            }
            FlowType::Binary(v) => {
                let (lhs, rhs) = v.repr();
                self.weaken(lhs);
                self.weaken(rhs);
            }
            FlowType::If(v) => {
                self.weaken(&v.cond);
                self.weaken(&v.then);
                self.weaken(&v.else_);
            }
            FlowType::Union(v) => {
                for ty in v.iter() {
                    self.weaken(ty);
                }
            }
            FlowType::Let(v) => {
                for ty in v.lbs.iter() {
                    self.weaken(ty);
                }
                for ty in v.ubs.iter() {
                    self.weaken(ty);
                }
            }
        }
    }

    fn weaken_constraint(&self, c: &FlowType, kind: &FlowVarKind) -> FlowType {
        if matches!(kind, FlowVarKind::Strong(_)) {
            return c.clone();
        }

        if let FlowType::Value(v) = c {
            return FlowBuiltinType::from_value(&v.0);
        }

        c.clone()
    }
}

#[derive(Default)]
struct TypeCanoStore {
    cano_cache: HashMap<(u128, bool), FlowType>,
    cano_local_cache: HashMap<(DefId, bool), FlowType>,
    negatives: HashSet<DefId>,
    positives: HashSet<DefId>,
}

#[derive(Default)]
struct TypeDescriber {
    described: HashMap<u128, String>,
    results: HashSet<String>,
    functions: Vec<FlowSignature>,
}

impl TypeDescriber {
    fn describe_root(&mut self, ty: &FlowType) -> Option<String> {
        // recursive structure
        if let Some(t) = self.described.get(&hash128(ty)) {
            return Some(t.clone());
        }

        let res = self.describe(ty);
        if !res.is_empty() {
            return Some(res);
        }
        self.described.insert(hash128(ty), "$self".to_string());

        let mut results = std::mem::take(&mut self.results)
            .into_iter()
            .collect::<Vec<_>>();
        let functions = std::mem::take(&mut self.functions);
        if !functions.is_empty() {
            // todo: union signature
            // only first function is described
            let f = functions[0].clone();

            let mut res = String::new();
            res.push('(');
            let mut not_first = false;
            for ty in f.pos.iter() {
                if not_first {
                    res.push_str(", ");
                } else {
                    not_first = true;
                }
                res.push_str(self.describe_root(ty).as_deref().unwrap_or("any"));
            }
            for (k, ty) in f.named.iter() {
                if not_first {
                    res.push_str(", ");
                } else {
                    not_first = true;
                }
                res.push_str(k);
                res.push_str(": ");
                res.push_str(self.describe_root(ty).as_deref().unwrap_or("any"));
            }
            if let Some(r) = &f.rest {
                if not_first {
                    res.push_str(", ");
                }
                res.push_str("..: ");
                res.push_str(self.describe_root(r).as_deref().unwrap_or(""));
                res.push_str("[]");
            }
            res.push_str(") => ");
            res.push_str(self.describe_root(&f.ret).as_deref().unwrap_or("any"));
            results.push(res);
        }

        if results.is_empty() {
            self.described.insert(hash128(ty), "any".to_string());
            return None;
        }

        results.sort();
        results.dedup();
        let res = results.join(" | ");
        self.described.insert(hash128(ty), res.clone());
        Some(res)
    }

    fn describe_iter(&mut self, ty: &[FlowType]) {
        for ty in ty.iter() {
            let desc = self.describe(ty);
            if !desc.is_empty() {
                self.results.insert(desc);
            }
        }
    }

    fn describe(&mut self, ty: &FlowType) -> String {
        match ty {
            FlowType::Var(..) => {}
            FlowType::Union(tys) => {
                self.describe_iter(tys);
            }
            FlowType::Let(lb) => {
                self.describe_iter(&lb.lbs);
                self.describe_iter(&lb.ubs);
            }
            FlowType::Func(f) => {
                self.functions.push(*f.clone());
            }
            FlowType::Dict(..) => {
                return "dict".to_string();
            }
            FlowType::Tuple(..) => {
                return "array".to_string();
            }
            FlowType::Array(..) => {
                return "array".to_string();
            }
            FlowType::With(w) => {
                return self.describe(&w.0);
            }
            FlowType::Clause => {}
            FlowType::Undef => {}
            FlowType::Content => {
                return "content".to_string();
            }
            // Doesn't provide any information, hence we doesn't describe it intermediately here.
            FlowType::Any => {}
            FlowType::Space => {}
            FlowType::None => {
                return "none".to_string();
            }
            FlowType::Infer => {}
            FlowType::FlowNone => {
                return "none".to_string();
            }
            FlowType::Auto => {
                return "auto".to_string();
            }
            FlowType::Boolean(None) => {
                return "boolean".to_string();
            }
            FlowType::Boolean(Some(b)) => {
                return b.to_string();
            }
            FlowType::Builtin(b) => {
                return b.describe().to_string();
            }
            FlowType::Value(v) => return v.0.repr().to_string(),
            FlowType::ValueDoc(v) => return v.0.repr().to_string(),
            FlowType::Field(..) => {
                return "field".to_string();
            }
            FlowType::Element(..) => {
                return "element".to_string();
            }
            FlowType::Args(..) => {
                return "args".to_string();
            }
            FlowType::At(..) => {
                return "any".to_string();
            }
            FlowType::Unary(..) => {
                return "any".to_string();
            }
            FlowType::Binary(..) => {
                return "any".to_string();
            }
            FlowType::If(..) => {
                return "any".to_string();
            }
        }

        String::new()
    }
}

struct TypeSimplifier<'a, 'b> {
    principal: bool,

    vars: &'a HashMap<DefId, FlowVar>,

    cano_cache: &'b mut HashMap<(u128, bool), FlowType>,
    cano_local_cache: &'b mut HashMap<(DefId, bool), FlowType>,
    negatives: &'b mut HashSet<DefId>,
    positives: &'b mut HashSet<DefId>,
}

impl<'a, 'b> TypeSimplifier<'a, 'b> {
    fn simplify(&mut self, ty: FlowType, principal: bool) -> FlowType {
        // todo: hash safety
        let ty_key = hash128(&ty);
        if let Some(cano) = self.cano_cache.get(&(ty_key, principal)) {
            return cano.clone();
        }

        self.analyze(&ty, true);

        self.transform(&ty, true)
    }

    fn analyze(&mut self, ty: &FlowType, pol: bool) {
        match ty {
            FlowType::Var(v) => {
                let w = self.vars.get(&v.0).unwrap();
                match &w.kind {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();
                        let inserted = if pol {
                            self.positives.insert(v.0)
                        } else {
                            self.negatives.insert(v.0)
                        };
                        if !inserted {
                            return;
                        }

                        if pol {
                            for lb in w.lbs.iter() {
                                self.analyze(lb, pol);
                            }
                        } else {
                            for ub in w.ubs.iter() {
                                self.analyze(ub, pol);
                            }
                        }
                    }
                }
            }
            FlowType::Func(f) => {
                for p in &f.pos {
                    self.analyze(p, !pol);
                }
                for (_, p) in &f.named {
                    self.analyze(p, !pol);
                }
                if let Some(r) = &f.rest {
                    self.analyze(r, !pol);
                }
                self.analyze(&f.ret, pol);
            }
            FlowType::Dict(r) => {
                for (_, p, _) in &r.fields {
                    self.analyze(p, pol);
                }
            }
            FlowType::Tuple(e) => {
                for ty in e.iter() {
                    self.analyze(ty, pol);
                }
            }
            FlowType::Array(e) => {
                self.analyze(e, pol);
            }
            FlowType::With(w) => {
                self.analyze(&w.0, pol);
                for m in &w.1 {
                    for arg in m.args.iter() {
                        self.analyze(arg, pol);
                    }
                    for (_, arg) in m.named.iter() {
                        self.analyze(arg, pol);
                    }
                }
            }
            FlowType::Args(args) => {
                for arg in &args.args {
                    self.analyze(arg, pol);
                }
            }
            FlowType::Unary(u) => self.analyze(u.lhs(), pol),
            FlowType::Binary(b) => {
                let (lhs, rhs) = b.repr();
                self.analyze(lhs, pol);
                self.analyze(rhs, pol);
            }
            FlowType::If(i) => {
                self.analyze(&i.cond, pol);
                self.analyze(&i.then, pol);
                self.analyze(&i.else_, pol);
            }
            FlowType::Union(v) => {
                for ty in v.iter() {
                    self.analyze(ty, pol);
                }
            }
            FlowType::At(a) => {
                self.analyze(&a.0 .0, pol);
            }
            FlowType::Let(v) => {
                for lb in v.lbs.iter() {
                    self.analyze(lb, !pol);
                }
                for ub in v.ubs.iter() {
                    self.analyze(ub, pol);
                }
            }
            FlowType::Field(v) => {
                self.analyze(&v.1, pol);
            }
            FlowType::Value(_v) => {}
            FlowType::ValueDoc(_v) => {}
            FlowType::Clause => {}
            FlowType::Undef => {}
            FlowType::Content => {}
            FlowType::Any => {}
            FlowType::None => {}
            FlowType::Infer => {}
            FlowType::FlowNone => {}
            FlowType::Space => {}
            FlowType::Auto => {}
            FlowType::Boolean(_) => {}
            FlowType::Builtin(_) => {}
            FlowType::Element(_) => {}
        }
    }

    fn transform(&mut self, ty: &FlowType, pol: bool) -> FlowType {
        match ty {
            // todo
            FlowType::Let(w) => self.transform_let(w, None, pol),
            FlowType::Var(v) => {
                if let Some(cano) = self.cano_local_cache.get(&(v.0, self.principal)) {
                    return cano.clone();
                }
                // todo: avoid cycle
                self.cano_local_cache
                    .insert((v.0, self.principal), FlowType::Any);

                let res = match &self.vars.get(&v.0).unwrap().kind {
                    FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => {
                        let w = w.read();

                        self.transform_let(&w, Some(&v.0), pol)
                    }
                };

                self.cano_local_cache
                    .insert((v.0, self.principal), res.clone());

                res
            }
            FlowType::Func(f) => {
                let pos = f.pos.iter().map(|p| self.transform(p, !pol)).collect();
                let named = f
                    .named
                    .iter()
                    .map(|(n, p)| (n.clone(), self.transform(p, !pol)))
                    .collect();
                let rest = f.rest.as_ref().map(|r| self.transform(r, !pol));
                let ret = self.transform(&f.ret, pol);

                FlowType::Func(Box::new(FlowSignature {
                    pos,
                    named,
                    rest,
                    ret,
                }))
            }
            FlowType::Dict(f) => {
                let fields = f
                    .fields
                    .iter()
                    .map(|p| (p.0.clone(), self.transform(&p.1, !pol), p.2))
                    .collect();

                FlowType::Dict(FlowRecord { fields })
            }
            FlowType::Tuple(e) => {
                let e2 = e.iter().map(|ty| self.transform(ty, pol)).collect();

                FlowType::Tuple(e2)
            }
            FlowType::Array(e) => {
                let e2 = self.transform(e, pol);

                FlowType::Array(Box::new(e2))
            }
            FlowType::With(w) => {
                let primary = self.transform(&w.0, pol);
                let args =
                    w.1.iter()
                        .map(|a| {
                            let args_res = a.args.iter().map(|a| self.transform(a, pol)).collect();
                            let named = a
                                .named
                                .iter()
                                .map(|(n, a)| (n.clone(), self.transform(a, pol)))
                                .collect();

                            FlowArgs {
                                args: args_res,
                                named,
                            }
                        })
                        .collect();
                FlowType::With(Box::new((primary, args)))
            }
            FlowType::Args(args) => {
                let args_res = args.args.iter().map(|a| self.transform(a, pol)).collect();
                let named = args
                    .named
                    .iter()
                    .map(|(n, a)| (n.clone(), self.transform(a, pol)))
                    .collect();

                FlowType::Args(Box::new(FlowArgs {
                    args: args_res,
                    named,
                }))
            }
            FlowType::Unary(u) => {
                let lhs = self.transform(u.lhs(), pol);
                FlowType::Unary(FlowUnaryType {
                    op: u.op,
                    lhs: Box::new(lhs),
                })
            }
            FlowType::Binary(b) => {
                let (lhs, rhs) = b.repr();
                let lhs = self.transform(lhs, pol);
                let rhs = self.transform(rhs, pol);

                FlowType::Binary(FlowBinaryType {
                    op: b.op,
                    operands: Box::new((lhs, rhs)),
                })
            }
            FlowType::If(i) => {
                let i2 = *i.clone();

                FlowType::If(Box::new(FlowIfType {
                    cond: self.transform(&i2.cond, pol),
                    then: self.transform(&i2.then, pol),
                    else_: self.transform(&i2.else_, pol),
                }))
            }
            FlowType::Union(v) => {
                let v2 = v.iter().map(|ty| self.transform(ty, pol)).collect();

                FlowType::Union(Box::new(v2))
            }
            FlowType::Field(f) => {
                let (x, y, z) = *f.clone();

                FlowType::Field(Box::new((x, self.transform(&y, pol), z)))
            }
            FlowType::At(a) => {
                let FlowAt(at) = a.clone();
                let atee = self.transform(&at.0, pol);

                FlowType::At(FlowAt(Box::new((atee, at.1))))
            }

            FlowType::Value(v) => FlowType::Value(v.clone()),
            FlowType::ValueDoc(v) => FlowType::ValueDoc(v.clone()),
            FlowType::Element(v) => FlowType::Element(*v),
            FlowType::Clause => FlowType::Clause,
            FlowType::Undef => FlowType::Undef,
            FlowType::Content => FlowType::Content,
            FlowType::Any => FlowType::Any,
            FlowType::None => FlowType::None,
            FlowType::Infer => FlowType::Infer,
            FlowType::FlowNone => FlowType::FlowNone,
            FlowType::Space => FlowType::Space,
            FlowType::Auto => FlowType::Auto,
            FlowType::Boolean(b) => FlowType::Boolean(*b),
            FlowType::Builtin(b) => FlowType::Builtin(b.clone()),
        }
    }

    fn transform_let(&mut self, w: &FlowVarStore, def_id: Option<&DefId>, pol: bool) -> FlowType {
        let mut lbs = EcoVec::with_capacity(w.lbs.len());
        let mut ubs = EcoVec::with_capacity(w.ubs.len());

        log::debug!("transform let [principal={}] with {w:?}", self.principal);

        if !self.principal || ((pol) && !def_id.is_some_and(|i| self.negatives.contains(i))) {
            for lb in w.lbs.iter() {
                lbs.push(self.transform(lb, pol));
            }
        }
        if !self.principal || ((!pol) && !def_id.is_some_and(|i| self.positives.contains(i))) {
            for ub in w.ubs.iter() {
                ubs.push(self.transform(ub, !pol));
            }
        }

        if ubs.is_empty() {
            if lbs.len() == 1 {
                return lbs.pop().unwrap();
            }
            if lbs.is_empty() {
                return FlowType::Any;
            }
        }

        FlowType::Let(Arc::new(FlowVarStore { lbs, ubs }))
    }
}

fn to_ident_ref(root: &LinkedNode, c: ast::Ident) -> Option<IdentRef> {
    Some(IdentRef {
        name: c.get().to_string(),
        range: root.find(c.span())?.range(),
    })
}

struct Joiner {
    break_or_continue_or_return: bool,
    definite: Ty,
    possibles: Vec<Ty>,
}
impl Joiner {
    fn finalize(self) -> Ty {
        log::debug!("join: {:?} {:?}", self.possibles, self.definite);
        if self.possibles.is_empty() {
            return self.definite;
        }
        if self.possibles.len() == 1 {
            return self.possibles.into_iter().next().unwrap();
        }

        // let mut definite = self.definite.clone();
        // for p in &self.possibles {
        //     definite = definite.join(p);
        // }

        // log::debug!("possibles: {:?} {:?}", self.definite, self.possibles);

        Ty::Any
    }

    fn join(&mut self, child: Ty) {
        if self.break_or_continue_or_return {
            return;
        }

        match (child, &self.definite) {
            (Ty::Clause, _) => {}
            (Ty::Undef, _) => {}
            (Ty::Space, _) => {}
            (Ty::Any, _) | (_, Ty::Any) => {}
            (Ty::Infer, _) => {}
            (Ty::None, _) => {}
            // todo: mystery flow none
            (Ty::FlowNone, _) => {}
            (Ty::Content, Ty::Content) => {}
            (Ty::Content, Ty::None) => self.definite = Ty::Content,
            (Ty::Content, _) => self.definite = Ty::Undef,
            (Ty::Var(v), _) => self.possibles.push(Ty::Var(v)),
            // todo: check possibles
            (Ty::Array(e), Ty::None) => self.definite = Ty::Array(e),
            (Ty::Array(..), _) => self.definite = Ty::Undef,
            (Ty::Tuple(e), Ty::None) => self.definite = Ty::Tuple(e),
            (Ty::Tuple(..), _) => self.definite = Ty::Undef,
            // todo: possible some style
            (Ty::Auto, Ty::None) => self.definite = Ty::Auto,
            (Ty::Auto, _) => self.definite = Ty::Undef,
            (Ty::Builtin(b), Ty::None) => self.definite = Ty::Builtin(b),
            (Ty::Builtin(..), _) => self.definite = Ty::Undef,
            // todo: value join
            (Ty::Value(v), Ty::None) => self.definite = Ty::Value(v),
            (Ty::Value(..), _) => self.definite = Ty::Undef,
            (Ty::Func(f), Ty::None) => self.definite = Ty::Func(f),
            (Ty::Func(..), _) => self.definite = Ty::Undef,
            (Ty::Dict(w), Ty::None) => self.definite = Ty::Dict(w),
            (Ty::Dict(..), _) => self.definite = Ty::Undef,
            (Ty::With(w), Ty::None) => self.definite = Ty::With(w),
            (Ty::With(..), _) => self.definite = Ty::Undef,
            (Ty::Args(w), Ty::None) => self.definite = Ty::Args(w),
            (Ty::Args(..), _) => self.definite = Ty::Undef,
            (Ty::Select(w), Ty::None) => self.definite = Ty::Select(w),
            (Ty::Select(..), _) => self.definite = Ty::Undef,
            (Ty::Unary(w), Ty::None) => self.definite = Ty::Unary(w),
            (Ty::Unary(..), _) => self.definite = Ty::Undef,
            (Ty::Binary(w), Ty::None) => self.definite = Ty::Binary(w),
            (Ty::Binary(..), _) => self.definite = Ty::Undef,
            (Ty::If(w), Ty::None) => self.definite = Ty::If(w),
            (Ty::If(..), _) => self.definite = Ty::Undef,
            (Ty::Union(w), Ty::None) => self.definite = Ty::Union(w),
            (Ty::Union(..), _) => self.definite = Ty::Undef,
            (Ty::Let(w), Ty::None) => self.definite = Ty::Let(w),
            (Ty::Let(..), _) => self.definite = Ty::Undef,
            (Ty::Field(w), Ty::None) => self.definite = Ty::Field(w),
            (Ty::Field(..), _) => self.definite = Ty::Undef,
            (Ty::Boolean(b), Ty::None) => self.definite = Ty::Boolean(b),
            (Ty::Boolean(..), _) => self.definite = Ty::Undef,
        }
    }
}
impl Default for Joiner {
    fn default() -> Self {
        Self {
            break_or_continue_or_return: false,
            definite: Ty::None,
            possibles: Vec::new(),
        }
    }
}
