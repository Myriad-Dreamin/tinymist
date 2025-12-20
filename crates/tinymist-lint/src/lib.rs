//! A linter for Typst.

use std::sync::Arc;

use tinymist_analysis::{
    adt::interner::Interned,
    syntax::{Decl, ExprInfo},
    ty::{Ty, TyCtx, TypeInfo},
};
use tinymist_project::LspWorld;
use typst::{
    diag::{EcoString, SourceDiagnostic, Tracepoint, eco_format},
    ecow::EcoVec,
    syntax::{
        FileId, Span, Spanned, SyntaxNode,
        ast::{self, AstNode},
    },
};

mod cfg;

/// A type alias for a vector of diagnostics.
type DiagnosticVec = EcoVec<SourceDiagnostic>;

/// The lint information about a file.
#[derive(Debug, Clone)]
pub struct LintInfo {
    /// The revision of expression information
    pub revision: usize,
    /// The belonging file id
    pub fid: FileId,
    /// The diagnostics
    pub diagnostics: DiagnosticVec,
}

/// Performs linting check on file and returns a vector of diagnostics.
pub fn lint_file(
    world: &LspWorld,
    ei: &ExprInfo,
    ti: Arc<TypeInfo>,
    known_issues: KnownIssues,
) -> LintInfo {
    let diagnostics = Linter::new(world, ei.clone(), ti, known_issues).lint(ei.source.root());
    LintInfo {
        revision: ei.revision,
        fid: ei.fid,
        diagnostics,
    }
}

/// Information about issues the linter checks for that will already be reported
/// to the user via other means (such as compiler diagnostics), to avoid
/// duplicating warnings.
#[derive(Default, Clone, Hash)]
pub struct KnownIssues {
    unknown_vars: EcoVec<Span>,
}

impl KnownIssues {
    /// Collects known lint issues from the given compiler diagnostics.
    pub fn from_compiler_diagnostics<'a>(
        diags: impl Iterator<Item = &'a SourceDiagnostic>,
    ) -> Self {
        let mut unknown_vars = Vec::default();
        for diag in diags {
            if diag.message.starts_with("unknown variable") {
                unknown_vars.push(diag.span);
            }
        }
        unknown_vars.sort_by_key(|span| span.into_raw());
        let unknown_vars = EcoVec::from(unknown_vars);
        Self { unknown_vars }
    }

    pub(crate) fn has_unknown_math_ident(&self, ident: ast::MathIdent<'_>) -> bool {
        self.unknown_vars.contains(&ident.span())
    }
}

struct Linter<'w> {
    world: &'w LspWorld,
    ei: ExprInfo,
    ti: Arc<TypeInfo>,
    known_issues: KnownIssues,
    diag: DiagnosticVec,
    loop_info: Option<LoopInfo>,
    func_info: Option<FuncInfo>,
}

