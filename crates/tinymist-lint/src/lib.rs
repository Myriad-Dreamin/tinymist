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

    fn lint(mut self, node: &SyntaxNode) -> DiagnosticVec {
        if let Some(markup) = node.cast::<ast::Markup>() {
            self.exprs(markup.exprs());
        } else if let Some(expr) = node.cast() {
            self.expr(expr);
        }

        self.diag
    }

    fn exprs<'a>(&mut self, exprs: impl Iterator<Item = ast::Expr<'a>>) -> Option<()> {
        for expr in exprs {
            self.expr(expr);
        }
        Some(())
    }

    fn exprs_untyped(&mut self, to_untyped: &SyntaxNode) -> Option<()> {
        for expr in to_untyped.children() {
            if let Some(expr) = expr.cast() {
                self.expr(expr);
            }
        }
        Some(())
    }

    fn expr(&mut self, node: ast::Expr) -> Option<()> {
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

    fn array(&mut self, expr: ast::Array<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn dict(&mut self, expr: ast::Dict<'_>) -> Option<()> {
        self.exprs_untyped(expr.to_untyped())
    }

    fn unary(&mut self, expr: ast::Unary<'_>) -> Option<()> {
        self.expr(expr.expr())
    }

    fn binary(&mut self, expr: ast::Binary<'_>) -> Option<()> {
        self.expr(expr.lhs());
        self.expr(expr.rhs())
    }

    fn field_access(&mut self, expr: ast::FieldAccess<'_>) -> Option<()> {
        self.expr(expr.target())
    }

    fn func_call(&mut self, expr: ast::FuncCall<'_>) -> Option<()> {
        self.exprs_untyped(expr.args().to_untyped());
        self.expr(expr.callee())
    }

    fn closure(&mut self, expr: ast::Closure<'_>) -> Option<()> {
        self.exprs_untyped(expr.params().to_untyped());
        self.expr(expr.body())
    }

    fn let_binding(&mut self, expr: ast::LetBinding<'_>) -> Option<()> {
        self.expr(expr.init()?)
    }

    fn destruct_assign(&mut self, expr: ast::DestructAssignment<'_>) -> Option<()> {
        self.expr(expr.value())
    }

    fn set(&mut self, expr: ast::SetRule<'_>) -> Option<()> {
        if let Some(target) = expr.condition() {
            self.expr(target);
        }
        self.exprs_untyped(expr.args().to_untyped());
        self.expr(expr.target())
    }

    fn show(&mut self, expr: ast::ShowRule<'_>) -> Option<()> {
        if let Some(target) = expr.selector() {
            self.expr(target);
        }
        let transform = expr.transform();
        match transform {
            ast::Expr::Code(..) | ast::Expr::Content(..) => self.buggy_set(transform),
            _ => None,
        };

        self.expr(transform)
    }

    fn contextual(&mut self, expr: ast::Contextual<'_>) -> Option<()> {
        self.expr(expr.body())
    }

    fn conditional(&mut self, expr: ast::Conditional<'_>) -> Option<()> {
        self.expr(expr.condition());

        let if_body = expr.if_body();
        self.buggy_set(if_body);
        self.expr(if_body);

        if let Some(else_body) = expr.else_body() {
            self.buggy_set(else_body);
            self.expr(else_body);
        }

        Some(())
    }

    fn while_loop(&mut self, expr: ast::WhileLoop<'_>) -> Option<()> {
        self.expr(expr.condition());
        let body = expr.body();
        self.buggy_set(body);
        self.expr(body)
    }

    fn for_loop(&mut self, expr: ast::ForLoop<'_>) -> Option<()> {
        self.expr(expr.iterable());
        let body = expr.body();
        self.buggy_set(body);
        self.expr(body)
    }

    fn loop_break(&mut self, _expr: ast::LoopBreak<'_>) -> Option<()> {
        Some(())
    }

    fn loop_continue(&mut self, _expr: ast::LoopContinue<'_>) -> Option<()> {
        Some(())
    }

    fn func_return(&mut self, _expr: ast::FuncReturn<'_>) -> Option<()> {
        Some(())
    }

    fn buggy_set(&mut self, expr: ast::Expr) -> Option<()> {
        if self.only_set(expr) {
            let sets = match expr {
                ast::Expr::Code(block) => block
                    .body()
                    .exprs()
                    .filter(|it| is_show_set(*it))
                    .collect::<Vec<_>>(),
                ast::Expr::Content(block) => block
                    .body()
                    .exprs()
                    .filter(|it| is_show_set(*it))
                    .collect::<Vec<_>>(),
                _ => return None,
            };

            for set in sets {
                match set {
                    ast::Expr::Set(..) => {
                        self.diag.push(SourceDiagnostic::warning(
                            set.span(),
                            "This set statement doesn't take effect.",
                        ));
                    }
                    ast::Expr::Show(..) => {
                        self.diag.push(SourceDiagnostic::warning(
                            set.span(),
                            "This show statement doesn't take effect.",
                        ));
                    }
                    _ => {}
                }
            }

            return None;
        }

        Some(())
    }

    fn only_set(&mut self, expr: ast::Expr) -> bool {
        let mut has_set = false;

        match expr {
            ast::Expr::Code(block) => {
                for it in block.body().exprs() {
                    if is_show_set(it) {
                        has_set = true;
                    } else {
                        return false;
                    }
                }
            }
            ast::Expr::Content(block) => {
                for it in block.body().exprs() {
                    if is_show_set(it) {
                        has_set = true;
                    } else if !it.to_untyped().kind().is_trivia() {
                        return false;
                    }
                }
            }
            _ => {
                return false;
            }
        }

        has_set
    }
}

fn is_show_set(it: ast::Expr) -> bool {
    matches!(it, ast::Expr::Set(..) | ast::Expr::Show(..))
}
