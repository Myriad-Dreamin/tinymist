//! A linter for Typst.

use std::sync::Arc;

use tinymist_analysis::{
    syntax::ExprInfo,
    ty::{Ty, TyCtx, TypeInfo},
};
use typst::{
    diag::{eco_format, EcoString, SourceDiagnostic, Tracepoint},
    ecow::EcoVec,
    syntax::{
        ast::{self, AstNode},
        Span, Spanned, SyntaxNode,
    },
};

/// A type alias for a vector of diagnostics.
type DiagnosticVec = EcoVec<SourceDiagnostic>;

/// Performs linting check on file and returns a vector of diagnostics.
pub fn lint_file(expr: &ExprInfo, ti: Arc<TypeInfo>) -> DiagnosticVec {
    Linter::new(ti).lint(expr.source.root())
}

struct Linter {
    ti: Arc<TypeInfo>,
    diag: DiagnosticVec,
    loop_info: Option<LoopInfo>,
    func_info: Option<FuncInfo>,
}

impl Linter {
    fn new(ti: Arc<TypeInfo>) -> Self {
        Self {
            ti,
            diag: EcoVec::new(),
            loop_info: None,
            func_info: None,
        }
    }

    fn tctx(&self) -> &impl TyCtx {
        self.ti.as_ref()
    }

    fn lint(mut self, node: &SyntaxNode) -> DiagnosticVec {
        if let Some(markup) = node.cast::<ast::Markup>() {
            self.exprs(markup.exprs());
        } else if let Some(expr) = node.cast() {
            self.expr(expr);
        }

        self.diag
    }

    fn with_loop_info<F>(&mut self, span: Span, f: F) -> Option<()>
    where
        F: FnOnce(&mut Self) -> Option<()>,
    {
        let old = self.loop_info.take();
        self.loop_info = Some(LoopInfo {
            span,
            has_break: false,
            has_continue: false,
        });
        f(self);
        self.loop_info = old;
        Some(())
    }

    fn with_func_info<F>(&mut self, span: Span, f: F) -> Option<()>
    where
        F: FnOnce(&mut Self) -> Option<()>,
    {
        let old = self.func_info.take();
        self.func_info = Some(FuncInfo {
            span,
            is_contextual: false,
            has_return: false,
            has_return_value: false,
            parent_loop: self.loop_info.clone(),
        });
        f(self);
        self.loop_info = self.func_info.take().expect("func info").parent_loop;
        self.func_info = old;
        Some(())
    }

    fn late_func_return(&mut self, f: impl FnOnce(LateFuncLinter) -> Option<()>) -> Option<()> {
        let func_info = self.func_info.as_ref().expect("func info").clone();
        f(LateFuncLinter {
            linter: self,
            func_info,
            return_block_info: None,
        })
    }

    fn bad_branch_stmt(&mut self, expr: &SyntaxNode, name: &str) -> Option<()> {
        let parent_loop = self
            .func_info
            .as_ref()
            .map(|info| (info.parent_loop.as_ref(), info));

        let mut diag = SourceDiagnostic::warning(
            expr.span(),
            eco_format!("`{name}` statement in a non-loop context"),
        );
        if let Some((Some(loop_info), func_info)) = parent_loop {
            diag.trace.push(Spanned::new(
                Tracepoint::Show(EcoString::inline("loop")),
                loop_info.span,
            ));
            diag.trace
                .push(Spanned::new(Tracepoint::Call(None), func_info.span));
        }
        self.diag.push(diag);

        Some(())
    }

    #[inline(always)]
    fn buggy_block_expr(&mut self, expr: ast::Expr, loc: BuggyBlockLoc) -> Option<()> {
        self.buggy_block(Block::from(expr)?, loc)
    }