impl<'w> Linter<'w> {
    fn new(
        world: &'w LspWorld,
        ei: ExprInfo,
        ti: Arc<TypeInfo>,
        known_issues: KnownIssues,
    ) -> Self {
        Self {
            world,
            ei,
            ti,
            known_issues,
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

    fn expr(&mut self, expr: ast::Expr) -> Option<()> {
        match expr {
            ast::Expr::Parenthesized(expr) => self.expr(expr.expr()),
            ast::Expr::CodeBlock(expr) => self.exprs(expr.body().exprs()),
            ast::Expr::ContentBlock(expr) => self.exprs(expr.body().exprs()),
            ast::Expr::Math(expr) => self.exprs(expr.exprs()),

            ast::Expr::Text(..)
            | ast::Expr::Space(..)
            | ast::Expr::Linebreak(..)
            | ast::Expr::Parbreak(..)
            | ast::Expr::Escape(..)
            | ast::Expr::Shorthand(..)
            | ast::Expr::SmartQuote(..)
            | ast::Expr::Raw(..)
            | ast::Expr::Link(..)
            | ast::Expr::Label(..)
            | ast::Expr::Ref(..)
            | ast::Expr::None(..)
            | ast::Expr::Auto(..)
            | ast::Expr::Bool(..)
            | ast::Expr::Int(..)
            | ast::Expr::Float(..)
            | ast::Expr::Numeric(..)
            | ast::Expr::Str(..)
            | ast::Expr::MathText(..)
            | ast::Expr::MathShorthand(..)
            | ast::Expr::MathAlignPoint(..)
            | ast::Expr::MathPrimes(..)
            | ast::Expr::MathRoot(..) => Some(()),

            ast::Expr::Strong(content) => self.exprs(content.body().exprs()),
            ast::Expr::Emph(content) => self.exprs(content.body().exprs()),
            ast::Expr::Heading(content) => self.exprs(content.body().exprs()),
            ast::Expr::ListItem(content) => self.exprs(content.body().exprs()),
            ast::Expr::EnumItem(content) => self.exprs(content.body().exprs()),
            ast::Expr::TermItem(content) => {
                self.exprs(content.term().exprs().chain(content.description().exprs()))
            }
            ast::Expr::MathDelimited(content) => self.exprs(content.body().exprs()),
            ast::Expr::MathAttach(..) | ast::Expr::MathFrac(..) => self.exprs(expr.exprs()),

            ast::Expr::Ident(..) => Some(()),
            ast::Expr::MathIdent(expr) => self.math_ident(expr),
            ast::Expr::Equation(expr) => self.exprs(expr.body().exprs()),
            ast::Expr::Array(expr) => self.exprs(expr.to_untyped().exprs()),
            ast::Expr::Dict(expr) => self.exprs(expr.to_untyped().exprs()),
            ast::Expr::Unary(expr) => self.expr(expr.expr()),
            ast::Expr::Binary(expr) => self.binary(expr),
            ast::Expr::FieldAccess(expr) => self.expr(expr.target()),
            ast::Expr::FuncCall(expr) => self.func_call(expr),
            ast::Expr::Closure(expr) => self.closure(expr),
            ast::Expr::LetBinding(expr) => self.expr(expr.init()?),
            ast::Expr::DestructAssignment(expr) => self.expr(expr.value()),
            ast::Expr::SetRule(expr) => self.set(expr),
            ast::Expr::ShowRule(expr) => self.show(expr),
            ast::Expr::Contextual(expr) => self.contextual(expr),
            ast::Expr::Conditional(expr) => self.conditional(expr),
            ast::Expr::WhileLoop(expr) => self.while_loop(expr),
            ast::Expr::ForLoop(expr) => self.for_loop(expr),
            ast::Expr::ModuleImport(..) | ast::Expr::ModuleInclude(..) => Some(()),
            ast::Expr::LoopBreak(expr) => self.loop_break(expr),
            ast::Expr::LoopContinue(expr) => self.loop_continue(expr),
            ast::Expr::FuncReturn(expr) => self.func_return(expr),
        }
    }

    fn exprs<'a>(&mut self, exprs: impl DoubleEndedIterator<Item = ast::Expr<'a>>) -> Option<()> {
        for expr in exprs {
            self.expr(expr);
        }
        Some(())
    }

    fn with_loop_info<F>(&mut self, span: Span, f: F) -> Option<()>
    where
        F: FnOnce(&mut Self) -> Option<()>,
    {
        let old = self.loop_info.take();
        self.loop_info = Some(LoopInfo { span });
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
            parent_loop: self.loop_info.clone(),
        });
        f(self);
        self.loop_info = self.func_info.take().expect("func info").parent_loop;
        self.func_info = old;
        Some(())
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
                    ast::Expr::SetRule(..) => "This set statement doesn't take effect.",
                    ast::Expr::ShowRule(..) => "This show statement doesn't take effect.",
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
            } else if matches!(it, ast::Expr::LoopBreak(..) | ast::Expr::LoopContinue(..)) {
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
            if let ast::Arg::Named(arg) = arg
                && arg.name().as_str() == "font"
            {
                self.check_variable_font_object(arg.expr().to_untyped());
                if let Some(array) = arg.expr().to_untyped().cast::<ast::Array>() {
                    for item in array.items() {
                        self.check_variable_font_object(item.to_untyped());
                    }
                }
            }
        }
    }

    fn check_variable_font_object(&mut self, expr: &SyntaxNode) -> Option<()> {
        if let Some(font_dict) = expr.cast::<ast::Dict>() {
            for item in font_dict.items() {
                if let ast::DictItem::Named(arg) = item
                    && arg.name().as_str() == "name"
                {
                    self.check_variable_font_str(arg.expr().to_untyped());
                }
            }
        }

        self.check_variable_font_str(expr)
    }
    fn check_variable_font_str(&mut self, expr: &SyntaxNode) -> Option<()> {
        if !expr.cast::<ast::Str>()?.get().ends_with("VF") {
            return None;
        }

        let _ = self.world;

        let diag =
            SourceDiagnostic::warning(expr.span(), "variable font is not supported by typst yet");
        let diag = diag.with_hint("consider using a static font instead. For more information, see https://github.com/typst/typst/issues/185");
        self.diag.push(diag);

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

            let body = expr.body();
            this.expr(body);
            cfg::lint_discarded_by_function_return(&mut this.diag, body);

            Some(())
        })
    }

    fn closure(&mut self, expr: ast::Closure<'_>) -> Option<()> {
        self.with_func_info(expr.span(), |this| {
            this.loop_info = None;
            this.exprs(expr.params().to_untyped().exprs());

            let body = expr.body();
            this.expr(body);
            cfg::lint_discarded_by_function_return(&mut this.diag, body);

            Some(())
        })
    }

    fn loop_break(&mut self, expr: ast::LoopBreak<'_>) -> Option<()> {
        if self.loop_info.is_none() {
            self.bad_branch_stmt(expr.to_untyped(), "break");
        }
        Some(())
    }

    fn loop_continue(&mut self, expr: ast::LoopContinue<'_>) -> Option<()> {
        if self.loop_info.is_none() {
            self.bad_branch_stmt(expr.to_untyped(), "continue");
        }
        Some(())
    }

    fn func_return(&mut self, expr: ast::FuncReturn<'_>) -> Option<()> {
        if self.func_info.is_none() {
            self.diag.push(SourceDiagnostic::warning(
                expr.span(),
                "`return` statement in a non-function context",
            ));
        }

        if let Some(body) = expr.body() {
            self.expr(body);
        }

        Some(())
    }

    fn binary(&mut self, expr: ast::Binary<'_>) -> Option<()> {
        self.check_type_compare(expr);
        self.exprs([expr.lhs(), expr.rhs()].into_iter())
    }

    fn func_call(&mut self, expr: ast::FuncCall<'_>) -> Option<()> {
        if expr.callee().to_untyped().text() == "text" {
            self.check_variable_font(expr.args().items());
        }
        self.exprs(expr.args().to_untyped().exprs().chain(expr.callee().once()));
        Some(())
    }

    fn math_ident(&mut self, ident: ast::MathIdent<'_>) -> Option<()> {
        let resolved = self.ei.get_def(&Interned::new(Decl::math_ident_ref(ident)));
        let is_defined = resolved.is_some_and(|expr| expr.is_defined());

        if !is_defined && !self.known_issues.has_unknown_math_ident(ident) {
            let var = ident.as_str();
            let mut warning =
                SourceDiagnostic::warning(ident.span(), eco_format!("unknown variable: {var}"));

            let in_global = self.world.library.global.scope().get(var).is_some();
            hint_unknown_variable_math(var, in_global, &mut warning);
            self.diag.push(warning);
        }

        Some(())
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
}

