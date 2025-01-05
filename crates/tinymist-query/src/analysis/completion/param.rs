//! Completion for param items.
//!
//! Note, this is used for the completion of parameters on a function's
//! *definition* instead of the completion of arguments of some *function call*.

use typst::eval::CapturesVisitor;

use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Complete parameters.
    pub fn complete_params(&mut self) -> Option<()> {
        self.cursor.from = self.cursor.leaf.offset();

        let leaf = self.cursor.leaf.clone();
        let closure_node = node_ancestors(&leaf).find(|node| node.kind() == SyntaxKind::Closure)?;

        let mut bindings = HashSet::<EcoString>::default();

        let closure_node = closure_node.cast::<ast::Closure>()?;

        // The function references itself is common in typst.
        let name = closure_node.name();
        if let Some(name) = name {
            bindings.insert(name.get().clone());
        }

        // Collects all bindings from the parameters.
        let param_list = closure_node.params();
        for param in param_list.children() {
            match param {
                ast::Param::Pos(pos) => {
                    for name in pos.bindings() {
                        bindings.insert(name.get().clone());
                    }
                }
                ast::Param::Named(named) => {
                    bindings.insert(named.name().get().clone());
                }
                ast::Param::Spread(spread) => {
                    if let Some(ident) = spread.sink_ident() {
                        bindings.insert(ident.get().clone());
                    }
                }
            }
        }

        let mut visitor = CapturesVisitor::new(None, typst::foundations::Capturer::Function);
        visitor.visit(closure_node.body().to_untyped());
        let captures = visitor.finish();

        // Converts the captures into completions.
        for (name, value, _) in captures.iter() {
            if !bindings.contains(name) {
                let docs = "Parametrizes the captured variable.";
                self.value_completion(Some(name.clone()), value, false, Some(docs));
            }
        }

        Some(())
    }
}