    fn buggy_block(&mut self, block: Block, loc: BuggyBlockLoc) -> Option<()> {
        if self.only_show(block) {
            let mut first = true;
            for set in block.iter() {
                let msg = match set {
                    ast::Expr::Set(..) => "This set statement doesn't take effect.",
                    ast::Expr::Show(..) => "This show statement doesn't take effect.",
                    _ => continue,
                };
                let mut warning = SourceDiagnostic::warning(set.span(), msg);
                if first {
                    first = false;
                    warning.hint(loc.hint(set));
                }
                self.diag.push(warning);
            }

            return None;
        }

        Some(())
    }

    fn only_show(&mut self, block: Block) -> bool {
        let mut has_set = false;

        for it in block.iter() {
            if is_show_set(it) {
                has_set = true;
            } else if matches!(it, ast::Expr::Break(..) | ast::Expr::Continue(..)) {
                return has_set;
            } else if !it.to_untyped().kind().is_trivia() {
                return false;
            }
        }

        has_set
    }

    fn check_type_compare(&mut self, expr: ast::Binary<'_>) {
        let op = expr.op();
        if is_compare_op(op) {
            let lhs = expr.lhs();
            let rhs = expr.rhs();

            let mut lhs = self.expr_ty(lhs);
            let mut rhs = self.expr_ty(rhs);

            let other_is_str = lhs.is_str(self.tctx());
            if other_is_str {
                (lhs, rhs) = (rhs, lhs);
            }

            if lhs.is_type(self.tctx()) && (other_is_str || rhs.is_str(self.tctx())) {
                let msg = "comparing strings with types is deprecated";
                let diag = SourceDiagnostic::warning(expr.span(), msg);
                let diag = diag.with_hints([
                    "compare with the literal type instead".into(),
                    "this comparison will always return `false` since typst v0.14".into(),
                ]);
                self.diag.push(diag);
            }
        }
    }

    fn expr_ty<'a>(&self, expr: ast::Expr<'a>) -> TypedExpr<'a> {
        TypedExpr {
            expr,
            ty: self.ti.type_of_span(expr.span()),
        }
    }

    fn check_variable_font<'a>(&mut self, args: impl IntoIterator<Item = ast::Arg<'a>>) {
        for arg in args {
            if let ast::Arg::Named(arg) = arg {
                if arg.name().as_str() == "font" {
                    self.check_variable_font_object(arg.expr().to_untyped());
                    if let Some(array) = arg.expr().to_untyped().cast::<ast::Array>() {
                        for item in array.items() {
                            self.check_variable_font_object(item.to_untyped());
                        }
                    }
                }
            }
        }
    }

    fn check_variable_font_object(&mut self, expr: &SyntaxNode) -> Option<()> {
        if let Some(font_dict) = expr.cast::<ast::Dict>() {
            for item in font_dict.items() {
                if let ast::DictItem::Named(arg) = item {
                    if arg.name().as_str() == "name" {
                        self.check_variable_font_str(arg.expr().to_untyped());
                    }
                }
            }
        }

        self.check_variable_font_str(expr)
    }
    fn check_variable_font_str(&mut self, expr: &SyntaxNode) -> Option<()> {
        if !expr.cast::<ast::Str>()?.get().ends_with("VF") {
            return None;
        }

        let diag =
            SourceDiagnostic::warning(expr.span(), "variable font is not supported by typst yet");
        let diag = diag.with_hint("consider using a static font instead. For more information, see https://github.com/typst/typst/issues/185");
        self.diag.push(diag);

        Some(())
    }
}

