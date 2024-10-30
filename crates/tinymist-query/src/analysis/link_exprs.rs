//! Analyze color expressions in a source file.

use lsp_types::Url;

use super::prelude::*;
use crate::path_to_url;

/// Get link expressions from a source.
pub fn get_link_exprs(ctx: &mut LocalContext, src: &Source) -> Option<Vec<(Range<usize>, Url)>> {
    let root = LinkedNode::new(src.root());
    get_link_exprs_in(ctx, &root)
}

/// Get link expressions in a source node.
pub fn get_link_exprs_in(
    ctx: &mut LocalContext,
    node: &LinkedNode,
) -> Option<Vec<(Range<usize>, Url)>> {
    let mut worker = LinkStrWorker { ctx, links: vec![] };
    worker.collect_links(node)?;
    Some(worker.links)
}

struct LinkStrWorker<'a> {
    ctx: &'a mut LocalContext,
    links: Vec<(Range<usize>, Url)>,
}

impl<'a> LinkStrWorker<'a> {
    fn collect_links(&mut self, node: &LinkedNode) -> Option<()> {
        match node.kind() {
            // SyntaxKind::Link => { }
            SyntaxKind::FuncCall => {
                let fc = self.analyze_call(node);
                if fc.is_some() {
                    return Some(());
                }
            }
            // early exit
            k if k.is_trivia() || k.is_keyword() || k.is_error() => return Some(()),
            _ => {}
        };

        for child in node.children() {
            self.collect_links(&child);
        }

        Some(())
    }

    fn analyze_call(&mut self, node: &LinkedNode) -> Option<()> {
        let call = node.cast::<ast::FuncCall>()?;
        let mut callee = call.callee();
        'check_link_fn: loop {
            match callee {
                ast::Expr::FieldAccess(fa) => {
                    let target = fa.target();
                    let ast::Expr::Ident(ident) = target else {
                        return None;
                    };
                    if ident.get().as_str() != "std" {
                        return None;
                    }
                    callee = ast::Expr::Ident(fa.field());
                    continue 'check_link_fn;
                }
                ast::Expr::Ident(ident) => match ident.get().as_str() {
                    "raw" => {
                        self.analyze_reader(node, call, "theme", false);
                        self.analyze_reader(node, call, "syntaxes", false);
                    }
                    "bibliography" => {
                        self.analyze_reader(node, call, "cite", false);
                        self.analyze_reader(node, call, "style", false);
                        self.analyze_reader(node, call, "path", true);
                    }
                    "cbor" | "csv" | "image" | "read" | "json" | "yaml" | "xml" => {
                        self.analyze_reader(node, call, "path", true);
                    }
                    _ => return None,
                },
                _ => return None,
            }
            return None;
        }
    }

    fn analyze_reader(
        &mut self,
        node: &LinkedNode,
        call: ast::FuncCall,
        key: &str,
        pos: bool,
    ) -> Option<()> {
        let arg = call.args().items().next()?;
        match arg {
            ast::Arg::Pos(s) if pos => {
                self.analyze_path_exp(node, s);
            }
            _ => {}
        }
        for item in call.args().items() {
            match item {
                ast::Arg::Named(named) if named.name().get().as_str() == key => {
                    self.analyze_path_exp(node, named.expr());
                }
                _ => {}
            }
        }
        Some(())
    }

    fn analyze_path_exp(&mut self, node: &LinkedNode, expr: ast::Expr) -> Option<()> {
        match expr {
            ast::Expr::Str(s) => self.analyze_path_str(node, s),
            ast::Expr::Array(a) => {
                for item in a.items() {
                    if let ast::ArrayItem::Pos(ast::Expr::Str(s)) = item {
                        self.analyze_path_str(node, s);
                    }
                }
                Some(())
            }
            _ => None,
        }
    }

    fn analyze_path_str(&mut self, node: &LinkedNode, s: ast::Str<'_>) -> Option<()> {
        let str_node = node.find(s.span())?;
        let str_range = str_node.range();
        let content_range = str_range.start + 1..str_range.end - 1;
        if content_range.is_empty() {
            return None;
        }

        // Avoid creating new ids here.
        let id = node.span().id()?;
        let base = id.vpath().join(s.get().as_str());
        let root = self.ctx.path_for_id(id.join("/")).ok()?;
        let path = base.resolve(&root)?;
        if !path.exists() {
            return None;
        }

        self.push_path(content_range, path.as_path())
    }

    fn push_path(&mut self, range: Range<usize>, path: &Path) -> Option<()> {
        self.push_link(range, path_to_url(path).ok()?)
    }

    fn push_link(&mut self, range: Range<usize>, target: Url) -> Option<()> {
        // let rng = self.ctx.to_lsp_range(range, &self.source);

        self.links.push((range, target));

        Some(())
    }
}
