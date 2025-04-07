//! A linter for Typst.

use typst::{
    diag::SourceDiagnostic,
    ecow::EcoVec,
    syntax::{
        ast::{self, AstNode},
        Source, SyntaxNode,
    },
};

/// A type alias for a vector of diagnostics.
type DiagnosticVec = EcoVec<SourceDiagnostic>;

/// Lints a Typst source and returns a vector of diagnostics.
pub fn lint_source(source: &Source) -> DiagnosticVec {
    SourceLinter::new().lint(source.root())
}

struct SourceLinter {
    diag: DiagnosticVec,
}

impl SourceLinter {
    fn new() -> Self {
        Self {
            diag: EcoVec::new(),
        }
    }

    fn lint(self, node: &SyntaxNode) -> DiagnosticVec {
        if let Some(expr) = node.cast() {
            self.expr(expr);
        }

        self.diag
    }

    fn exprs<'a>(&self, exprs: impl Iterator<Item = ast::Expr<'a>>) -> Option<()> {
        for expr in exprs {
            self.expr(expr);
        }
        Some(())
    }

    fn exprs_untyped(&self, to_untyped: &SyntaxNode) -> Option<()> {
        for expr in to_untyped.children() {
            if let Some(expr) = expr.cast() {
                self.expr(expr);
            }
        }
        Some(())
    }

    fn expr(&self, node: ast::Expr) -> Option<()> {
        match node {
            ast::Expr::Parenthesized(expr) => self.expr(expr.expr()),
            ast::Expr::Code(expr) => self.exprs(expr.body().exprs()),
            ast::Expr::Content(expr) => self.exprs(expr.body().exprs()),
            ast::Expr::Equation(expr) => self.exprs(expr.body().exprs()),
            ast::Expr::Math(expr) => self.exprs(expr.exprs()),

            ast::Expr::Text(..) => None,
            ast::Expr::Space(..) => None,
            ast::Expr::Linebreak(..) => None,
            ast::Expr::Parbreak(..) => None,
            ast::Expr::Escape(..) => None,
            ast::Expr::Shorthand(..) => None,
            ast::Expr::SmartQuote(..) => None,
            ast::Expr::Raw(..) => None,
            ast::Expr::Link(..) => None,

            ast::Expr::Label(..) => None,
            ast::Expr::Ref(..) => None,
            ast::Expr::None(..) => None,
            ast::Expr::Auto(..) => None,
            ast::Expr::Bool(..) => None,
            ast::Expr::Int(..) => None,
            ast::Expr::Float(..) => None,
            ast::Expr::Numeric(..) => None,
            ast::Expr::Str(..) => None,
            ast::Expr::MathText(..) => None,
            ast::Expr::MathShorthand(..) => None,
            ast::Expr::MathAlignPoint(..) => None,
            ast::Expr::MathPrimes(..) => None,
            ast::Expr::MathRoot(..) => None,

            ast::Expr::Strong(content) => self.exprs(content.body().exprs()),
            ast::Expr::Emph(content) => self.exprs(content.body().exprs()),
            ast::Expr::Heading(content) => self.exprs(content.body().exprs()),
            ast::Expr::List(content) => self.exprs(content.body().exprs()),
            ast::Expr::Enum(content) => self.exprs(content.body().exprs()),
            ast::Expr::Term(content) => {
                self.exprs(content.term().exprs());
                self.exprs(content.description().exprs())
            }
            ast::Expr::MathDelimited(content) => self.exprs(content.body().exprs()),
            ast::Expr::MathAttach(..) | ast::Expr::MathFrac(..) => {
                self.exprs_untyped(node.to_untyped())
            }

            ast::Expr::Ident(expr) => self.ident(expr),
            ast::Expr::MathIdent(expr) => self.math_ident(expr),
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

    fn ident(&self, _expr: ast::Ident<'_>) -> Option<()> {
        Some(())
    }

    fn math_ident(&self, _expr: ast::MathIdent<'_>) -> Option<()> {
        Some(())
    }

    fn import(&self, _expr: ast::ModuleImport<'_>) -> Option<()> {
        Some(())
    }

    fn include(&self, _expr: ast::ModuleInclude<'_>) -> Option<()> {
        Some(())
    }

    fn array(&self, expr: ast::Array<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn dict(&self, expr: ast::Dict<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn unary(&self, expr: ast::Unary<'_>) -> Option<()> {
        self.expr(expr.expr())
    }

    fn binary(&self, expr: ast::Binary<'_>) -> Option<()> {
        self.expr(expr.lhs());
        self.expr(expr.rhs())
    }

    fn field_access(&self, expr: ast::FieldAccess<'_>) -> Option<()> {
        self.expr(expr.target())
    }

    fn func_call(&self, expr: ast::FuncCall<'_>) -> Option<()> {
        self.exprs_untyped(expr.args().to_untyped());
        self.expr(expr.callee())
    }

    fn closure(&self, expr: ast::Closure<'_>) -> Option<()> {
        self.exprs_untyped(expr.params().to_untyped());
        self.expr(expr.body())
    }

    fn let_binding(&self, expr: ast::LetBinding<'_>) -> Option<()> {
        self.expr(expr.init()?)
    }

    fn destruct_assign(&self, expr: ast::DestructAssignment<'_>) -> Option<()> {
        self.expr(expr.value())
    }

    fn set(&self, expr: ast::SetRule<'_>) -> Option<()> {
        if let Some(target) = expr.condition() {
            self.expr(target);
        }
        self.exprs_untyped(expr.args().to_untyped());
        self.expr(expr.target())
    }

    fn show(&self, expr: ast::ShowRule<'_>) -> Option<()> {
        if let Some(target) = expr.selector() {
            self.expr(target);
        }
        self.expr(expr.transform())
    }

    fn contextual(&self, expr: ast::Contextual<'_>) -> Option<()> {
        self.expr(expr.body())
    }

    fn conditional(&self, expr: ast::Conditional<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn while_loop(&self, expr: ast::WhileLoop<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn for_loop(&self, expr: ast::ForLoop<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn loop_break(&self, _expr: ast::LoopBreak<'_>) -> Option<()> {
        Some(())
    }

    fn loop_continue(&self, _expr: ast::LoopContinue<'_>) -> Option<()> {
        Some(())
    }

    fn func_return(&self, _expr: ast::FuncReturn<'_>) -> Option<()> {
        Some(())
    }
}