impl DataFlowVisitor for Linter {
    fn exprs<'a>(&mut self, exprs: impl DoubleEndedIterator<Item = ast::Expr<'a>>) -> Option<()> {
        for expr in exprs {
            self.expr(expr);
        }
        Some(())
    }

    fn set(&mut self, expr: ast::SetRule<'_>) -> Option<()> {
        if let Some(target) = expr.condition() {
            self.expr(target);
        }
        self.exprs(expr.args().to_untyped().exprs());

        if expr.target().to_untyped().text() == "text" {
            self.check_variable_font(expr.args().items());
        }

        self.expr(expr.target())
    }

    fn show(&mut self, expr: ast::ShowRule<'_>) -> Option<()> {
        if let Some(target) = expr.selector() {
            self.expr(target);
        }
        let transform = expr.transform();
        self.buggy_block_expr(transform, BuggyBlockLoc::Show(expr));
        self.expr(transform)
    }

    fn conditional(&mut self, expr: ast::Conditional<'_>) -> Option<()> {
        self.expr(expr.condition());

        let if_body = expr.if_body();
        self.buggy_block_expr(if_body, BuggyBlockLoc::IfTrue(expr));
        self.expr(if_body);

        if let Some(else_body) = expr.else_body() {
            self.buggy_block_expr(else_body, BuggyBlockLoc::IfFalse(expr));
            self.expr(else_body);
        }

        Some(())
    }

    fn while_loop(&mut self, expr: ast::WhileLoop<'_>) -> Option<()> {
        self.with_loop_info(expr.span(), |this| {
            this.expr(expr.condition());
            let body = expr.body();
            this.buggy_block_expr(body, BuggyBlockLoc::While(expr));
            this.expr(body)
        })
    }

    fn for_loop(&mut self, expr: ast::ForLoop<'_>) -> Option<()> {
        self.with_loop_info(expr.span(), |this| {
            this.expr(expr.iterable());
            let body = expr.body();
            this.buggy_block_expr(body, BuggyBlockLoc::For(expr));
            this.expr(body)
        })
    }

    fn contextual(&mut self, expr: ast::Contextual<'_>) -> Option<()> {
        self.with_func_info(expr.span(), |this| {
            this.loop_info = None;
            this.func_info
                .as_mut()
                .expect("contextual function info")
                .is_contextual = true;
            this.expr(expr.body());
            this.late_func_return(|mut this| this.late_contextual(expr))
        })
    }

    fn closure(&mut self, expr: ast::Closure<'_>) -> Option<()> {
        self.with_func_info(expr.span(), |this| {
            this.loop_info = None;
            this.exprs(expr.params().to_untyped().exprs());
            this.expr(expr.body());
            this.late_func_return(|mut this| this.late_closure(expr))
        })
    }

    fn loop_break(&mut self, expr: ast::LoopBreak<'_>) -> Option<()> {
        if let Some(info) = &mut self.loop_info {
            info.has_break = true;
        } else {
            self.bad_branch_stmt(expr.to_untyped(), "break");
        }
        Some(())
    }

    fn loop_continue(&mut self, expr: ast::LoopContinue<'_>) -> Option<()> {
        if let Some(info) = &mut self.loop_info {
            info.has_continue = true;
        } else {
            self.bad_branch_stmt(expr.to_untyped(), "continue");
        }
        Some(())
    }

    fn func_return(&mut self, expr: ast::FuncReturn<'_>) -> Option<()> {
        if let Some(info) = &mut self.func_info {
            info.has_return = true;
            info.has_return_value = expr.body().is_some();
        } else {
            self.diag.push(SourceDiagnostic::warning(
                expr.span(),
                "`return` statement in a non-function context",
            ));
        }
        Some(())
    }

    fn binary(&mut self, expr: ast::Binary<'_>) -> Option<()> {
        self.check_type_compare(expr);
        self.exprs([expr.lhs(), expr.rhs()].into_iter())
    }

    fn func_call(&mut self, expr: ast::FuncCall<'_>) -> Option<()> {
        // warn if text(font: ("Font Name", "Font Name")) in which Font Name ends with
        // "VF"
        if expr.callee().to_untyped().text() == "text" {
            self.check_variable_font(expr.args().items());
        }
        Some(())
    }
}

struct LateFuncLinter<'a> {
    linter: &'a mut Linter,
    func_info: FuncInfo,
    return_block_info: Option<ReturnBlockInfo>,
}