#[derive(Clone)]
struct FuncInfo {
    span: Span,
    is_contextual: bool,
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
            ast::Expr::CodeBlock(block) => Block::Code(block.body()),
            ast::Expr::ContentBlock(block) => Block::Markup(block.body()),
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
                if let ast::Expr::ShowRule(show) = show_set {
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
                if let ast::Expr::ShowRule(show) = show_set {
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
    matches!(it, ast::Expr::SetRule(..) | ast::Expr::ShowRule(..))
}

fn is_compare_op(op: ast::BinOp) -> bool {
    use ast::BinOp::*;
    matches!(op, Lt | Leq | Gt | Geq | Eq | Neq)
}

/// The error message when a variable wasn't found it math.
#[cold]
fn hint_unknown_variable_math(var: &str, in_global: bool, diag: &mut SourceDiagnostic) {
    if matches!(var, "none" | "auto" | "false" | "true") {
        diag.hint(eco_format!(
            "if you meant to use a literal, \
             try adding a hash before it: `#{var}`",
        ));
    } else if in_global {
        diag.hint(eco_format!(
            "`{var}` is not available directly in math, \
             try adding a hash before it: `#{var}`",
        ));
    } else {
        diag.hint(eco_format!(
            "if you meant to display multiple letters as is, \
             try adding spaces between each letter: `{}`",
            var.chars()
                .flat_map(|c| [' ', c])
                .skip(1)
                .collect::<EcoString>()
        ));
        diag.hint(eco_format!(
            "or if you meant to display this as text, \
             try placing it in quotes: `\"{var}\"`"
        ));
    }
}
