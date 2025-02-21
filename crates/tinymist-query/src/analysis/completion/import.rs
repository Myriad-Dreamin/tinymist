//! Completion for import items.

use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Complete imports.
    pub fn complete_imports(&mut self) -> bool {
        // On the colon marker of an import list:
        // "#import "path.typ":|"
        if_chain! {
            if matches!(self.cursor.leaf.kind(), SyntaxKind::Colon);
            if let Some(parent) = self.cursor.leaf.clone().parent();
            if let Some(ast::Expr::Import(import)) = parent.get().cast();
            if !matches!(import.imports(), Some(ast::Imports::Wildcard));
            if let Some(source) = parent.children().find(|child| child.is::<ast::Expr>());
            then {
                let items = match import.imports() {
                    Some(ast::Imports::Items(items)) => items,
                    _ => Default::default(),
                };

                self.cursor.from = self.cursor.cursor;

                self.import_item_completions(items, vec![], &source);
                if items.iter().next().is_some() {
                    self.worker.enrich("", ", ");
                }
                return true;
            }
        }

        // Behind an import list:
        // "#import "path.typ": |",
        // "#import "path.typ": a, b, |".
        if_chain! {
            if let Some(prev) = self.cursor.leaf.prev_sibling();
            if let Some(ast::Expr::Import(import)) = prev.get().cast();
            if !self.cursor.text[prev.offset()..self.cursor.cursor].contains('\n');
            if let Some(ast::Imports::Items(items)) = import.imports();
            if let Some(source) = prev.children().find(|child| child.is::<ast::Expr>());
            then {
                self.  cursor.from = self.cursor.cursor;
                self.import_item_completions(items, vec![], &source);
                return true;
            }
        }

        // Behind a comma in an import list:
        // "#import "path.typ": this,|".
        if_chain! {
            if matches!(self.cursor.leaf.kind(), SyntaxKind::Comma);
            if let Some(parent) = self.cursor.leaf.clone().parent();
            if parent.kind() == SyntaxKind::ImportItems;
            if let Some(grand) = parent.parent();
            if let Some(ast::Expr::Import(import)) = grand.get().cast();
            if let Some(ast::Imports::Items(items)) = import.imports();
            if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
            then {
                self.import_item_completions(items, vec![], &source);
                self.worker.enrich(" ", "");
                return true;
            }
        }

        // Behind a half-started identifier in an import list:
        // "#import "path.typ": th|".
        if_chain! {
            if matches!(self.cursor.leaf.kind(), SyntaxKind::Ident | SyntaxKind::Dot);
            if let Some(path_ctx) = self.cursor.leaf.clone().parent();
            if path_ctx.kind() == SyntaxKind::ImportItemPath;
            if let Some(parent) = path_ctx.parent();
            if parent.kind() == SyntaxKind::ImportItems;
            if let Some(grand) = parent.parent();
            if let Some(ast::Expr::Import(import)) = grand.get().cast();
            if let Some(ast::Imports::Items(items)) = import.imports();
            if let Some(source) = grand.children().find(|child| child.is::<ast::Expr>());
            then {
                if self.cursor.leaf.kind() == SyntaxKind::Ident {
                    self.cursor.from = self.cursor.leaf.offset();
                }
                let path = path_ctx.cast::<ast::ImportItemPath>().map(|path| path.iter().take_while(|ident| ident.span() != self.cursor.leaf.span()).collect());
                self.import_item_completions( items, path.unwrap_or_default(), &source);
                return true;
            }
        }

        false
    }

    /// Add completions for all exports of a module.
    pub fn import_item_completions(
        &mut self,
        existing: ast::ImportItems,
        comps: Vec<ast::Ident>,
        source: &LinkedNode,
    ) {
        // Select the source by `comps`
        let value = self.worker.ctx.module_by_syntax(source);
        let value = comps
            .iter()
            .fold(value.as_ref(), |value, comp| value?.scope()?.get(comp));
        let Some(scope) = value.and_then(|v| v.scope()) else {
            return;
        };

        // Check imported items in the scope
        let seen = existing
            .iter()
            .flat_map(|item| {
                let item_comps = item.path().iter().collect::<Vec<_>>();
                if item_comps.len() == comps.len() + 1
                    && item_comps
                        .iter()
                        .zip(comps.as_slice())
                        .all(|(l, r)| l.as_str() == r.as_str())
                {
                    // item_comps.len() >= 1
                    item_comps.last().cloned()
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if existing.iter().next().is_none() {
            self.snippet_completion("*", "*", "Import everything.");
        }

        for (name, value, _) in scope.iter() {
            if seen.iter().all(|item| item.as_str() != name) {
                self.value_completion(Some(name.clone()), value, false, None);
            }
        }
    }
}