impl LateFuncLinter<'_> {
    fn late_closure(&mut self, expr: ast::Closure<'_>) -> Option<()> {
        if !self.func_info.has_return {
            return Some(());
        }
        self.expr(expr.body())
    }

    fn late_contextual(&mut self, expr: ast::Contextual<'_>) -> Option<()> {
        if !self.func_info.has_return {
            return Some(());
        }
        self.expr(expr.body())
    }

    fn join(&mut self, parent: Option<ReturnBlockInfo>) {
        if let Some(parent) = parent {
            match &mut self.return_block_info {
                Some(info) => {
                    if info.return_value == parent.return_value {
                        return;
                    }

                    // Merge the two return block info
                    *info = parent.merge(std::mem::take(info));
                }
                info @ None => {
                    *info = Some(parent);
                }
            }
        }
    }
}

impl DataFlowVisitor for LateFuncLinter<'_> {
    fn exprs<'a>(&mut self, exprs: impl DoubleEndedIterator<Item = ast::Expr<'a>>) -> Option<()> {
        for expr in exprs.rev() {
            self.expr(expr);
        }
        Some(())
    }

    fn block<'a>(&mut self, exprs: impl DoubleEndedIterator<Item = ast::Expr<'a>>) -> Option<()> {
        self.exprs(exprs)
    }

    fn loop_break(&mut self, _expr: ast::LoopBreak<'_>) -> Option<()> {
        self.return_block_info = Some(ReturnBlockInfo {
            return_value: false,
            return_none: false,
            warned: false,
        });
        Some(())
    }

    fn loop_continue(&mut self, _expr: ast::LoopContinue<'_>) -> Option<()> {
        self.return_block_info = Some(ReturnBlockInfo {
            return_value: false,
            return_none: false,
            warned: false,
        });
        Some(())
    }

    fn func_return(&mut self, expr: ast::FuncReturn<'_>) -> Option<()> {
        if expr.body().is_some() {
            self.return_block_info = Some(ReturnBlockInfo {
                return_value: true,
                return_none: false,
                warned: false,
            });
        } else {
            self.return_block_info = Some(ReturnBlockInfo {
                return_value: false,
                return_none: true,
                warned: false,
            });
        }
        Some(())
    }

    fn closure(&mut self, expr: ast::Closure<'_>) -> Option<()> {
        let ident = expr.name().map(ast::Expr::Ident).into_iter();
        let params = expr.params().to_untyped().exprs();
        // the body is ignored in the return stmt analysis
        let _body = expr.body().once();
        self.exprs(ident.chain(params))
    }

    fn contextual(&mut self, expr: ast::Contextual<'_>) -> Option<()> {
        // the body is ignored in the return stmt analysis
        let _body = expr.body();
        Some(())
    }

    fn equation(&mut self, expr: ast::Equation<'_>) -> Option<()> {
        self.value(ast::Expr::Equation(expr));
        Some(())
    }

    fn array(&mut self, expr: ast::Array<'_>) -> Option<()> {
        self.value(ast::Expr::Array(expr));
        Some(())
    }

    fn dict(&mut self, expr: ast::Dict<'_>) -> Option<()> {
        self.value(ast::Expr::Dict(expr));
        Some(())
    }

    fn func_call(&mut self, _expr: ast::FuncCall<'_>) -> Option<()> {
        Some(())
    }

    fn let_binding(&mut self, _expr: ast::LetBinding<'_>) -> Option<()> {
        Some(())
    }

    fn destruct_assign(&mut self, _expr: ast::DestructAssignment<'_>) -> Option<()> {
        Some(())
    }

    fn conditional(&mut self, expr: ast::Conditional<'_>) -> Option<()> {
        let if_body = expr.if_body();
        let else_body = expr.else_body();

        let parent = self.return_block_info.clone();
        self.exprs(if_body.once());
        let if_branch = std::mem::replace(&mut self.return_block_info, parent.clone());
        self.exprs(else_body.into_iter());
        // else_branch
        self.join(if_branch);

        Some(())
    }

    fn value(&mut self, expr: ast::Expr) -> Option<()> {
        let ri = self.return_block_info.as_mut()?;
        if ri.warned {
            return None;
        }
        if matches!(expr, ast::Expr::None(..)) || expr.to_untyped().kind().is_trivia() {
            return None;
        }

        if ri.return_value {
            ri.warned = true;
            let diag = SourceDiagnostic::warning(
                expr.span(),
                eco_format!(
                    "This {} is implicitly discarded by function return",
                    expr.to_untyped().kind().name()
                ),
            );
            let diag = match expr {
                ast::Expr::Show(..) | ast::Expr::Set(..) => diag,
                expr if expr.hash() => diag.with_hint(eco_format!(
                    "consider ignoring the value explicitly using underscore: `let _ = {}`",
                    expr.to_untyped().clone().into_text()
                )),
                _ => diag,
            };
            self.linter.diag.push(diag);
        } else if ri.return_none && matches!(expr, ast::Expr::Show(..) | ast::Expr::Set(..)) {
            ri.warned = true;
            let diag = SourceDiagnostic::warning(
                expr.span(),
                eco_format!(
                    "This {} is implicitly discarded by function return",
                    expr.to_untyped().kind().name()
                ),
            );
            self.linter.diag.push(diag);
        }

        Some(())
    }

    fn field_access(&mut self, _expr: ast::FieldAccess<'_>) -> Option<()> {
        Some(())
    }

    fn show(&mut self, expr: ast::ShowRule<'_>) -> Option<()> {
        self.value(ast::Expr::Show(expr));
        Some(())
    }

    fn set(&mut self, expr: ast::SetRule<'_>) -> Option<()> {
        self.value(ast::Expr::Set(expr));
        Some(())
    }

    fn for_loop(&mut self, expr: ast::ForLoop<'_>) -> Option<()> {
        self.expr(expr.body())
    }

    fn while_loop(&mut self, expr: ast::WhileLoop<'_>) -> Option<()> {
        self.expr(expr.body())
    }

    fn include(&mut self, expr: ast::ModuleInclude<'_>) -> Option<()> {
        self.value(ast::Expr::Include(expr));
        Some(())
    }
}

