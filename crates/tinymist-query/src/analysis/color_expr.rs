//! Analyze color expressions in a source file.
use std::str::FromStr;

use typst::visualize::Color;

use crate::prelude::*;

/// Analyzes the document and provides color information.
pub struct ColorExprWorker<'a> {
    /// The local analysis context to work with.
    ctx: &'a mut LocalContext,
    /// The source document to analyze.
    source: Source,
    /// The color information to provide.
    pub colors: Vec<ColorInformation>,
}

impl<'a> ColorExprWorker<'a> {
    /// Creates a new color expression worker.
    pub fn new(ctx: &'a mut LocalContext, source: Source) -> Self {
        Self {
            ctx,
            source,
            colors: vec![],
        }
    }

    /// Starts to work.
    pub fn work(&mut self, node: LinkedNode) -> Option<()> {
        match node.kind() {
            SyntaxKind::FuncCall => {
                let fc = self.on_call(node.clone());
                if fc.is_some() {
                    return Some(());
                }
            }
            SyntaxKind::Named => {}
            kind if kind.is_trivia() || kind.is_keyword() || kind.is_error() => return Some(()),
            _ => {}
        };

        for child in node.children() {
            self.work(child);
        }

        Some(())
    }

    fn on_call(&mut self, node: LinkedNode) -> Option<()> {
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
                        "rgb" => self.on_rgb(&node, call)?,
                        "luma" | "oklab" | "oklch" | "linear-rgb" | "cmyk" | "hsl" | "hsv" => {
                            self.on_const_call(&node, call)?
                        }
                        _ => return None,
                    }
                }
                _ => return None,
            }
            return None;
        }
    }

    fn on_rgb(&mut self, node: &LinkedNode, call: ast::FuncCall) -> Option<()> {
        let mut args = call.args().items();
        let hex_or_color_or_r = args.next()?;
        let arg = args.next();
        match (arg.is_some(), hex_or_color_or_r) {
            (true, _) => self.on_const_call(node, call)?,
            (false, ast::Arg::Pos(ast::Expr::Str(s))) => {
                // parse hex
                let color = typst::visualize::Color::from_str(s.get().as_str()).ok()?;
                // todo: smarter
                // let arg = node.find(hex_or_color_or_r.span())?;
                let arg = node.find(call.span())?;
                self.push_color(arg.range(), color);
            }
            (false, _) => {}
        }

        Some(())
    }

    fn on_const_call(&mut self, node: &LinkedNode, call: ast::FuncCall) -> Option<()> {
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
