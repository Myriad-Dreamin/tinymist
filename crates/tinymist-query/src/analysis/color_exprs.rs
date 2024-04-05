//! Analyze color expressions in a source file.
use std::{ops::Range, str::FromStr};

use lsp_types::ColorInformation;
use typst::{
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Source, SyntaxKind,
    },
    visualize::Color,
};

use crate::AnalysisContext;

/// Get color expressions from a source.
pub fn get_color_exprs(ctx: &mut AnalysisContext, src: &Source) -> Option<Vec<ColorInformation>> {
    let mut worker = ColorExprWorker {
        ctx,
        source: src.clone(),
        colors: vec![],
    };
    let root = LinkedNode::new(src.root());
    worker.collect_colors(root)?;
    Some(worker.colors)
}

struct ColorExprWorker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    source: Source,
    colors: Vec<ColorInformation>,
}

impl<'a, 'w> ColorExprWorker<'a, 'w> {
    fn collect_colors(&mut self, node: LinkedNode) -> Option<()> {
        match node.kind() {
            SyntaxKind::FuncCall => {
                let fc = self.analyze_call(node.clone());
                if fc.is_some() {
                    return Some(());
                }
            }
            SyntaxKind::Named => {}
            k if k.is_trivia() || k.is_keyword() || k.is_error() => return Some(()),
            _ => {}
        };

        for child in node.children() {
            self.collect_colors(child);
        }

        Some(())
    }

    fn analyze_call(&mut self, node: LinkedNode) -> Option<()> {
        let call = node.cast::<ast::FuncCall>()?;
        let mut callee = call.callee();
        'check_color_fn: loop {
            match callee {
                ast::Expr::FieldAccess(fa) => {
                    let target = fa.target();
                    let ast::Expr::Ident(ident) = target else {
                        return None;
                    };
                    if ident.get().as_str() != "color" {
                        return None;
                    }
                    callee = ast::Expr::Ident(fa.field());
                    continue 'check_color_fn;
                }
                ast::Expr::Ident(ident) => {
                    // currently support rgb, luma
                    match ident.get().as_str() {
                        "rgb" => self.analyze_rgb(&node, call)?,
                        "luma" | "oklab" | "oklch" | "linear-rgb" | "cmyk" | "hsl" | "hsv" => {
                            self.analyze_general(&node, call)?
                        }
                        _ => return None,
                    }
                }
                _ => return None,
            }
            return None;
        }
    }

    fn analyze_rgb(&mut self, node: &LinkedNode, call: ast::FuncCall) -> Option<()> {
        let mut args = call.args().items();
        let hex_or_color_or_r = args.next()?;
        let g = args.next();
        match (g.is_some(), hex_or_color_or_r) {
            (true, _) => self.analyze_general(node, call)?,
            (false, ast::Arg::Pos(ast::Expr::Str(s))) => {
                // parse hex
                let color = typst::visualize::Color::from_str(s.get().as_str()).ok()?;
                let arg = node.find(hex_or_color_or_r.span())?;
                self.push_color(arg.range(), color);
            }
            (false, _) => {}
        }

        Some(())
    }

    fn analyze_general(&mut self, node: &LinkedNode, call: ast::FuncCall) -> Option<()> {
        let color = self.ctx.mini_eval(ast::Expr::FuncCall(call))?.cast().ok()?;
        self.push_color(node.range(), color);
        Some(())
    }

    fn push_color(&mut self, range: Range<usize>, color: Color) -> Option<()> {
        let rng = self.ctx.to_lsp_range(range, &self.source);
        let [r, g, b, a] = color.to_rgb().to_vec4();

        self.colors.push(ColorInformation {
            range: rng,
            color: lsp_types::Color {
                red: r,
                green: g,
                blue: b,
                alpha: a,
            },
        });

        Some(())
    }
}