#[derive(Clone, Default)]
struct ReturnBlockInfo {
    return_value: bool,
    return_none: bool,
    warned: bool,
}

impl ReturnBlockInfo {
    fn merge(self, other: Self) -> Self {
        Self {
            return_value: self.return_value && other.return_value,
            return_none: self.return_none && other.return_none,
            warned: self.warned && other.warned,
        }
    }
}

trait DataFlowVisitor {
    fn expr(&mut self, expr: ast::Expr) -> Option<()> {
        match expr {
            ast::Expr::Parenthesized(expr) => self.expr(expr.expr()),
            ast::Expr::Code(expr) => self.block(expr.body().exprs()),
            ast::Expr::Content(expr) => self.block(expr.body().exprs()),
            ast::Expr::Math(expr) => self.exprs(expr.exprs()),

            ast::Expr::Text(..) => self.value(expr),
            ast::Expr::Space(..) => self.value(expr),
            ast::Expr::Linebreak(..) => self.value(expr),
            ast::Expr::Parbreak(..) => self.value(expr),
            ast::Expr::Escape(..) => self.value(expr),
            ast::Expr::Shorthand(..) => self.value(expr),
            ast::Expr::SmartQuote(..) => self.value(expr),
            ast::Expr::Raw(..) => self.value(expr),
            ast::Expr::Link(..) => self.value(expr),

            ast::Expr::Label(..) => self.value(expr),
            ast::Expr::Ref(..) => self.value(expr),
            ast::Expr::None(..) => self.value(expr),
            ast::Expr::Auto(..) => self.value(expr),
            ast::Expr::Bool(..) => self.value(expr),
            ast::Expr::Int(..) => self.value(expr),
            ast::Expr::Float(..) => self.value(expr),
            ast::Expr::Numeric(..) => self.value(expr),
            ast::Expr::Str(..) => self.value(expr),
            ast::Expr::MathText(..) => self.value(expr),
            ast::Expr::MathShorthand(..) => self.value(expr),
            ast::Expr::MathAlignPoint(..) => self.value(expr),
            ast::Expr::MathPrimes(..) => self.value(expr),
            ast::Expr::MathRoot(..) => self.value(expr),

            ast::Expr::Strong(content) => self.exprs(content.body().exprs()),
            ast::Expr::Emph(content) => self.exprs(content.body().exprs()),
            ast::Expr::Heading(content) => self.exprs(content.body().exprs()),
            ast::Expr::List(content) => self.exprs(content.body().exprs()),
            ast::Expr::Enum(content) => self.exprs(content.body().exprs()),
            ast::Expr::Term(content) => {
                self.exprs(content.term().exprs().chain(content.description().exprs()))
            }
            ast::Expr::MathDelimited(content) => self.exprs(content.body().exprs()),
            ast::Expr::MathAttach(..) | ast::Expr::MathFrac(..) => self.exprs(expr.exprs()),

            ast::Expr::Ident(expr) => self.ident(expr),
            ast::Expr::MathIdent(expr) => self.math_ident(expr),
            ast::Expr::Equation(expr) => self.equation(expr),
            ast::Expr::Array(expr) => self.array(expr),
            ast::Expr::Dict(expr) => self.dict(expr),
            ast::Expr::Unary(expr) => self.unary(expr),
            ast::Expr::Binary(expr) => self.binary(expr),
            ast::Expr::FieldAccess(expr) => self.field_access(expr),
            ast::Expr::FuncCall(expr) => self.func_call(expr),
            ast::Expr::Closure(expr) => self.closure(expr),
            ast::Expr::Let(expr) => self.let_binding(expr),
            ast::Expr::DestructAssign(expr) => self.destruct_assign(expr),
            ast::Expr::Set(expr) => self.set(expr),
            ast::Expr::Show(expr) => self.show(expr),
            ast::Expr::Contextual(expr) => self.contextual(expr),
            ast::Expr::Conditional(expr) => self.conditional(expr),
            ast::Expr::While(expr) => self.while_loop(expr),
            ast::Expr::For(expr) => self.for_loop(expr),
            ast::Expr::Import(expr) => self.import(expr),
            ast::Expr::Include(expr) => self.include(expr),
            ast::Expr::Break(expr) => self.loop_break(expr),
            ast::Expr::Continue(expr) => self.loop_continue(expr),
            ast::Expr::Return(expr) => self.func_return(expr),
        }
    }

