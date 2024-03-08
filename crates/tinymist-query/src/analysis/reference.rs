use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, SyntaxKind,
};
use typst_ts_core::typst::prelude::{eco_vec, EcoVec};

pub fn find_lexical_references_after<'a, 'b: 'a>(
    parent: LinkedNode<'a>,
    node: LinkedNode<'a>,
    target: &'b str,
) -> EcoVec<LinkedNode<'a>> {
    let mut worker = Worker {
        idents: eco_vec![],
        target,
    };
    worker.analyze_after(parent, node);

    worker.idents
}

struct Worker<'a> {
    target: &'a str,
    idents: EcoVec<LinkedNode<'a>>,
}

impl<'a> Worker<'a> {
    fn analyze_after(&mut self, parent: LinkedNode<'a>, node: LinkedNode<'a>) -> Option<()> {
        let mut after_node = false;

        for child in parent.children() {
            if child.offset() > node.offset() {
                after_node = true;
            }
            if after_node {
                self.analyze(child);
            }
        }

        None
    }

    fn analyze(&mut self, node: LinkedNode<'a>) -> Option<()> {
        match node.kind() {
            SyntaxKind::LetBinding => {
                let lb = node.cast::<ast::LetBinding>().unwrap();
                let name = lb.kind().idents();
                for n in name {
                    if n.get() == self.target {
                        return None;
                    }
                }

                if let Some(init) = lb.init() {
                    let init_expr = node.find(init.span())?;
                    self.analyze(init_expr);
                }
                return None;
            }
            // todo: analyze import effect
            SyntaxKind::Import => {}
            SyntaxKind::Ident | SyntaxKind::MathIdent => {
                if self.target == node.text() {
                    self.idents.push(node.clone());
                }
            }
            _ => {}
        }
        for child in node.children() {
            self.analyze(child);
        }

        None
    }
}
