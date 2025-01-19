//! Analyze link expressions in a source file.

use std::str::FromStr;

use lsp_types::Url;
use tinymist_world::package::PackageSpec;

use super::prelude::*;

/// Get link expressions from a source.
#[comemo::memoize]
pub fn get_link_exprs(src: &Source) -> Arc<LinkInfo> {
    let root = LinkedNode::new(src.root());
    Arc::new(get_link_exprs_in(&root).unwrap_or_default())
}

/// Get link expressions in a source node.
pub fn get_link_exprs_in(node: &LinkedNode) -> Option<LinkInfo> {
    let mut worker = LinkStrWorker {
        info: LinkInfo::default(),
    };
    worker.collect_links(node)?;
    Some(worker.info)
}

/// A valid link target.
pub enum LinkTarget {
    /// A package specification.
    Package(Box<PackageSpec>),
    /// A URL.
    Url(Box<Url>),
    /// A file path.
    Path(TypstFileId, EcoString),
}

impl LinkTarget {
    pub(crate) fn resolve(&self, ctx: &mut LocalContext) -> Option<Url> {
        match self {
            LinkTarget::Package(..) => None,
            LinkTarget::Url(url) => Some(url.as_ref().clone()),
            LinkTarget::Path(id, path) => {
                // Avoid creating new ids here.
                let root = ctx.path_for_id(id.join("/")).ok()?;
                crate::path_res_to_url(root.join(path).ok()?).ok()
            }
        }
    }
}

/// A link object in a source file.
pub struct LinkObject {
    /// The range of the link expression.
    pub range: Range<usize>,
    /// The span of the link expression.
    pub span: Span,
    /// The target of the link.
    pub target: LinkTarget,
}

/// Link information in a source file.
#[derive(Default)]
pub struct LinkInfo {
    /// The link objects in a source file.
    pub objects: Vec<LinkObject>,
}

struct LinkStrWorker {
    info: LinkInfo,
}

impl LinkStrWorker {
    fn collect_links(&mut self, node: &LinkedNode) -> Option<()> {
        match node.kind() {
            // SyntaxKind::Link => { }
            SyntaxKind::FuncCall => {
                let fc = self.analyze_call(node);
                if fc.is_some() {
                    return Some(());
                }
            }
            SyntaxKind::Include => {
                let inc = node.cast::<ast::ModuleInclude>()?;
                let path = inc.source();
                self.analyze_path_expr(node, path);
            }
            // early exit
            kind if kind.is_trivia() || kind.is_keyword() || kind.is_error() => return Some(()),
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
                self.analyze_path_expr(node, s);
            }
            _ => {}
        }
        for item in call.args().items() {
            match item {
                ast::Arg::Named(named) if named.name().get().as_str() == key => {
                    self.analyze_path_expr(node, named.expr());
                }
                _ => {}
            }
        }
        Some(())
    }

    fn analyze_path_expr(&mut self, node: &LinkedNode, path_expr: ast::Expr) -> Option<()> {
        match path_expr {
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
        let range = str_range.start + 1..str_range.end - 1;
        if range.is_empty() {
            return None;
        }

        let content = s.get();
        if content.starts_with('@') {
            let pkg_spec = PackageSpec::from_str(&content).ok()?;
            self.info.objects.push(LinkObject {
                range,
                span: s.span(),
                target: LinkTarget::Package(Box::new(pkg_spec)),
            });
            return Some(());
        }

        let id = node.span().id()?;
        self.info.objects.push(LinkObject {
            range,
            span: s.span(),
            target: LinkTarget::Path(id, content),
        });
        Some(())
    }
}