    fn exprs<'a>(&mut self, exprs: impl DoubleEndedIterator<Item = ast::Expr<'a>>) -> Option<()> {
        for expr in exprs {
            self.expr(expr);
        }
        Some(())
    }

    fn block<'a>(&mut self, exprs: impl DoubleEndedIterator<Item = ast::Expr<'a>>) -> Option<()> {
        self.exprs(exprs)
    }

    fn value(&mut self, _expr: ast::Expr) -> Option<()> {
        Some(())
    }

    fn ident(&mut self, _expr: ast::Ident<'_>) -> Option<()> {
        Some(())
    }

    fn math_ident(&mut self, _expr: ast::MathIdent<'_>) -> Option<()> {
        Some(())
    }

    fn import(&mut self, _expr: ast::ModuleImport<'_>) -> Option<()> {
        Some(())
    }

    fn include(&mut self, _expr: ast::ModuleInclude<'_>) -> Option<()> {
        Some(())
    }

    fn equation(&mut self, expr: ast::Equation<'_>) -> Option<()> {
        self.exprs(expr.body().exprs())
    }

    fn array(&mut self, expr: ast::Array<'_>) -> Option<()> {
        self.exprs(expr.to_untyped().exprs())
    }

    fn dict(&mut self, expr: ast::Dict<'_>) -> Option<()> {
        self.exprs(expr.to_untyped().exprs())
    }

    fn unary(&mut self, expr: ast::Unary<'_>) -> Option<()> {
        self.expr(expr.expr())
    }

    fn binary(&mut self, expr: ast::Binary<'_>) -> Option<()> {
        self.exprs([expr.lhs(), expr.rhs()].into_iter())
    }

    fn field_access(&mut self, expr: ast::FieldAccess<'_>) -> Option<()> {
        self.expr(expr.target())
    }

    fn func_call(&mut self, expr: ast::FuncCall<'_>) -> Option<()> {
        self.exprs(expr.args().to_untyped().exprs().chain(expr.callee().once()))
    }

    fn closure(&mut self, expr: ast::Closure<'_>) -> Option<()> {
        let ident = expr.name().map(ast::Expr::Ident).into_iter();
        let params = expr.params().to_untyped().exprs();
        let body = expr.body().once();
        self.exprs(ident.chain(params).chain(body))
    }

    fn let_binding(&mut self, expr: ast::LetBinding<'_>) -> Option<()> {
        self.expr(expr.init()?)
    }

    fn destruct_assign(&mut self, expr: ast::DestructAssignment<'_>) -> Option<()> {
        self.expr(expr.value())
    }

    fn set(&mut self, expr: ast::SetRule<'_>) -> Option<()> {
        let cond = expr.condition().into_iter();
        let args = expr.args().to_untyped().exprs();
        self.exprs(cond.chain(args).chain(expr.target().once()))
    }

    fn show(&mut self, expr: ast::ShowRule<'_>) -> Option<()> {
        let selector = expr.selector().into_iter();
        let transform = expr.transform();
        self.exprs(selector.chain(transform.once()))
    }

    fn contextual(&mut self, expr: ast::Contextual<'_>) -> Option<()> {
        self.expr(expr.body())
    }

    fn conditional(&mut self, expr: ast::Conditional<'_>) -> Option<()> {
        let cond = expr.condition().once();
        let if_body = expr.if_body().once();
        let else_body = expr.else_body().into_iter();
        self.exprs(cond.chain(if_body).chain(else_body))
    }

    fn while_loop(&mut self, expr: ast::WhileLoop<'_>) -> Option<()> {
        let cond = expr.condition().once();
        let body = expr.body().once();
        self.exprs(cond.chain(body))
    }

    fn for_loop(&mut self, expr: ast::ForLoop<'_>) -> Option<()> {
        let iterable = expr.iterable().once();
        let body = expr.body().once();
        self.exprs(iterable.chain(body))
    }

    fn loop_break(&mut self, _expr: ast::LoopBreak<'_>) -> Option<()> {
        Some(())
    }

    fn loop_continue(&mut self, _expr: ast::LoopContinue<'_>) -> Option<()> {
        Some(())
    }

    fn func_return(&mut self, expr: ast::FuncReturn<'_>) -> Option<()> {
        self.expr(expr.body()?)
    }
}

