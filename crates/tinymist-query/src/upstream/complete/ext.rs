use super::{Completion, CompletionContext, CompletionKind};
use std::collections::BTreeMap;

use typst::foundations::Value;
use typst::syntax::{ast, SyntaxKind};

use crate::analysis::analyze_import;

impl<'a> CompletionContext<'a> {
    /// Add completions for definitions that are available at the cursor.
    ///
    /// Filters the global/math scope with the given filter.
    pub fn scope_completions_(&mut self, parens: bool, filter: impl Fn(&Value) -> bool) {
        let mut defined = BTreeMap::new();

        let mut ancestor = Some(self.leaf.clone());
        while let Some(node) = &ancestor {
            let mut sibling = Some(node.clone());
            while let Some(node) = &sibling {
                if let Some(v) = node.cast::<ast::LetBinding>() {
                    let kind = match v.kind() {
                        ast::LetBindingKind::Closure(..) => CompletionKind::Func,
                        ast::LetBindingKind::Normal(..) => CompletionKind::Variable,
                    };
                    for ident in v.kind().bindings() {
                        defined.insert(ident.get().clone(), kind.clone());
                    }
                }

                if let Some(v) = node.cast::<ast::ModuleImport>() {
                    let imports = v.imports();
                    match imports {
                        None | Some(ast::Imports::Wildcard) => {
                            if let Some(value) = node
                                .children()
                                .find(|child| child.is::<ast::Expr>())
                                .and_then(|source| analyze_import(self.world, &source))
                            {
                                if imports.is_none() {
                                    // todo: correct kind
                                    defined.extend(
                                        value
                                            .name()
                                            .map(Into::into)
                                            .map(|e| (e, CompletionKind::Variable)),
                                    );
                                } else if let Some(scope) = value.scope() {
                                    for (name, _) in scope.iter() {
                                        defined.insert(name.clone(), CompletionKind::Variable);
                                    }
                                }
                            }
                        }
                        Some(ast::Imports::Items(items)) => {
                            for item in items.iter() {
                                defined.insert(
                                    item.bound_name().get().clone(),
                                    CompletionKind::Variable,
                                );
                            }
                        }
                    }
                }

                sibling = node.prev_sibling();
            }

            if let Some(parent) = node.parent() {
                if let Some(v) = parent.cast::<ast::ForLoop>() {
                    if node.prev_sibling_kind() != Some(SyntaxKind::In) {
                        let pattern = v.pattern();
                        for ident in pattern.bindings() {
                            defined.insert(ident.get().clone(), CompletionKind::Variable);
                        }
                    }
                }

                ancestor = Some(parent.clone());
                continue;
            }

            break;
        }

        let in_math = matches!(
            self.leaf.parent_kind(),
            Some(SyntaxKind::Equation)
                | Some(SyntaxKind::Math)
                | Some(SyntaxKind::MathFrac)
                | Some(SyntaxKind::MathAttach)
        );

        let scope = if in_math { self.math } else { self.global };
        for (name, value) in scope.iter() {
            if filter(value) && !defined.contains_key(name) {
                self.value_completion(Some(name.clone()), value, parens, None);
            }
        }

        for (name, kind) in defined {
            if !name.is_empty() {
                self.completions.push(Completion {
                    kind,
                    label: name,
                    apply: None,
                    detail: None,
                });
            }
        }
    }
}