trait ExprsUntyped {
    fn exprs(&self) -> impl DoubleEndedIterator<Item = ast::Expr<'_>>;
}

impl ExprsUntyped for ast::Expr<'_> {
    fn exprs(&self) -> impl DoubleEndedIterator<Item = ast::Expr<'_>> {
        self.to_untyped().exprs()
    }
}

impl ExprsUntyped for SyntaxNode {
    fn exprs(&self) -> impl DoubleEndedIterator<Item = ast::Expr<'_>> {
        self.children().filter_map(SyntaxNode::cast)
    }
}

trait ExprsOnce<'a> {
    fn once(self) -> impl DoubleEndedIterator<Item = ast::Expr<'a>>;
}

impl<'a> ExprsOnce<'a> for ast::Expr<'a> {
    fn once(self) -> impl DoubleEndedIterator<Item = ast::Expr<'a>> {
        std::iter::once(self)
    }
}

#[derive(Clone)]
struct LoopInfo {
    span: Span,
    has_break: bool,
    has_continue: bool,
}

#[derive(Clone)]
struct FuncInfo {
    span: Span,
    is_contextual: bool,
    has_return: bool,
    has_return_value: bool,
    parent_loop: Option<LoopInfo>,
}

#[derive(Clone, Copy)]
enum Block<'a> {
    Code(ast::Code<'a>),
    Markup(ast::Markup<'a>),
}

impl<'a> Block<'a> {
    fn from(expr: ast::Expr<'a>) -> Option<Self> {
        Some(match expr {
            ast::Expr::Code(block) => Block::Code(block.body()),
            ast::Expr::Content(block) => Block::Markup(block.body()),
            _ => return None,
        })
    }

    #[inline(always)]
    fn iter(&self) -> impl Iterator<Item = ast::Expr<'a>> {
        let (x, y) = match self {
            Block::Code(block) => (Some(block.exprs()), None),
            Block::Markup(block) => (None, Some(block.exprs())),
        };

        x.into_iter().flatten().chain(y.into_iter().flatten())
    }
}

#[derive(Debug, Clone)]
struct TypedExpr<'a> {
    expr: ast::Expr<'a>,
    ty: Option<Ty>,
}

impl TypedExpr<'_> {
    fn is_str(&self, ctx: &impl TyCtx) -> bool {
        self.ty
            .as_ref()
            .map(|ty| ty.is_str(ctx))
            .unwrap_or_else(|| matches!(self.expr, ast::Expr::Str(..)))
    }

    fn is_type(&self, ctx: &impl TyCtx) -> bool {
        self.ty
            .as_ref()
            .map(|ty| ty.is_type(ctx))
            .unwrap_or_default()
    }
}

enum BuggyBlockLoc<'a> {
    Show(ast::ShowRule<'a>),
    IfTrue(ast::Conditional<'a>),
    IfFalse(ast::Conditional<'a>),
    While(ast::WhileLoop<'a>),
    For(ast::ForLoop<'a>),
}

impl BuggyBlockLoc<'_> {
    fn hint(&self, show_set: ast::Expr<'_>) -> EcoString {
        match self {
            BuggyBlockLoc::Show(show_parent) => {
                if let ast::Expr::Show(show) = show_set {
                    eco_format!(
                        "consider changing parent to `show {}: it => {{ {}; it }}`",
                        match show_parent.selector() {
                            Some(selector) => selector.to_untyped().clone().into_text(),
                            None => "".into(),
                        },
                        show.to_untyped().clone().into_text()
                    )
                } else {
                    eco_format!(
                        "consider changing parent to `show {}: {}`",
                        match show_parent.selector() {
                            Some(selector) => selector.to_untyped().clone().into_text(),
                            None => "".into(),
                        },
                        show_set.to_untyped().clone().into_text()
                    )
                }
            }
            BuggyBlockLoc::IfTrue(conditional) | BuggyBlockLoc::IfFalse(conditional) => {
                let neg = if matches!(self, BuggyBlockLoc::IfTrue(..)) {
                    ""
                } else {
                    "not "
                };
                if let ast::Expr::Show(show) = show_set {
                    eco_format!(
                        "consider changing parent to `show {}: if {neg}({}) {{ .. }}`",
                        match show.selector() {
                            Some(selector) => selector.to_untyped().clone().into_text(),
                            None => "".into(),
                        },
                        conditional.condition().to_untyped().clone().into_text()
                    )
                } else {
                    eco_format!(
                        "consider changing parent to `{} if {neg}({})`",
                        show_set.to_untyped().clone().into_text(),
                        conditional.condition().to_untyped().clone().into_text()
                    )
                }
            }
            BuggyBlockLoc::While(w) => {
                eco_format!(
                    "consider changing parent to `show: it => if {} {{ {}; it }}`",
                    w.condition().to_untyped().clone().into_text(),
                    show_set.to_untyped().clone().into_text()
                )
            }
            BuggyBlockLoc::For(f) => {
                eco_format!(
                    "consider changing parent to `show: {}.fold(it => it, (style-it, {}) => it => {{ {}; style-it(it) }})`",
                    f.iterable().to_untyped().clone().into_text(),
                    f.pattern().to_untyped().clone().into_text(),
                    show_set.to_untyped().clone().into_text()
                )
            }
        }
    }
}

fn is_show_set(it: ast::Expr) -> bool {
    matches!(it, ast::Expr::Set(..) | ast::Expr::Show(..))
}

fn is_compare_op(op: ast::BinOp) -> bool {
    use ast::BinOp::*;
    matches!(op, Lt | Leq | Gt | Geq | Eq | Neq)
}
